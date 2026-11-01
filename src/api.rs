//! Shared read helpers for the REST API surface (Phase 16, D-04).
//!
//! The ACL resolution for the read surface lives HERE and only here: MCP
//! tools and REST handlers both call these free helpers, so the two
//! transports can never drift. An invisible vault yields `NotFound` —
//! never a 403-shaped, existence-leaking reason (D-02).

use std::path::{Path, PathBuf};

use axum::extract::{Extension, Path as AxumPath, Query, Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, ETAG, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

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
        Some(token) => state.store.user_for_api_token(&token).await.ok().flatten(),
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
    problem(StatusCode::NOT_FOUND, "not_found", "no such API endpoint")
}

/// `GET /api/v1` — JSON index of the surface + auth hint (D-14). Static
/// content, no state needed; bare object, no envelope (D-08 style).
async fn api_index() -> Response {
    Json(serde_json::json!({
        "endpoints": {
            "/api/v1/vaults": "List every vault you can access (slug, name, scope, role).",
            "/api/v1/vaults/{slug}/notes": "List note paths in one vault, sorted, cursor-paginated.",
            "/api/v1/vaults/{slug}/notes/{path}": "Read one note as raw markdown (ETag / If-None-Match).",
            "/api/v1/search": "Search note contents across your vaults (?q=, optional &slug=).",
            "/api/v1/docs": "Human-readable API docs (public — served without a token)."
        },
        "auth": "Send 'Authorization: Bearer <token>' — create a token on /account."
    }))
    .into_response()
}

/// `GET /api/v1/vaults` — the caller's accessible vaults as a BARE array
/// with the MCP field names (API-02, D-08): exactly slug/name/scope/role,
/// no `{"data": ...}` envelope. Empty array for a user with no grants.
async fn api_vaults(
    State(state): State<SharedState>,
    Extension(user): Extension<User>,
) -> Response {
    let vaults: Vec<serde_json::Value> = accessible_vault_entries(&state.store, &user.email)
        .await
        .iter()
        .map(vault_json)
        .collect();
    Json(vaults).into_response()
}

// ---- 16-03: notes list (cursor), search, note read (ETag/304) ----

/// D-09: `?limit=` default 50, hard cap 200 (silent clamp — kills
/// DoS-via-limit without a distinguishable error).
const NOTES_LIMIT_DEFAULT: u32 = 50;
const NOTES_LIMIT_MAX: u32 = 200;

/// Query DTO for the notes list (BrowseQuery pattern, server.rs:1820).
/// `limit` is kept as a raw string so an unparseable value answers a
/// problem+json `invalid_request` instead of axum's bare Query-rejection 400.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ListQuery {
    pub(crate) limit: Option<String>,
    pub(crate) cursor: Option<String>,
}

/// Query DTO for search: `q` required (validated in-handler so a missing
/// one is a problem+json, not an extractor rejection); `vault` optional.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct SearchQuery {
    pub(crate) q: Option<String>,
    pub(crate) vault: Option<String>,
}

/// `GET /api/v1/vaults/{slug}/notes` — sorted note paths with opaque
/// cursor pagination (D-09). Cursor is URL_SAFE_NO_PAD base64 of the last
/// path returned (session.rs precedent); decode failure fails closed to
/// 400 `invalid_request`. Invisible vault / missing folder → 404 (D-02).
async fn api_list_notes(
    State(state): State<SharedState>,
    Extension(user): Extension<User>,
    AxumPath(slug): AxumPath<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    let paths = match list_notes(&state.store, &user.email, &slug, None).await {
        Ok(p) => p,
        Err(ApiReadError::NotFound) | Err(ApiReadError::NoSuchFolder) => {
            return problem(StatusCode::NOT_FOUND, "not_found", "vault not found")
        }
        Err(_) => {
            return problem(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid list request",
            )
        }
    };
    let limit = match q.limit.as_deref() {
        None => NOTES_LIMIT_DEFAULT,
        Some(raw) => match raw.parse::<u32>() {
            Ok(0) | Err(_) => {
                return problem(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "limit must be a positive integer",
                )
            }
            Ok(n) => n.min(NOTES_LIMIT_MAX),
        },
    };
    // Fail closed: an undecodable/non-UTF-8 cursor is a 400, same spirit as
    // Precondition::Malformed — never silently treated as "no cursor".
    let cursor = match q.cursor.as_deref() {
        None => None,
        Some(c) => match URL_SAFE_NO_PAD
            .decode(c)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
        {
            Some(s) => Some(s),
            None => return problem(StatusCode::BAD_REQUEST, "invalid_request", "invalid cursor"),
        },
    };
    // list_notes output is sorted — skip everything <= cursor, take limit+1
    // to learn whether a next page exists.
    let start = match &cursor {
        Some(c) => paths.partition_point(|p| p <= c),
        None => 0,
    };
    let remaining = &paths[start..];
    let has_more = remaining.len() > limit as usize;
    let page = &remaining[..(limit as usize).min(remaining.len())];
    let next_cursor = if has_more {
        Some(
            URL_SAFE_NO_PAD.encode(
                page.last()
                    .expect("page non-empty when has_more")
                    .as_bytes(),
            ),
        )
    } else {
        None
    };
    Json(serde_json::json!({
        "notes": page,
        "has_more": has_more,
        "next_cursor": next_cursor,
    }))
    .into_response()
}

/// `GET /api/v1/search?q=[&vault=slug]` — MCP Hit-shaped hits (bare array,
/// D-08), top 50 via `search_md`'s cap (D-10), scoped to the caller's
/// accessible vaults. Invisible `vault=` slug → 404, never 403 (D-02).
async fn api_search(
    State(state): State<SharedState>,
    Extension(user): Extension<User>,
    Query(q): Query<SearchQuery>,
) -> Response {
    let query = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(query) = query else {
        return problem(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "q query parameter is required",
        );
    };
    match search_notes(&state.store, &user.email, query, q.vault.as_deref()).await {
        Ok(hits) => Json(hits).into_response(),
        Err(ApiReadError::NotFound) => {
            problem(StatusCode::NOT_FOUND, "not_found", "vault not found")
        }
        Err(_) => problem(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "invalid search request",
        ),
    }
}

/// `GET /api/v1/vaults/{slug}/notes/{*path}` — RAW markdown source
/// (never mdBook build output — D-06/B3) as `text/markdown` with the
/// strong quoted ETag (D-11). Path resolution is exclusively the 16-01
/// `read_note` helper's `safe_join` — no new join logic here.
/// If-None-Match is parsed ONLY by `webdav::if_none_match_of`, fail-closed
/// on Malformed before any body (D-12, PITFALLS C2).
async fn api_read_note(
    State(state): State<SharedState>,
    Extension(user): Extension<User>,
    AxumPath((slug, path)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let bytes = match read_note(&state.store, &user.email, &slug, &path).await {
        Ok(b) => b,
        // NotFound, missing note AND refused traversal all collapse to one
        // 404 shape — no existence leak on any axis (D-02/B1).
        Err(ApiReadError::NotFound)
        | Err(ApiReadError::NoSuchNote)
        | Err(ApiReadError::InvalidPath)
        | Err(ApiReadError::NoSuchFolder) => {
            return problem(StatusCode::NOT_FOUND, "not_found", "note not found")
        }
        Err(ApiReadError::Unreadable) => {
            return problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "could not read the note",
            )
        }
    };
    // ONE hash, quoted wire form compared directly — never re-hash-and-quote
    // separately (15-01-SUMMARY rule).
    let etag = crate::webdav::etag_of(&bytes);
    let not_modified = match crate::webdav::if_none_match_of(&headers) {
        crate::webdav::Precondition::Absent => false,
        // `*` matches: we just successfully read the file, it exists.
        crate::webdav::Precondition::Any => true,
        crate::webdav::Precondition::Tags(tags) => tags.contains(&etag),
        // Fail closed BEFORE any body is served (D-12, PITFALLS C2).
        crate::webdav::Precondition::Malformed => {
            return problem(
                StatusCode::BAD_REQUEST,
                "precondition_failed",
                "malformed If-None-Match header",
            )
        }
    };
    if not_modified {
        // RFC 9110 §8.8.1: 304 carries the ETag header, no body.
        return (StatusCode::NOT_MODIFIED, [(ETAG, etag)]).into_response();
    }
    (
        [
            (CONTENT_TYPE, HeaderValue::from_static("text/markdown")),
            (
                ETAG,
                HeaderValue::from_str(&etag).expect("quoted hex etag is a valid header value"),
            ),
        ],
        bytes,
    )
        .into_response()
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
        .route("/api/v1", get(api_index))
        .route("/api/v1/vaults", get(api_vaults))
        .route("/api/v1/vaults/{slug}/notes", get(api_list_notes))
        .route("/api/v1/vaults/{slug}/notes/{*path}", get(api_read_note))
        .route("/api/v1/search", get(api_search))
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
        let app = build_router_with_email(router_cfg(), s.clone(), Arc::new(InMemorySender::new()));
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
            res.headers()[WWW_AUTHENTICATE],
            "Bearer",
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
        // Live token passes the gate and reaches the endpoint (200).
        let res = get(app.clone(), "/api/v1/vaults", Some(&plain), None).await;
        assert_eq!(res.status(), StatusCode::OK, "live token → 200");

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
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Bearer + unrelated cookie succeeds on the Bearer alone"
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

    /// Owner + sharee with a shared vault (mcp.rs share_fixture pattern):
    /// returns the full app router + store; the sharee can mint a token.
    async fn share_app(tag: &str, slug: &str, role: &str) -> (axum::Router, Store) {
        let s = store().await;
        s.create_user("owner@api.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("sharee@api.test", "pw", "reader")
            .await
            .unwrap();
        let src = tmpdir(tag);
        std::fs::write(src.join("note.md"), "# Shared\nsecret needle content").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault(slug, "Shared vault", &src.join("book"))
            .await
            .unwrap();
        s.share_vault(slug, "sharee@api.test", role).await.unwrap();
        let app = build_router_with_email(router_cfg(), s.clone(), Arc::new(InMemorySender::new()));
        (app, s)
    }

    #[tokio::test]
    async fn api_index_lists_endpoints() {
        let (app, s) = api_app().await;
        let plain = mint_token(&s, "admin@api.test").await;
        let res = get(app, "/api/v1", Some(&plain), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let j = body_json(res).await;
        let text = j.to_string();
        for path in [
            "/api/v1/vaults",
            "/api/v1/vaults/{slug}/notes",
            "/api/v1/vaults/{slug}/notes/{path}",
            "/api/v1/search",
            "/api/v1/docs",
        ] {
            assert!(text.contains(path), "index lists {path}: {text}");
        }
        let auth = j["auth"].as_str().expect("auth hint string");
        assert!(
            auth.contains("Bearer") && auth.contains("/account"),
            "auth hint names Bearer + /account: {auth}"
        );
    }

    #[tokio::test]
    async fn api_vaults_matches_mcp_fields() {
        let (app, s) = share_app("apifields", "av1", "writer").await;
        let plain = mint_token(&s, "sharee@api.test").await;
        let res = get(app, "/api/v1/vaults", Some(&plain), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let j = body_json(res).await;
        // Bare ARRAY — no {"data": ...} envelope (D-08, API-02).
        let arr = j.as_array().expect("top-level JSON array");
        let entry = arr
            .iter()
            .find(|v| v["slug"] == serde_json::json!("av1"))
            .expect("shared vault listed");
        // EXACTLY the MCP field set (D-08): slug/name/scope/role.
        let mut keys: Vec<&str> = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(keys, vec!["name", "role", "scope", "slug"]);
        assert_eq!(entry["role"], "write", "share role surfaces verbatim");
        assert_eq!(entry["name"], "Shared vault");
    }

    #[tokio::test]
    async fn api_vaults_empty_for_stranger() {
        let (app, s) = api_app().await;
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        let plain = mint_token(&s, "stranger@api.test").await;
        let res = get(app, "/api/v1/vaults", Some(&plain), None).await;
        // 200 + empty array, never 403/404 (API-02).
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::json!([]));
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

    // ---- 16-03 task 1: notes list cursor pagination + search ----

    /// Admin-owned vault seeded with `files` (relative paths under the
    /// source dir); returns app + store + source dir.
    async fn seed_notes_app(
        tag: &str,
        slug: &str,
        files: &[&str],
    ) -> (axum::Router, Store, PathBuf) {
        let s = store().await;
        s.ensure_admin("admin@api.test", "pw").await.unwrap();
        let src = tmpdir(tag);
        for f in files {
            let p = src.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, format!("# {f}\ncontent of {f}")).unwrap();
        }
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault(slug, "Notes vault", &src.join("book"))
            .await
            .unwrap();
        let app = build_router_with_email(router_cfg(), s.clone(), Arc::new(InMemorySender::new()));
        (app, s, src)
    }

    async fn get_cond(
        app: axum::Router,
        uri: &str,
        bearer: &str,
        if_none_match: Option<&str>,
    ) -> Response {
        let mut builder = Request::builder()
            .method("GET")
            .uri(uri)
            .header(AUTHORIZATION, format!("Bearer {bearer}"));
        if let Some(inm) = if_none_match {
            builder = builder.header("if-none-match", inm);
        }
        app.oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body_bytes(res: Response) -> Vec<u8> {
        to_bytes(res.into_body(), 1_048_576).await.unwrap().to_vec()
    }

    #[tokio::test]
    async fn api_notes_pagination_round_trip() {
        let (app, s, _src) = seed_notes_app("pg1", "pg1", &["a.md", "b.md", "c.md"]).await;
        let tok = mint_token(&s, "admin@api.test").await;
        let res = get(
            app.clone(),
            "/api/v1/vaults/pg1/notes?limit=2",
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let j = body_json(res).await;
        assert_eq!(j["notes"], serde_json::json!(["a.md", "b.md"]));
        assert_eq!(j["has_more"], true);
        let cur = j["next_cursor"]
            .as_str()
            .expect("next_cursor present")
            .to_string();
        assert!(
            !cur.contains('+') && !cur.contains('/') && !cur.contains('='),
            "cursor must be URL_SAFE_NO_PAD base64: {cur}"
        );
        let res = get(
            app.clone(),
            &format!("/api/v1/vaults/pg1/notes?limit=2&cursor={cur}"),
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let j2 = body_json(res).await;
        assert_eq!(j2["notes"], serde_json::json!(["c.md"]));
        assert_eq!(j2["has_more"], false);
        assert!(j2["next_cursor"].is_null(), "last page has no cursor");
        // Union of both pages == full list, no duplicates.
        let mut all: Vec<String> = j["notes"]
            .as_array()
            .unwrap()
            .iter()
            .chain(j2["notes"].as_array().unwrap().iter())
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        all.sort();
        assert_eq!(all, vec!["a.md", "b.md", "c.md"]);
    }

    #[tokio::test]
    async fn api_notes_limit_default_and_cap() {
        let files: Vec<String> = (0..201).map(|i| format!("note-{i:03}.md")).collect();
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let (app, s, _src) = seed_notes_app("pgcap", "pgcap", &refs).await;
        let tok = mint_token(&s, "admin@api.test").await;
        // Absent limit behaves as 50 (D-09).
        let res = get(app.clone(), "/api/v1/vaults/pgcap/notes", Some(&tok), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let j = body_json(res).await;
        assert_eq!(j["notes"].as_array().unwrap().len(), 50);
        assert_eq!(j["has_more"], true);
        // ?limit=500 is silently clamped to 200 (D-09, never a 400).
        let res = get(
            app.clone(),
            "/api/v1/vaults/pgcap/notes?limit=500",
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let j = body_json(res).await;
        assert_eq!(j["notes"].as_array().unwrap().len(), 200);
        assert_eq!(j["has_more"], true);
        // ?limit=0 → 400 problem+json invalid_request.
        let res = get(
            app.clone(),
            "/api/v1/vaults/pgcap/notes?limit=0",
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(body_json(res).await["code"], "invalid_request");
        // Unparseable limit → same 400 invalid_request problem.
        let res = get(
            app.clone(),
            "/api/v1/vaults/pgcap/notes?limit=abc",
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(body_json(res).await["code"], "invalid_request");
    }

    #[tokio::test]
    async fn api_notes_invalid_cursor_400() {
        let (app, s, _src) = seed_notes_app("pgbad", "pgbad", &["a.md"]).await;
        let tok = mint_token(&s, "admin@api.test").await;
        let res = get(
            app,
            "/api/v1/vaults/pgbad/notes?cursor=%21%21%21",
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "bad cursor → 400");
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(body_json(res).await["code"], "invalid_request");
    }

    #[tokio::test]
    async fn api_search_scoped_to_accessible() {
        let (app, s) = share_app("apiscoped", "sc1", "reader").await;
        // Sharee finds the needle in the shared vault via GLOBAL q (D-10).
        let tok = mint_token(&s, "sharee@api.test").await;
        let res = get(
            app.clone(),
            "/api/v1/search?q=secret%20needle",
            Some(&tok),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let j = body_json(res).await;
        let arr = j.as_array().expect("bare array of hits");
        assert_eq!(arr.len(), 1);
        // Element keys EXACTLY the MCP Hit shape (D-08/D-10).
        let mut keys: Vec<&str> = arr[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(keys, vec!["line", "path", "slug", "snippet"]);
        assert_eq!(arr[0]["slug"], "sc1");
        assert_eq!(arr[0]["path"], "note.md");
        // Stranger's identical global search returns [].
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        let tok = mint_token(&s, "stranger@api.test").await;
        let res = get(app, "/api/v1/search?q=secret%20needle", Some(&tok), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn api_search_requires_q() {
        let (app, s) = api_app().await;
        let tok = mint_token(&s, "admin@api.test").await;
        for uri in ["/api/v1/search", "/api/v1/search?q="] {
            let res = get(app.clone(), uri, Some(&tok), None).await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{uri} → 400");
            assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
            assert_eq!(body_json(res).await["code"], "invalid_request");
        }
    }

    #[tokio::test]
    async fn api_invisible_vault_404_not_403() {
        let (app, s) = share_app("apivis", "vis1", "reader").await;
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        let tok = mint_token(&s, "stranger@api.test").await;
        // Guessed slug on the notes list → 404 problem+json, never 403 (D-02/B1).
        let res = get(app.clone(), "/api/v1/vaults/vis1/notes", Some(&tok), None).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(body_json(res).await["code"], "not_found");
        // Guessed slug on search ?vault= → same 404 shape.
        let res = get(app, "/api/v1/search?q=x&vault=vis1", Some(&tok), None).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(body_json(res).await["code"], "not_found");
    }

    // ---- 16-03 task 2: note read — raw markdown + ETag/304 ----

    #[tokio::test]
    async fn api_note_read_serves_raw_source() {
        let (app, s, src) = seed_notes_app("readraw", "rr1", &["note.md"]).await;
        // A raw source-only token AND a decoy in the build output: serving
        // mdBook output (D-06/B3) would fail both assertions below.
        let source_bytes = b"{{#include ../secret.md}}\n# Real source\n";
        std::fs::write(src.join("note.md"), source_bytes).unwrap();
        std::fs::write(src.join("book/note.md"), b"BUILD OUTPUT LEAK").unwrap();
        let tok = mint_token(&s, "admin@api.test").await;
        let res = get(app, "/api/v1/vaults/rr1/notes/note.md", Some(&tok), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[CONTENT_TYPE], "text/markdown");
        let etag = res.headers()[axum::http::header::ETAG]
            .to_str()
            .unwrap()
            .to_string();
        let bytes = body_bytes(res).await;
        assert_eq!(bytes, source_bytes, "body is the RAW SOURCE bytes");
        // Quoted strong sha256 wire form, equal to webdav::etag_of (15-01 rule).
        let inner = etag.trim_matches('"');
        assert_eq!(inner.len(), 64);
        assert!(inner
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(etag, crate::webdav::etag_of(&bytes));
    }

    #[tokio::test]
    async fn api_note_read_etag_304() {
        let (app, s, _src) = seed_notes_app("etag304", "e304", &["note.md"]).await;
        let tok = mint_token(&s, "admin@api.test").await;
        let res = get_cond(app.clone(), "/api/v1/vaults/e304/notes/note.md", &tok, None).await;
        let etag = res.headers()[axum::http::header::ETAG]
            .to_str()
            .unwrap()
            .to_string();
        let res = get_cond(app, "/api/v1/vaults/e304/notes/note.md", &tok, Some(&etag)).await;
        assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            res.headers()[axum::http::header::ETAG],
            etag.as_str(),
            "304 carries the ETag header (RFC 9110 §8.8.1)"
        );
        assert!(body_bytes(res).await.is_empty(), "304 has no body");
    }

    #[tokio::test]
    async fn api_note_read_if_none_match_star() {
        let (app, s, _src) = seed_notes_app("etagstar", "estar", &["note.md"]).await;
        let tok = mint_token(&s, "admin@api.test").await;
        let res = get_cond(app, "/api/v1/vaults/estar/notes/note.md", &tok, Some("*")).await;
        assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
        assert!(res.headers().contains_key(axum::http::header::ETAG));
    }

    #[tokio::test]
    async fn api_note_read_malformed_if_none_match_400() {
        let (app, s, _src) = seed_notes_app("etagbad", "ebad", &["note.md"]).await;
        let tok = mint_token(&s, "admin@api.test").await;
        // Weak tag → Malformed → 400 fail-closed, NEVER degraded to 200 (D-12).
        let res = get_cond(
            app,
            "/api/v1/vaults/ebad/notes/note.md",
            &tok,
            Some("W/\"x\""),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        let j = body_json(res).await;
        assert_eq!(j["code"], "precondition_failed");
    }

    #[tokio::test]
    async fn api_note_read_refuses_traversal() {
        let (app, s, src) = seed_notes_app("trav", "trav", &["note.md"]).await;
        // A real file OUTSIDE the source dir — reading it would prove escape.
        std::fs::write(src.parent().unwrap().join("secret.md"), b"outside").unwrap();
        let tok = mint_token(&s, "admin@api.test").await;
        for path in ["../secret.md", "%2e%2e%2fsecret.md"] {
            let res = get(
                app.clone(),
                &format!("/api/v1/vaults/trav/notes/{path}"),
                Some(&tok),
                None,
            )
            .await;
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path} → 404");
            assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
            assert_eq!(body_json(res).await["code"], "not_found");
        }
    }

    #[tokio::test]
    async fn api_note_invisible_vault_404() {
        let (app, s) = share_app("visread", "vr1", "reader").await;
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        let tok = mint_token(&s, "stranger@api.test").await;
        let res = get(app, "/api/v1/vaults/vr1/notes/note.md", Some(&tok), None).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "hidden share → 404");
        assert_eq!(res.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(body_json(res).await["code"], "not_found");
    }
}
