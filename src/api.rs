//! Shared read helpers for the REST API surface (Phase 16, D-04).
//!
//! The ACL resolution for the read surface lives HERE and only here: MCP
//! tools and REST handlers both call these free helpers, so the two
//! transports can never drift. An invisible vault yields `NotFound` —
//! never a 403-shaped, existence-leaking reason (D-02).

use std::path::{Path, PathBuf};

use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router};

use crate::server::SharedState;
use crate::store::{AccessibleVault, Store, User};

/// Typed reason a read failed. Each transport maps these to its own error
/// shape: MCP to the legacy strings via [`ApiReadError::mcp_message`], REST
/// to RFC 9457 problems (wave 2/3). `NotFound` deliberately covers BOTH
/// unknown slug and no-access so nothing leaks vault existence (D-02).
#[derive(Debug, Clone, Copy)]
pub(crate) enum ApiReadError {
    /// Vault invisible to this user (unknown slug OR no access — D-02).
    NotFound,
    /// Traversal / absolute path refused by `safe_join`.
    InvalidPath,
    /// Requested sub-folder does not exist inside the vault.
    NoSuchFolder,
    /// Requested note file does not exist inside the vault.
    NoSuchNote,
    /// File exists but could not be read.
    Unreadable,
}

impl ApiReadError {
    /// The EXACT legacy MCP error strings (test-asserted verbatim at
    /// mcp.rs `mcp_hidden_share_invisible` / `mcp_revoked_share_denied`).
    pub(crate) fn mcp_message(&self) -> &'static str {
        match self {
            ApiReadError::NotFound => "vault not found or access denied",
            ApiReadError::InvalidPath => "invalid path",
            ApiReadError::NoSuchFolder => "no such folder",
            ApiReadError::NoSuchNote => "no such note",
            ApiReadError::Unreadable => "could not read the note",
        }
    }
}

/// Every vault the user can access (personal + shares). Store errors degrade
/// to an empty list, same as the legacy MCP bodies (`unwrap_or_default`).
pub(crate) async fn accessible_vault_entries(store: &Store, email: &str) -> Vec<AccessibleVault> {
    store.accessible_vaults(email).await.unwrap_or_default()
}

/// Vault JSON with EXACTLY the MCP field names — slug/name/scope/role, no
/// envelope (D-08). REST serves the same object (D-08 field parity).
pub(crate) fn vault_json(a: &AccessibleVault) -> serde_json::Value {
    serde_json::json!({
        "slug": a.vault.slug,
        "name": a.vault.name,
        "scope": a.vault.scope,
        "role": a.role,
    })
}

/// The source directory of an accessible vault (same resolution as
/// WebDAV: the markdown source, never the generated tree — D-06). One
/// generic `None` for unknown slug and no-access — no existence leak (D-02).
pub(crate) async fn source_dir(store: &Store, email: &str, slug: &str) -> Option<PathBuf> {
    let entry = accessible_vault_entries(store, email)
        .await
        .into_iter()
        .find(|a| a.vault.slug == slug)?;
    crate::webdav::source_dir_of(Path::new(&entry.vault.book_path))
}

/// Sorted `.md` paths under `sub` (or the vault root) of an accessible
/// vault. Sorted is load-bearing: the wave-3 cursor pages over exactly
/// this order (D-09).
pub(crate) async fn list_notes(
    store: &Store,
    email: &str,
    slug: &str,
    sub: Option<&str>,
) -> Result<Vec<String>, ApiReadError> {
    let src = source_dir(store, email, slug)
        .await
        .ok_or(ApiReadError::NotFound)?;
    let base = match sub {
        Some(s) => crate::mcp::safe_join(&src, s).ok_or(ApiReadError::InvalidPath)?,
        None => src,
    };
    if !base.is_dir() {
        return Err(ApiReadError::NoSuchFolder);
    }
    Ok(crate::mcp::list_md_files(&base))
}

/// Raw bytes of one markdown note (REST needs bytes for `webdav::etag_of`;
/// the MCP wrapper converts to String itself).
pub(crate) async fn read_note(
    store: &Store,
    email: &str,
    slug: &str,
    path: &str,
) -> Result<Vec<u8>, ApiReadError> {
    let src = source_dir(store, email, slug)
        .await
        .ok_or(ApiReadError::NotFound)?;
    let full = crate::mcp::safe_join(&src, path).ok_or(ApiReadError::InvalidPath)?;
    if !full.is_file() {
        return Err(ApiReadError::NoSuchNote);
    }
    std::fs::read(&full).map_err(|_| ApiReadError::Unreadable)
}

/// Case-insensitive substring hits across accessible vaults (or one slug),
/// capped at 50 (D-10, mirrors the MCP `search_md` cap).
pub(crate) async fn search_notes(
    store: &Store,
    email: &str,
    query: &str,
    slug: Option<&str>,
) -> Result<Vec<crate::mcp::Hit>, ApiReadError> {
    let vaults: Vec<(String, PathBuf)> = match slug {
        Some(s) => match source_dir(store, email, s).await {
            Some(dir) => vec![(s.to_string(), dir)],
            None => return Err(ApiReadError::NotFound),
        },
        None => accessible_vault_entries(store, email)
            .await
            .iter()
            .filter_map(|a| {
                crate::webdav::source_dir_of(Path::new(&a.vault.book_path))
                    .map(|dir| (a.vault.slug.clone(), dir))
            })
            .collect(),
    };
    Ok(crate::mcp::search_md(&vaults, query, 50))
}

// ---- RFC 9457 problem+json + Bearer gate (16-02, D-01/D-03) ----

/// The ONE error constructor for the whole `/api/v1` surface (D-03). Every
/// error path — middleware 401, unknown route 404, wave-3 400/404/429 —
/// routes through here so the surface has a single error shape. `code`
/// comes from the CLOSED vocabulary fixed by the plan: `unauthorized` (401),
/// `not_found` (404), `invalid_request` (400), `precondition_failed` (400),
/// `rate_limited` (429), `internal_error` (500) — no other codes exist.
/// `detail` is a fixed human string: NEVER echo the token or vault content
/// (B2). Nothing here logs anything (B2 grep gate).
pub(crate) fn problem(status: StatusCode, code: &str, detail: &str) -> Response {
    (
        status,
        [(CONTENT_TYPE, "application/problem+json")],
        Json(serde_json::json!({
            "type": "about:blank",
            "title": problem_title(code),
            "status": status.as_u16(),
            "detail": detail,
            "code": code,
        })),
    )
        .into_response()
}

/// RFC 9457 `title` from the code slug: `not_found` → "Not Found".
fn problem_title(code: &str) -> String {
    code.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Bearer-only gate for `/api/v1` (D-01). Reads ONLY the AUTHORIZATION
/// header — session cookies are structurally never consulted, so a browser
/// session can never authenticate the API (Avoids B4). The resolved `User`
/// is handed to handlers via request extensions (the `require_dav_auth`
/// `Claims`-insert pattern, server.rs:350-353); handlers must NOT re-look-up
/// the token.
async fn require_api_bearer(
    State(state): State<SharedState>,
    mut req: Request,
    next: Next,
) -> Response {
    let bearer = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());
    let user = match bearer {
        Some(token) => state
            .store
            .user_for_api_token(&token)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let Some(user) = user else {
        let mut res = problem(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "a valid Bearer API token is required (create one on /account)",
        );
        res.headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return res;
    };
    req.extensions_mut().insert(user);
    next.run(req).await
}

/// `ANY /api/v1/{*rest}` catch-all for unknown API paths: one uniform
/// `not_found` problem instead of axum's bare-status default 404 (D-03).
async fn api_not_found() -> Response {
    problem(
        StatusCode::NOT_FOUND,
        "not_found",
        "no such API endpoint",
    )
}

/// The `/api/v1` group (API-01): a standalone merged router with its own
/// Bearer `route_layer` — NEVER inside the `protected` group, so
/// `require_session` cookies can never authenticate it (D-01 / Avoids B4).
///
/// Wave 3 appends the note/search endpoint routes BEFORE the catch-all
/// route (static/param routes win over `{*rest}` in matchit) and keeps
/// everything inside the single `route_layer` below.
pub fn router(state: &SharedState) -> Router {
    Router::new()
        // Registered BEFORE route_layer so it sits INSIDE the Bearer group:
        // unknown paths answer 401 without a token, 404 problem+json with
        // one. NEVER Router::fallback — a merged router's fallback becomes
        // the whole-app fallback and would change site-wide non-API 404s.
        .route("/api/v1/{*rest}", any(api_not_found))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_api_bearer,
        ))
        .with_state(state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::path::PathBuf;

    async fn store() -> Store {
        Store::connect_memory().await.expect("memory store")
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morinet-api-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Admin-owned vault seeded with `note.md`; returns (store, src_dir).
    async fn seed_vault(tag: &str, slug: &str) -> (Store, PathBuf) {
        let s = store().await;
        s.ensure_admin("admin@api.test", "pw").await.unwrap();
        let src = tmpdir(tag);
        std::fs::write(src.join("note.md"), "# Note\ncontent").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault(slug, "API vault", &src.join("book"))
            .await
            .unwrap();
        (s, src)
    }

    #[tokio::test]
    async fn helper_source_dir_invisible_vault_is_none() {
        let (s, _src) = seed_vault("srcdir", "sv1").await;
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        // Visible to the admin, invisible to a stranger: None, never an error.
        assert!(source_dir(&s, "admin@api.test", "sv1").await.is_some());
        assert!(source_dir(&s, "stranger@api.test", "sv1").await.is_none());
        assert!(source_dir(&s, "admin@api.test", "nope").await.is_none());
    }

    #[tokio::test]
    async fn helper_list_notes_not_found_vs_invalid_path() {
        let (s, _src) = seed_vault("listnotes", "ln1").await;
        // Unknown slug → NotFound (same shape as no-access: D-02).
        assert!(matches!(
            list_notes(&s, "admin@api.test", "unknown", None).await,
            Err(ApiReadError::NotFound)
        ));
        // Traversal sub → InvalidPath, a DIFFERENT reason.
        assert!(matches!(
            list_notes(&s, "admin@api.test", "ln1", Some("../x")).await,
            Err(ApiReadError::InvalidPath)
        ));
        // Happy path: sorted .md list of the source root.
        let files = list_notes(&s, "admin@api.test", "ln1", None).await.unwrap();
        assert_eq!(files, vec!["note.md".to_string()]);
        // Missing sub-folder → NoSuchFolder.
        assert!(matches!(
            list_notes(&s, "admin@api.test", "ln1", Some("ghost")).await,
            Err(ApiReadError::NoSuchFolder)
        ));
    }

    #[tokio::test]
    async fn helper_read_note_maps_reasons() {
        let (s, _src) = seed_vault("readnote", "rn1").await;
        // Missing file → NoSuchNote.
        assert!(matches!(
            read_note(&s, "admin@api.test", "rn1", "ghost.md").await,
            Err(ApiReadError::NoSuchNote)
        ));
        // Traversal → InvalidPath.
        assert!(matches!(
            read_note(&s, "admin@api.test", "rn1", "../secret.md").await,
            Err(ApiReadError::InvalidPath)
        ));
        // Invisible vault → NotFound.
        assert!(matches!(
            read_note(&s, "admin@api.test", "nope", "note.md").await,
            Err(ApiReadError::NotFound)
        ));
        // Happy path: raw bytes of the source note.
        let bytes = read_note(&s, "admin@api.test", "rn1", "note.md")
            .await
            .unwrap();
        assert_eq!(bytes, b"# Note\ncontent");
    }

    #[tokio::test]
    async fn helper_search_scoped_to_accessible() {
        let s = store().await;
        s.create_user("owner@api.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("sharee@api.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        let src = tmpdir("search");
        std::fs::write(src.join("note.md"), "# Shared\nsecret needle content").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault("ss1", "Shared vault", &src.join("book"))
            .await
            .unwrap();
        s.share_vault("ss1", "sharee@api.test", "reader")
            .await
            .unwrap();

        // Sharee finds the hit in the shared vault.
        let hits = search_notes(&s, "sharee@api.test", "secret needle", None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "ss1");
        // Stranger finds nothing.
        let none = search_notes(&s, "stranger@api.test", "secret needle", None)
            .await
            .unwrap();
        assert!(none.is_empty());
        // Scoped search on an invisible slug → NotFound.
        assert!(matches!(
            search_notes(&s, "stranger@api.test", "secret needle", Some("ss1")).await,
            Err(ApiReadError::NotFound)
        ));
    }

    // ---- router-level harness (16-02: Bearer gate + problem+json) ----

    use crate::config::RuntimeConfig;
    use crate::email::InMemorySender;
    use crate::server::build_router_with_email;
    use axum::body::{to_bytes, Body};
    use axum::extract::Request;
    use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, WWW_AUTHENTICATE};
    use axum::http::StatusCode;
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt; // oneshot

    fn router_cfg() -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            base_url: "http://localhost:8080".to_string(),
            cookie_secret: b"test-secret-0123456789abcdef".to_vec(),
            cookie_name: "morinet_session".to_string(),
            admin_email: "admin@api.test".to_string(),
            admin_password: "pw".to_string(),
            smtp: Default::default(),
            data_dir: std::env::temp_dir().join("morinet-api-test-data"),
            vault_book_dir: None,
            locked_accounts: Vec::new(),
        }
    }

    /// Full app router (same wiring as production) + its store, so tests can
    /// mint real API tokens against it.
    async fn api_app() -> (axum::Router, Store) {
        let s = store().await;
        s.ensure_admin("admin@api.test", "pw").await.unwrap();
        let app = build_router_with_email(
            router_cfg(),
            s.clone(),
            Arc::new(InMemorySender::new()),
        );
        (app, s)
    }

    async fn mint_token(s: &Store, email: &str) -> String {
        let uid = s.verify_login(email, "pw").await.unwrap().unwrap().id;
        let (_id, plain) = s.create_api_token(uid, "api-test").await.unwrap();
        plain
    }

    async fn get(
        app: axum::Router,
        uri: &str,
        bearer: Option<&str>,
        cookie: Option<&str>,
    ) -> Response {
        let mut builder = Request::builder().method("GET").uri(uri);
        if let Some(b) = bearer {
            builder = builder.header(AUTHORIZATION, format!("Bearer {b}"));
        }
        if let Some(c) = cookie {
            builder = builder.header(COOKIE, c);
        }
        app.oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body_json(res: Response) -> serde_json::Value {
        let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
        serde_json::from_slice(&bytes).expect("JSON body")
    }

    #[tokio::test]
    async fn api_requires_bearer() {
        let (app, _s) = api_app().await;
        let res = get(app, "/api/v1/vaults", None, None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "no Bearer → 401");
        assert_eq!(
            res.headers()[CONTENT_TYPE],
            "application/problem+json",
            "401 body is RFC 9457 problem+json (D-03)"
        );
        assert_eq!(
            res.headers()[WWW_AUTHENTICATE], "Bearer",
            "401 carries the Bearer challenge"
        );
        let j = body_json(res).await;
        assert_eq!(j["type"], "about:blank");
        assert_eq!(j["code"], "unauthorized");
        assert_eq!(j["status"], 401);
        // B2: fixed detail string — never echoes any submitted token.
        assert_eq!(
            j["detail"].as_str().unwrap(),
            "a valid Bearer API token is required (create one on /account)"
        );
    }

    #[tokio::test]
    async fn api_rejects_forged_and_revoked_tokens() {
        let (app, s) = api_app().await;
        let res = get(app.clone(), "/api/v1/vaults", Some("mn_forged"), None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "forged → 401");
        assert_eq!(
            res.headers()[CONTENT_TYPE],
            "application/problem+json",
            "forged 401 is problem+json too"
        );

        let uid = s
            .verify_login("admin@api.test", "pw")
            .await
            .unwrap()
            .unwrap()
            .id;
        let (id, plain) = s.create_api_token(uid, "doomed").await.unwrap();
        // Gate passes on a live token (endpoint returns 200 once 16-02 task 2
        // lands; here the Bearer-gated catch-all answers 404, never 401).
        let res = get(app.clone(), "/api/v1/vaults", Some(&plain), None).await;
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED, "live token passes gate");

        s.revoke_api_token(uid, id).await.unwrap();
        let res = get(app, "/api/v1/vaults", Some(&plain), None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "revoked → 401");
    }

    #[tokio::test]
    async fn api_ignores_session_cookie() {
        // B4 / D-01: cookies can never authenticate the API, and a request
        // carrying BOTH succeeds on the Bearer alone.
        let (app, s) = api_app().await;
        let res = get(
            app.clone(),
            "/api/v1/vaults",
            None,
            Some("morinet_session=bogus-cookie-value"),
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "session cookie alone → 401"
        );

        let plain = mint_token(&s, "admin@api.test").await;
        let res = get(
            app,
            "/api/v1/vaults",
            Some(&plain),
            Some("morinet_session=bogus-cookie-value"),
        )
        .await;
        assert_ne!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "Bearer + unrelated cookie passes on the Bearer alone"
        );
    }

    #[tokio::test]
    async fn api_unknown_path_is_problem_not_found() {
        let (app, s) = api_app().await;
        let plain = mint_token(&s, "admin@api.test").await;
        let res = get(app.clone(), "/api/v1/nope", Some(&plain), None).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "unknown path → 404");
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        let j = body_json(res).await;
        assert_eq!(j["code"], "not_found");
        assert_eq!(j["status"], 404);
        // The catch-all sits INSIDE the Bearer group: no token → 401, not 404.
        let res = get(app, "/api/v1/nope", None, None).await;
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "unknown path without token → 401 (catch-all is gated)"
        );
    }

    #[tokio::test]
    async fn helper_vault_json_field_names() {
        let (s, _src) = seed_vault("vaultjson", "vj1").await;
        let entries = accessible_vault_entries(&s, "admin@api.test").await;
        let entry = entries.iter().find(|a| a.vault.slug == "vj1").unwrap();
        let j = vault_json(entry);
        // EXACTLY the MCP field set (D-08): slug/name/scope/role, no extras.
        let obj = j.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["name", "role", "scope", "slug"]);
        assert_eq!(obj["slug"].as_str().unwrap(), "vj1");
        assert_eq!(obj["name"].as_str().unwrap(), "API vault");
    }
}
