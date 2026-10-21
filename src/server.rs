//! Axum router: health, login (SQLite + bcrypt), multi-vault index and gated
//! per-slug vault serving with ACL checks (Phase 2).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Extension, Form, Path, Query, Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::browse;
use crate::config::RuntimeConfig;
use crate::email::{invitation_email, reset_email, EmailSender, InMemorySender, SmtpSender};
use crate::session::{Claims, SessionCodec, DEFAULT_TTL_SECS};
use crate::store::Store;
use crate::webdav;

/// Invitation links are valid 24h (REQ-EMAIL-005).
const INVITE_TTL_SECS: u64 = 24 * 3600;
/// Password-reset links are valid 1h (stricter — it is a live credential).
const RESET_TTL_SECS: u64 = 3600;

pub struct AppState {
    pub cfg: RuntimeConfig,
    pub codec: SessionCodec,
    pub store: Store,
    /// Phase 5: outbound email. Injected so tests use `InMemorySender`.
    pub email: Arc<dyn EmailSender>,
}

pub type SharedState = Arc<AppState>;

/// Embedded stylesheet for all MoriNet app pages (login, index, admin).
const STYLE_CSS: &str = include_str!("../assets/style.css");

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Sign in</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  <h1>MoriNet</h1>
  <!-- novalidate: the login field accepts usernames, not just emails (v1) -->
  <form method="post" action="/login" novalidate>
    <label>Login <input type="text" name="email" required></label>
    <label>Password <input type="password" name="password" required></label>
    <button type="submit">Sign in</button>
  </form>
  <p><a href="/reset">Forgot your password?</a></p>
</body>
</html>"#;

/// Render an app page with the embedded stylesheet + standard header/nav.
/// Every handler-generated page MUST go through this — a raw `Html(body)`
/// ships unstyled (verified in the browser: serif default, no card, no nav).
fn app_page(title: &str, body: &str) -> Response {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{t}</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  <h1>MoriNet</h1>
  {b}
</body>
</html>"#,
        t = html_escape(title),
        b = body
    );
    Html(html).into_response()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs()
}

/// Extract cookie values from a `Cookie` header into a name→value map.
fn parse_cookies(header: &str) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    for pair in header.split(';') {
        let pair = pair.trim();
        let (name, value) = pair.split_once('=')?;
        map.insert(name.trim().to_string(), value.trim().to_string());
    }
    Some(map)
}

pub fn build_router(cfg: RuntimeConfig, store: Store) -> Router {
    // Production wiring: a real SMTP sender when [smtp] is usable, else a
    // no-op recorder so the app still boots without email configured.
    let email: Arc<dyn EmailSender> = if cfg.smtp.is_usable() {
        match SmtpSender::from_config(&cfg.smtp) {
            Ok(sender) => Arc::new(sender),
            Err(e) => {
                eprintln!("warning: [smtp] is invalid, email disabled: {e}");
                Arc::new(InMemorySender::new())
            }
        }
    } else {
        Arc::new(InMemorySender::new())
    };
    build_router_with_email(cfg, store, email)
}

/// Same router, with an explicit sender (tests inject `InMemorySender`).
pub fn build_router_with_email(
    cfg: RuntimeConfig,
    store: Store,
    email: Arc<dyn EmailSender>,
) -> Router {
    let state: SharedState = Arc::new(AppState {
        codec: SessionCodec::new(cfg.cookie_secret.clone()),
        cfg,
        store,
        email,
    });

    // Public routes.
    let public = Router::new()
        .route("/health", get(health))
        .route("/login", get(login_form).post(login_submit))
        .route("/reset", get(reset_request_form).post(reset_request_submit))
        .route("/reset/{token}", get(reset_form).post(reset_submit))
        .route(
            "/invite/{token}",
            get(invite_accept_form).post(invite_accept_submit),
        )
        .route(
            "/assets/style.css",
            get(|| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    STYLE_CSS,
                )
            }),
        )
        .with_state(state.clone());

    // Protected routes: vault index + per-slug vault serving.
    let protected = Router::new()
        .route("/", get(index))
        .route("/vault", get(vault_root_redirect))
        .route("/vault/{slug}/", get(vault_index))
        .route("/vault/{slug}/{*rest}", get(vault_file))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ))
        .with_state(state.clone());

    // Admin routes: gate by role=admin (require_admin is self-contained — it
    // verifies the cookie itself since require_session is not in this chain).
    let admin = Router::new()
        .route("/admin", get(admin_dashboard))
        .route("/admin/users", get(admin_users))
        .route("/admin/users/invite", post(admin_invite))
        .route("/admin/vaults", get(admin_vaults))
        .route("/admin/browse", get(admin_browse))
        .route("/admin/groups", get(admin_groups).post(admin_create_group))
        .route("/admin/groups/create", post(admin_create_group))
        .route("/admin/groups/{id}/delete", post(admin_delete_group))
        .route("/admin/groups/{id}/members", post(admin_group_members))
        .route("/admin/groups/{id}/rename", post(admin_rename_group))
        .route("/admin/vaults/{slug}/groups", post(admin_vault_groups))
        .route("/admin/vaults/create", post(admin_create_vault))
        .route("/admin/vaults/{slug}/perms", get(admin_perms))
        .route("/admin/vaults/{slug}/delete", post(admin_delete_vault))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin))
        .with_state(state.clone());

    // WebDAV (Phase 6). Method-routing inside one catch-all handler (PROPFIND
    // is not a known method in the router, so it cannot be a .route() verb).
    let webdav = Router::new()
        .route("/webdav/{slug}", any(webdav_entry))
        .route("/webdav/{slug}/{*rest}", any(webdav_entry))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_dav_auth,
        ))
        .with_state(state.clone());

    public.merge(protected).merge(admin).merge(webdav)
}

/// WebDAV gate: HTTP Basic against the same bcrypt users table, then the
/// per-vault ACL. Returns 401 with a challenge when creds are missing/bad.
async fn require_dav_auth(
    State(state): State<SharedState>,
    mut req: Request,
    next: Next,
) -> Response {
    let creds = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(webdav::parse_basic);

    let unauthorized = || {
        (
            StatusCode::UNAUTHORIZED,
            [webdav::www_authenticate_basic()],
            "authentication required",
        )
            .into_response()
    };

    let Some((user, pass)) = creds else {
        return unauthorized();
    };
    match state.store.verify_login(&user, &pass).await {
        Ok(Some(user)) => {
            // Same pattern as require_session: hand the verified identity to
            // the handler as an extension.
            req.extensions_mut().insert(Claims {
                email: user.email,
                expires_at: 0,
            });
            next.run(req).await
        }
        Ok(None) => unauthorized(),
        Err(e) => {
            eprintln!("webdav verify_login failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Admin gate (REQ-ADMIN-001): valid cookie + role=admin, else 403.
async fn require_admin(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    let claims = req
        .headers()
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_cookies)
        .and_then(|cookies| cookies.get(&state.cfg.cookie_name).cloned())
        .and_then(|token| state.codec.verify(&token, unix_now()));

    let is_admin = match claims {
        Some(claims) => state
            .store
            .role_for(&claims.email)
            .await
            .ok()
            .flatten()
            .map(|r| r == "admin")
            .unwrap_or(false),
        None => false,
    };

    if is_admin {
        next.run(req).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

// ---------- handlers ----------

async fn health() -> Json<&'static str> {
    Json("ok")
}

async fn login_form() -> Html<&'static str> {
    Html(LOGIN_HTML)
}

#[derive(Debug, Deserialize)]
struct LoginParams {
    email: String,
    password: String,
}

async fn login_submit(
    State(state): State<SharedState>,
    Form(params): Form<LoginParams>,
) -> Response {
    let user = state
        .store
        .verify_login(&params.email, &params.password)
        .await
        .ok()
        .flatten();
    match user {
        Some(_) => {
            let token = state
                .codec
                .sign(&params.email, unix_now(), DEFAULT_TTL_SECS);
            let cookie = format!(
                "{}={token}; HttpOnly; Path=/; SameSite=Lax",
                state.cfg.cookie_name
            );
            let headers: HeaderMap = HeaderMap::from_iter([(SET_COOKIE, cookie.parse().unwrap())]);
            (headers, Redirect::to("/")).into_response()
        }
        None => (StatusCode::UNAUTHORIZED, Html(LOGIN_HTML)).into_response(),
    }
}

/// Auth middleware: valid signed session cookie required, otherwise redirect to
/// /login. On success the verified claims are inserted as request extensions so
/// downstream handlers know who is calling.
async fn require_session(
    State(state): State<SharedState>,
    mut req: Request,
    next: Next,
) -> Response {
    let claims = req
        .headers()
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_cookies)
        .and_then(|cookies| cookies.get(&state.cfg.cookie_name).cloned())
        .and_then(|token| state.codec.verify(&token, unix_now()));

    match claims {
        Some(claims) => {
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        None => Redirect::to("/login").into_response(),
    }
}

async fn vault_root_redirect() -> Redirect {
    Redirect::to("/")
}

async fn index(State(state): State<SharedState>, Extension(claims): Extension<Claims>) -> Response {
    // Phase 8: the listing uses the group model, so a user sees their own
    // personal vaults plus the global ones their groups are granted.
    let accessible = state
        .store
        .accessible_vaults(&claims.email)
        .await
        .unwrap_or_default();
    let mut personal_items = String::new();
    let mut global_items = String::new();
    for a in &accessible {
        let line = format!(
            r#"<li><a href="/vault/{}/">{}</a> <small>({})</small></li>"#,
            html_escape(&a.vault.slug),
            html_escape(&a.vault.name),
            if a.personal {
                "private"
            } else {
                a.role.as_str()
            },
        );
        if a.personal {
            personal_items.push_str(&line);
        } else {
            global_items.push_str(&line);
        }
    }
    // Admin nav (only for admins).
    let is_admin = state
        .store
        .role_for(&claims.email)
        .await
        .ok()
        .flatten()
        .map(|r| r == "admin")
        .unwrap_or(false);
    let admin_nav = if is_admin {
        r#"<p><a href="/admin"><strong>Admin</strong></a> — manage users, vaults and permissions</p>"#
    } else {
        ""
    };
    let personal_section = if personal_items.is_empty() {
        String::new()
    } else {
        format!("<h2>My vaults</h2>\n<ul>{personal_items}</ul>")
    };
    let global_section = if global_items.is_empty() {
        "<p><em>No shared vault yet.</em></p>".to_string()
    } else {
        format!("<h2>Shared with me</h2>\n<ul>{global_items}</ul>")
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Vaults</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  <h1>MoriNet</h1>
  <p>Signed in as {}</p>
  {admin_nav}
  {personal_section}
  {global_section}
</body>
</html>"#,
        html_escape(&claims.email)
    );
    Html(html).into_response()
}

async fn vault_index(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    serve_vault_file(&state, &claims.email, &slug, "index.html").await
}

async fn vault_file(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path((slug, rest)): Path<(String, String)>,
) -> Response {
    serve_vault_file(&state, &claims.email, &slug, &rest).await
}

/// ACL check + safe file serving from the vault's compiled book directory.
async fn serve_vault_file(state: &AppState, email: &str, slug: &str, rest: &str) -> Response {
    // Access gate (REQ-VAULT-004): admin, owner of a personal vault, or a group
    // grant. Reading only needs read; writing (WebDAV PUT) needs write.
    match state.store.access_for(email, slug).await {
        Ok(a) if a.can_read() => {}
        Ok(_) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            eprintln!("access_for failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // Resolve book dir for this slug, from the same model that granted access.
    let accessible = state
        .store
        .accessible_vaults(email)
        .await
        .unwrap_or_default();
    let Some(vault) = accessible.iter().find(|a| a.vault.slug == slug) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let book_dir = std::path::PathBuf::from(&vault.vault.book_path);

    // Normalize the requested path and reject traversal (`..` escapes).
    let rel = rest.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    if rel.split('/').any(|c| c == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let full = book_dir.join(rel);

    match tokio::fs::metadata(&full).await {
        Ok(meta) if meta.is_dir() => {
            let index = full.join("index.html");
            match tokio::fs::read(&index).await {
                Ok(bytes) => (
                    [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    bytes,
                )
                    .into_response(),
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        _ => match tokio::fs::read(&full).await {
            Ok(bytes) => {
                let ct = content_type(rel);
                ([(axum::http::header::CONTENT_TYPE, ct)], bytes).into_response()
            }
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

/// Resolve the vault's SOURCE dir for a slug, after the ACL gate.
/// Returns (source_dir, is_admin, book_dir).
async fn dav_resolve(
    state: &AppState,
    email: &str,
    slug: &str,
) -> Result<(PathBuf, bool, PathBuf), StatusCode> {
    let accessible = state
        .store
        .accessible_vaults(email)
        .await
        .unwrap_or_default();
    let Some(entry) = accessible.iter().find(|a| a.vault.slug == slug) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let book_dir = PathBuf::from(&entry.vault.book_path);
    let Some(src) = webdav::source_dir_of(&book_dir) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let is_admin = state
        .store
        .role_for(email)
        .await
        .unwrap_or(None)
        .map(|r| r == "admin")
        .unwrap_or(false);
    Ok((src, is_admin, book_dir))
}

/// Single entry point for every WebDAV verb on `/webdav/{slug}/...`.
async fn webdav_entry(State(state): State<SharedState>, req: Request) -> Response {
    // Extracted manually rather than via many extractors: PROPFIND is a custom
    // method and mixing Method/Uri/HeaderMap/Body extractors here is fragile.
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    let claims = req
        .extensions()
        .get::<Claims>()
        .cloned()
        .unwrap_or_else(|| Claims {
            email: String::new(),
            expires_at: 0,
        });
    let slug = uri
        .path()
        .strip_prefix("/webdav/")
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    let body = req.into_body();
    // The Basic-auth middleware verified the user; we still re-check access per
    // vault, and remember whether it includes write (PUT needs it).
    let access = match state.store.access_for(&claims.email, &slug).await {
        Ok(a) if a.can_read() => a,
        Ok(_) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            eprintln!("dav access_for failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let (src_dir, _is_admin, book_dir) = match dav_resolve(&state, &claims.email, &slug).await {
        Ok(v) => v,
        Err(code) => return code.into_response(),
    };
    let rel = webdav::rel_from_uri(&uri, &slug);
    let base = format!("/webdav/{slug}");

    if webdav::is_propfind(&method) {
        let full = match webdav::safe_join(&src_dir, &rel) {
            Some(p) => p,
            None => return StatusCode::BAD_REQUEST.into_response(),
        };
        let meta = match tokio::fs::metadata(&full).await {
            Ok(m) => m,
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        };
        if meta.is_file() {
            return webdav::file_propfind_response(
                &base,
                &rel,
                meta.len(),
                meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            );
        }
        let depth0 = webdav::depth_of(&headers) == 0;
        return match webdav::list_dir(&src_dir, &rel, &base, depth0).await {
            Some(xml) => webdav::propfind_response(xml),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }

    match method {
        Method::GET => {
            let full = match webdav::safe_join(&src_dir, &rel) {
                Some(p) => p,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };
            match tokio::fs::read(&full).await {
                Ok(bytes) => (
                    [(axum::http::header::CONTENT_TYPE, content_type(&rel))],
                    bytes,
                )
                    .into_response(),
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        Method::PUT => {
            // Writing needs the write role: admins, the owner of a personal
            // vault, or a group granted write.
            if !access.can_write() {
                return StatusCode::FORBIDDEN.into_response();
            }
            if rel.is_empty() {
                return StatusCode::METHOD_NOT_ALLOWED.into_response();
            }
            let full = match webdav::safe_join(&src_dir, &rel) {
                Some(p) => p,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };
            let bytes = match axum::body::to_bytes(body, 8 * 1024 * 1024).await {
                Ok(b) => b,
                Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
            };
            if let Some(parent) = full.parent() {
                if tokio::fs::create_dir_all(parent).await.is_err() {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            if tokio::fs::write(&full, &bytes).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            // Rebuild so the served HTML reflects the new note (AD-10).
            rebuild_book(&book_dir, &src_dir);
            StatusCode::CREATED.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

/// Re-run `mdbook build` on the source dir so the change goes live.
/// Best-effort: a build failure is logged, the write already happened.
fn rebuild_book(book_dir: &std::path::Path, src_dir: &std::path::Path) {
    if !src_dir.join("book.toml").is_file() {
        return;
    }
    match std::process::Command::new("mdbook")
        .arg("build")
        .arg(src_dir)
        .output()
    {
        Ok(out) if out.status.success() => {
            eprintln!(
                "webdav: rebuilt {} -> {}",
                src_dir.display(),
                book_dir.display()
            );
        }
        Ok(out) => eprintln!(
            "webdav: mdbook build failed after PUT: {}",
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(_) => eprintln!("webdav: mdbook not available, book not rebuilt"),
    }
}

fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------- admin handlers (Phase 4) ----------

async fn admin_page(title: &str, body: String) -> Response {
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — {title}</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  <h1>MoriNet admin</h1>
  <nav><a href="/">Vaults</a> | <a href="/admin/users">Users</a> | <a href="/admin/groups">Groups</a> | <a href="/admin/vaults">Vaults mgmt</a></nav>
  <h2>{title}</h2>
  {body}
</body>
</html>"#
    ))
    .into_response()
}

async fn admin_dashboard(State(state): State<SharedState>) -> Response {
    let users = state.store.list_users().await.unwrap_or_default();
    let vaults = state.store.list_all_vaults().await.unwrap_or_default();
    let body = format!("<p>{} user(s), {} vault(s)</p>", users.len(), vaults.len());
    admin_page("Dashboard", body).await
}

async fn admin_users(State(state): State<SharedState>) -> Response {
    let users = state.store.list_users().await.unwrap_or_default();
    let mut rows = String::new();
    for u in &users {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>\n",
            html_escape(&u.email),
            html_escape(&u.role)
        ));
    }
    let body = format!(
        r#"<table border="1" cellpadding="4"><tr><th>Email</th><th>Role</th></tr>
{rows}</table>
<h3>Invite a user</h3>
<form method="post" action="/admin/users/invite" novalidate>
  <label>Login <input type="text" name="email" required></label>
  <label>Role <select name="role"><option value="reader">reader</option><option value="admin">admin</option></select></label>
  <button type="submit">Invite</button>
</form>"#
    );
    admin_page("Users", body).await
}

#[derive(Debug, Deserialize)]
struct InviteParams {
    email: String,
    #[serde(default = "default_invite_role")]
    role: String,
}

fn default_invite_role() -> String {
    "reader".to_string()
}

/// sha256 hex of a plaintext token — what actually lands in the DB.
fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

/// Fresh 256-bit plaintext token (the only copy that is emailed).
fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

async fn admin_invite(State(state): State<SharedState>, Form(p): Form<InviteParams>) -> Response {
    // Phase 5: when SMTP is configured, create an invitation token and email
    // the link. The user does NOT exist yet — it is created when the invitee
    // picks a password. Without SMTP we keep the legacy temp-password flow.
    if state.cfg.smtp.is_usable() {
        let token = new_token();
        if let Err(e) = state
            .store
            .create_invitation(&token_hash(&token), &p.email, &p.role, INVITE_TTL_SECS)
            .await
        {
            let body = format!(
                "<p style=\"color:red\">Failed to create invitation: {}</p><p><a href=\"/admin/users\">Back</a></p>",
                html_escape(&e.to_string())
            );
            return (
                StatusCode::BAD_REQUEST,
                app_page("Invitation failed", &body),
            )
                .into_response();
        }
        let url = format!("{}/invite/{}", state.cfg.base_url, token);
        let mail = invitation_email(&p.email, &url, &p.role);
        if let Err(e) = state.email.send(&mail).await {
            // The token is already stored but undeliverable: tell the admin and
            // show the link once so the invite is not silently lost.
            let body = format!(
                "<p style=\"color:red\">Invitation stored but email delivery failed: {}</p><p>Send this link manually:</p><p><code>{}</code></p><p><a href=\"/admin/users\">Back</a></p>",
                html_escape(&e.to_string()),
                html_escape(&url)
            );
            return (
                StatusCode::BAD_GATEWAY,
                app_page("Invitation sent (manual)", &body),
            )
                .into_response();
        }
        let body = format!(
            "<p>Invitation email sent to <strong>{}</strong> with role <strong>{}</strong>.</p><p>The account is created when they set a password (link valid 24h).</p><p><a href=\"/admin/users\">Back to users</a></p>",
            html_escape(&p.email),
            html_escape(&p.role)
        );
        return admin_page("Invitation sent", body).await;
    }

    // No SMTP configured: legacy flow, temp password shown once to the admin.
    let temp_password = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    match state
        .store
        .create_user(&p.email, &temp_password, &p.role)
        .await
    {
        Ok(()) => {
            let body = format!(
                r#"<p>User <strong>{}</strong> created with role <strong>{}</strong>.</p>
<p>Temporary password (shown once): <code>{}</code></p>
<p style="color:#666">Configure the <code>[smtp]</code> section to send invitations by email instead.</p>
<p><a href="/admin/users">Back to users</a></p>"#,
                html_escape(&p.email),
                html_escape(&p.role),
                temp_password
            );
            admin_page("User invited", body).await
        }
        Err(e) => {
            let body = format!(
                "<p style=\"color:red\">Failed to create user: {}</p><p><a href=\"/admin/users\">Back</a></p>",
                html_escape(&e.to_string())
            );
            (StatusCode::BAD_REQUEST, app_page("Invalid link", &body)).into_response()
        }
    }
}

// ---------- invitation acceptance (Phase 5) ----------

const RESET_REQUEST_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Reset password</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  <h1>Reset your password</h1>
  <p>Enter your login and we'll email you a reset link.</p>
  <form method="post" action="/reset" novalidate>
    <label>Login <input type="text" name="email" required></label>
    <button type="submit">Send the link</button>
  </form>
  <p><a href="/login">Back to sign in</a></p>
</body>
</html>"#;

async fn reset_request_form() -> Html<&'static str> {
    Html(RESET_REQUEST_HTML)
}

#[derive(Debug, Deserialize)]
struct ResetRequestParams {
    email: String,
}

async fn reset_request_submit(
    State(state): State<SharedState>,
    Form(p): Form<ResetRequestParams>,
) -> Response {
    let token = new_token();
    // Silently no-op for an unknown login: never disclose whether an account
    // exists (create_reset_token returns Ok without storing anything).
    if let Err(e) = state
        .store
        .create_reset_token(&token_hash(&token), &p.email, RESET_TTL_SECS)
        .await
    {
        let body = format!(
            "<p style=\"color:red\">Reset request failed: {}</p>",
            html_escape(&e.to_string())
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            app_page("Something went wrong", &body),
        )
            .into_response();
    }
    let known = state
        .store
        .role_for(&p.email)
        .await
        .unwrap_or(None)
        .is_some();
    if known {
        let url = format!("{}/reset/{}", state.cfg.base_url, token);
        if let Err(e) = state.email.send(&reset_email(&p.email, &url)).await {
            eprintln!("warning: reset email to {} failed: {e}", p.email);
        }
    }
    // Identical answer either way — no account enumeration.
    let body = r#"<p>If that account exists, a reset link is on its way.</p><p>Check your inbox (and spam folder).</p><p><a href="/login">Back to sign in</a></p>"#;
    app_page("Reset your password", body)
}

async fn reset_form(State(state): State<SharedState>, Path(token): Path<String>) -> Response {
    if state
        .store
        .validate_reset_token(&token_hash(&token))
        .await
        .unwrap_or(None)
        .is_none()
    {
        return invalid_token_page().await;
    }
    let body = format!(
        r#"<h2>Choose a new password</h2>
<form method="post" action="/reset/{t}" novalidate>
  <label>New password <input type="password" name="password" required></label>
  <button type="submit">Save</button>
</form>"#,
        t = html_escape(&token)
    );
    app_page("Reset your password", &body)
}

#[derive(Debug, Deserialize)]
struct NewPasswordParams {
    password: String,
}

async fn reset_submit(
    State(state): State<SharedState>,
    Path(token): Path<String>,
    Form(p): Form<NewPasswordParams>,
) -> Response {
    if p.password.len() < 8 {
        let body = format!(
            r#"<p style="color:red">Password too short (8 characters minimum).</p>
<form method="post" action="/reset/{t}" novalidate>
  <label>New password <input type="password" name="password" required></label>
  <button type="submit">Save</button>
</form>"#,
            t = html_escape(&token)
        );
        return (
            StatusCode::BAD_REQUEST,
            app_page("Reset your password", &body),
        )
            .into_response();
    }
    match state
        .store
        .consume_reset_token(&token_hash(&token), &p.password)
        .await
    {
        Ok(Some(_)) => {
            let body = r#"<p>Password updated. <a href="/login">Sign in</a>.</p>"#;
            app_page("Password updated", body)
        }
        Ok(None) => invalid_token_page().await,
        Err(e) => {
            let body = format!(
                "<p style=\"color:red\">Reset failed: {}</p>",
                html_escape(&e.to_string())
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                app_page("Something went wrong", &body),
            )
                .into_response()
        }
    }
}

async fn invite_accept_form(
    State(state): State<SharedState>,
    Path(token): Path<String>,
) -> Response {
    match state.store.validate_invitation(&token_hash(&token)).await {
        Ok(Some((email, role))) => {
            let body = format!(
                r#"<h2>Accept your invitation</h2>
<p>Set a password to activate your account.</p>
<form method="post" action="/invite/{t}" novalidate>
  <label>Login <input type="text" name="email" value="{e}" readonly></label>
  <label>Password <input type="password" name="password" required></label>
  <p>Role: <strong>{r}</strong></p>
  <button type="submit">Create my account</button>
</form>"#,
                t = html_escape(&token),
                e = html_escape(&email),
                r = html_escape(&role)
            );
            app_page("Accept your invitation", &body)
        }
        Ok(None) => invalid_token_page().await,
        Err(e) => {
            let body = format!(
                "<p style=\"color:red\">Invitation check failed: {}</p>",
                html_escape(&e.to_string())
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                app_page("Something went wrong", &body),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct AcceptInviteParams {
    /// Accepted but deliberately IGNORED: the identity and role come from the
    /// stored invitation, never from the form — a tampered field cannot
    /// escalate the account. Parsed only so browsers that post it still work.
    #[serde(default)]
    #[allow(dead_code)]
    email: String,
    password: String,
}

async fn invite_accept_submit(
    State(state): State<SharedState>,
    Path(token): Path<String>,
    Form(p): Form<AcceptInviteParams>,
) -> Response {
    if p.password.len() < 8 {
        let body = r#"<p style="color:red">Password too short (8 characters minimum).</p>"#;
        return (
            StatusCode::BAD_REQUEST,
            app_page("Accept your invitation", body),
        )
            .into_response();
    }
    match state
        .store
        .consume_invitation(&token_hash(&token), &p.password)
        .await
    {
        Ok(Some(email)) => {
            // The role comes from the stored invitation, never from the form:
            // a tampered `email` field cannot escalate to admin.
            let body = format!(
                r#"<p>Welcome to MoriNet, <strong>{}</strong>.</p><p><a href="/login">Sign in</a>.</p>"#,
                html_escape(&email)
            );
            app_page("Welcome!", &body)
        }
        Ok(None) => invalid_token_page().await,
        Err(e) => {
            let body = format!(
                "<p style=\"color:red\">Could not accept invitation: {}</p>",
                html_escape(&e.to_string())
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                app_page("Something went wrong", &body),
            )
                .into_response()
        }
    }
}

async fn invalid_token_page() -> Response {
    let body = r#"<p style="color:red">This link is invalid, already used, or expired.</p><p><a href="/login">Back to sign in</a></p>"#;
    (StatusCode::BAD_REQUEST, app_page("Link expired", body)).into_response()
}

/// `GET /admin/browse?path=/…` → JSON listing of server directories (REQ-UX-002).
/// Confined to the configured data dir: the admin picks where a vault lives on
/// THIS host, so browsing `/etc` is refused (403), not merely hidden.
async fn admin_browse(State(state): State<SharedState>, Query(q): Query<BrowseQuery>) -> Response {
    let root = state.cfg.data_dir.clone();
    // Default to the root when the admin opens the picker with no path.
    let raw = q.path.unwrap_or_else(|| "/".to_string());

    match browse::browse(&root, &raw) {
        Ok(listing) => Json(json!({
            "current": listing.current,
            "parent": listing.parent,
            "dirs": listing.dirs.iter().map(|d| json!({
                "name": d.name,
                "path": d.path,
                "is_vault": d.is_vault,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => {
            let status =
                StatusCode::from_u16(e.as_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            if e == browse::BrowseError::OutsideRoot {
                eprintln!("admin browse refused an out-of-root path: {raw}");
            }
            (
                status,
                Json(json!({ "error": match e {
                    browse::BrowseError::OutsideRoot => "outside the allowed root",
                    browse::BrowseError::NotADirectory => "no such directory",
                    browse::BrowseError::Unreadable => "cannot read this directory",
                }})),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

// ---------- groups (Phase 8) ----------

/// One form action; the pages are simple server-rendered forms with no JS.
#[derive(Debug, Deserialize)]
struct GroupParams {
    #[serde(default)]
    name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    group_id: Option<i64>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    action: String,
}

async fn admin_groups(State(state): State<SharedState>) -> Response {
    let groups = state.store.list_groups().await.unwrap_or_default();
    let users = state.store.list_users().await.unwrap_or_default();

    let mut blocks = String::new();
    for (gid, name, count) in &groups {
        let members = state.store.group_members(*gid).await.unwrap_or_default();
        let mut mlist = String::new();
        for m in &members {
            mlist.push_str(&format!(
                r#"<li>{} <form method="post" action="/admin/groups/{gid}/members" style="display:inline">
<input type="hidden" name="email" value="{me}"><input type="hidden" name="action" value="remove">
<button type="submit">remove</button></form></li>
"#,
                html_escape(m),
                me = html_escape(m)
            ));
        }
        // Every user gets a row so adding is a single click, no free-text.
        let mut picker = String::new();
        for u in &users {
            if members.iter().any(|m| m == &u.email) {
                continue;
            }
            picker.push_str(&format!(
                r#"<option value="{}">{}</option>"#,
                html_escape(&u.email),
                html_escape(&u.email)
            ));
        }
        blocks.push_str(&format!(
            r#"<h3>{name} <small>({count} member(s))</small></h3>
<form method="post" action="/admin/groups/{gid}/rename" style="display:inline">
  <input type="text" name="name" value="{name}" aria-label="group name">
  <button type="submit">Rename</button>
</form>
<form method="post" action="/admin/groups/{gid}/delete" style="display:inline;margin-left:8px">
  <button type="submit">Delete group</button>
</form>
<p>Members:</p>
<ul style="list-style:none;padding-left:0">{mlist}</ul>
<form method="post" action="/admin/groups/{gid}/members">
  <select name="email"><option value="">— add a user —</option>{picker}</select>
  <input type="hidden" name="action" value="add">
  <button type="submit">Add to group</button>
</form>
<hr>
"#,
            name = html_escape(name),
            count = count,
            mlist = mlist,
            picker = picker,
        ));
    }

    let body = format!(
        r#"<p>Groups decide who sees which global vault. A group can hold
<b>read</b> or <b>write</b> on each vault, assigned per vault.</p>
{blocks}
<h3>Create a group</h3>
<form method="post" action="/admin/groups/create" novalidate>
  <label>Name <input type="text" name="name" required></label>
  <button type="submit">Create</button>
</form>"#
    );
    admin_page("Groups", body).await
}

async fn admin_create_group(
    State(state): State<SharedState>,
    Form(p): Form<GroupParams>,
) -> Response {
    let name = p.name.trim();
    if name.is_empty() {
        let body = "<p>Group name required.</p>";
        return (StatusCode::BAD_REQUEST, Html(body.to_string())).into_response();
    }
    match state.store.create_group(name).await {
        Ok(_) => Redirect::to("/admin/groups").into_response(),
        Err(e) => {
            let body = format!(
                r#"<p style="color:red">Could not create the group: {}</p>
<p><em>A group with that name may already exist.</em></p>
<p><a href="/admin/groups">Back</a></p>"#,
                html_escape(&e.to_string())
            );
            (StatusCode::BAD_REQUEST, Html(body)).into_response()
        }
    }
}

async fn admin_delete_group(State(state): State<SharedState>, Path(id): Path<i64>) -> Response {
    // Members and vault assignments cascade (migration FKs).
    let _ = state.store.delete_group(id).await;
    Redirect::to("/admin/groups").into_response()
}

async fn admin_rename_group(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Form(p): Form<GroupParams>,
) -> Response {
    let name = p.name.trim();
    if !name.is_empty() {
        let _ = state.store.rename_group(id, name).await;
    }
    Redirect::to("/admin/groups").into_response()
}

async fn admin_group_members(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Form(p): Form<GroupParams>,
) -> Response {
    let email = p.email.trim();
    if !email.is_empty() {
        if p.action == "remove" {
            let _ = state.store.remove_group_member(id, email).await;
        } else {
            let _ = state.store.add_group_member(id, email).await;
        }
    }
    Redirect::to("/admin/groups").into_response()
}

/// Assign groups a role on one global vault (REQ-VLT-004).
async fn admin_vault_groups(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Form(p): Form<GroupParams>,
) -> Response {
    if let Some(gid) = p.group_id {
        match p.role.as_deref() {
            Some("none") | Some("") => {
                let _ = state.store.clear_vault_group_role(&slug, gid).await;
            }
            Some("read") => {
                let _ = state.store.set_vault_group_role(&slug, gid, "read").await;
            }
            Some("write") => {
                let _ = state.store.set_vault_group_role(&slug, gid, "write").await;
            }
            _ => {}
        }
    }
    Redirect::to(&format!("/admin/vaults/{}/perms", slug)).into_response()
}

async fn admin_vaults(State(state): State<SharedState>) -> Response {
    let vaults = state.store.list_all_vaults().await.unwrap_or_default();
    let mut rows = String::new();
    for v in &vaults {
        rows.push_str(&format!(
            r#"<tr><td><a href="/admin/vaults/{s}/perms">{s}</a></td><td>{n}</td><td><form method="post" action="/admin/vaults/{s}/delete" style="display:inline"><button type="submit">Delete</button></form></td></tr>
"#,
            s = html_escape(&v.slug),
            n = html_escape(&v.name)
        ));
    }
    let body = format!(
        r#"<table border="1" cellpadding="4"><tr><th>Slug</th><th>Name</th><th></th></tr>
{rows}</table>
<h3>Add a vault</h3>
<form method="post" action="/admin/vaults/create" novalidate>
  <label>Slug <input type="text" name="slug" required></label>
  <label>Name <input type="text" name="name" required></label>
  <label>Kind <select name="kind" onchange="document.getElementById('gh-fields').style.display = this.value==='github'?'block':'none'; document.getElementById('lo-fields').style.display = this.value==='local'?'block':'none';"><option value="local">local</option><option value="github">github</option></select></label>
  <span id="lo-fields"><label>Book dir path <input type="text" name="path"></label></span>
  <span id="gh-fields" style="display:none"><label>Git URL <input type="text" name="url"></label><label>Token <input type="text" name="token"></label></span>
  <button type="submit">Add</button>
</form>
<p><em>Note:</em> `local` expects a compiled book directory; `github` expects a repo with <code>book.toml</code> + <code>src/SUMMARY.md</code> (cloned + built on add, re-synced every 5 min).</p>"#
    );
    admin_page("Vaults", body).await
}

#[derive(Debug, Deserialize)]
struct CreateVaultParams {
    slug: String,
    name: String,
    kind: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    token: String,
}

async fn admin_create_vault(
    State(state): State<SharedState>,
    Form(p): Form<CreateVaultParams>,
) -> Response {
    // Resolve + build now so failures surface immediately in the admin UI.
    use crate::sources::{GitSource, LocalSource, VaultSource};
    let data_dir = std::path::PathBuf::from("data");
    let result = match p.kind.as_str() {
        "local" => {
            let src = LocalSource {
                path: PathBuf::from(&p.path),
            };
            src.sync().and_then(|d| src.build(&d))
        }
        "github" => {
            let src = GitSource::github(
                p.url.clone(),
                (!p.token.is_empty()).then(|| p.token.clone()),
                data_dir.join("repos").join(&p.slug),
            );
            src.sync().and_then(|d| src.build(&d))
        }
        other => return error_page(&format!("unknown kind '{other}'")),
    };

    match result {
        Ok(book_dir) => match state.store.seed_vault(&p.slug, &p.name, &book_dir).await {
            Ok(()) => Redirect::to("/admin/vaults").into_response(),
            Err(e) => error_page(&format!("seed failed: {e}")),
        },
        Err(e) => error_page(&format!("sync/build failed: {e}")),
    }
}

fn error_page(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(format!(
            r#"<html lang="en"><head><meta charset="utf-8"><title>MoriNet — Error</title>
<link rel="stylesheet" href="/assets/style.css"></head><body>
<p style="color:red">{}</p><p><a href="/admin/vaults">Back to vaults</a></p>
</body></html>"#,
            html_escape(msg)
        )),
    )
        .into_response()
}

async fn admin_perms(State(state): State<SharedState>, Path(slug): Path<String>) -> Response {
    let groups = state.store.list_groups().await.unwrap_or_default();
    let assigned = state
        .store
        .vault_group_roles(&slug)
        .await
        .unwrap_or_default();
    let vault = state
        .store
        .list_all_vaults()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|v| v.slug == slug);

    let Some(vault) = vault else {
        let body =
            r#"<p style="color:red">No such vault.</p><p><a href="/admin/vaults">Back</a></p>"#;
        return (StatusCode::NOT_FOUND, Html(body.to_string())).into_response();
    };

    let personal = vault.scope == "personal";
    let mut rows = String::new();
    for (gid, name, _count) in &groups {
        let role = assigned
            .iter()
            .find(|(g, _, _)| g == gid)
            .map(|(_, _, r)| r.as_str())
            .unwrap_or("none");
        let row = format!(
            r#"<tr><td>{}</td>
<td><form method="post" action="/admin/vaults/{slug}/groups">
  <input type="hidden" name="group_id" value="{gid}">
  <select name="role">
    <option value="none"{nsel}>no access</option>
    <option value="read"{rsel}>read</option>
    <option value="write"{wsel}>write</option>
  </select>
  <button type="submit">Apply</button>
</form></td></tr>
"#,
            html_escape(name),
            nsel = if role == "none" { " selected" } else { "" },
            rsel = if role == "read" { " selected" } else { "" },
            wsel = if role == "write" { " selected" } else { "" },
        );
        rows.push_str(&row);
    }

    let note = if personal {
        format!(
            "<p style=\"color:#b45309\"><strong>Personal vault</strong> — it belongs to \
             <strong>{}</strong> and stays private. Groups do not apply.</p>",
            html_escape(vault.owner_email.as_deref().unwrap_or("its owner"))
        )
    } else {
        "<p>Global vault — groups below decide who can read or write it.</p>".to_string()
    };

    let body = format!(
        r#"<p>Vault: <strong>{name}</strong> <code>{slug}</code></p>
{note}
<table border="1" cellpadding="4"><tr><th>Group</th><th>Role</th></tr>
{rows}</table>
<p><a href="/admin/vaults">Back to vaults</a> · <a href="/admin/groups">Manage groups</a></p>"#,
        name = html_escape(&vault.name),
        slug = html_escape(&slug),
    );
    admin_page("Vault permissions", body).await
}

async fn admin_delete_vault(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
) -> Response {
    match state.store.delete_vault(&slug).await {
        Ok(()) => Redirect::to("/admin/vaults").into_response(),
        Err(e) => error_page(&format!("delete failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt; // oneshot

    use crate::email::SmtpConfig;
    use base64::Engine;

    fn test_config() -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            base_url: "http://localhost:8080".to_string(),
            cookie_secret: b"test-secret-0123456789abcdef".to_vec(),
            cookie_name: "morinet_session".to_string(),
            admin_email: "admin@example.com".to_string(),
            admin_password: "[REDACTED-DEV-PASSWORD]".to_string(),
            smtp: Default::default(), // Phase 5: no SMTP in tests unless asked
            data_dir: std::env::temp_dir().join("morinet-test-data"),
            vault_book_dir: None, // Phase 2: book dirs live in the vaults table
        }
    }

    async fn fixture_store() -> Store {
        let store = Store::connect_memory().await.expect("memory store");
        store
            .ensure_admin("admin@example.com", "[REDACTED-DEV-PASSWORD]")
            .await
            .unwrap();
        let book = std::env::temp_dir().join(format!("morinet-vault-{}", std::process::id()));
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(
            book.join("index.html"),
            "<html><body><h1>MoriNet demo vault</h1></body></html>",
        )
        .unwrap();
        std::fs::write(
            book.join("notes.html"),
            "<html><body>notes page</body></html>",
        )
        .unwrap();
        store.seed_vault("demo", "Demo vault", &book).await.unwrap();
        store
    }

    async fn test_router() -> (Router, SharedState) {
        let cfg = test_config();
        let store = fixture_store().await;
        let email: Arc<dyn EmailSender> = Arc::new(InMemorySender::new());
        let state: SharedState = Arc::new(AppState {
            codec: SessionCodec::new(cfg.cookie_secret.clone()),
            cfg,
            store,
            email,
        });
        (
            build_router_with_email(state.cfg.clone(), state.store.clone(), state.email.clone()),
            state,
        )
    }

    /// Router with a usable [smtp] section + an inspectable sender, so the
    /// email paths (invite / reset) can be exercised end to end.
    fn smtp_cfg() -> SmtpConfig {
        SmtpConfig {
            host: Some("smtp.test".into()),
            from: Some("bot@mori.test".into()),
            port: Some(587),
            starttls: Some(true),
            user: None,
            pass: None,
        }
    }

    async fn test_router_with_email(
        cfg: RuntimeConfig,
    ) -> (Router, SharedState, Arc<InMemorySender>) {
        let store = fixture_store().await;
        let sender = Arc::new(InMemorySender::new());
        let state: SharedState = Arc::new(AppState {
            codec: SessionCodec::new(cfg.cookie_secret.clone()),
            cfg,
            store,
            email: sender.clone(),
        });
        (
            build_router_with_email(state.cfg.clone(), state.store.clone(), state.email.clone()),
            state,
            sender,
        )
    }

    async fn req(
        app: Router,
        method: &str,
        uri: &str,
        headers: HeaderMap,
        body: Option<Body>,
    ) -> Response {
        let mut builder = Request::builder().method(method).uri(uri);
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }
        app.oneshot(builder.body(body.unwrap_or_else(Body::empty)).unwrap())
            .await
            .unwrap()
    }

    fn login_headers() -> HeaderMap {
        HeaderMap::from_iter([(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        )])
    }

    async fn login_cookie(app: &Router) -> String {
        let res = req(
            app.clone(),
            "POST",
            "/login",
            login_headers(),
            Some(Body::from("email=admin%40example.com&password=test-pass-9f3a")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let set_cookie = res
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        set_cookie.split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn health_is_public_and_ok() {
        let (app, _) = test_router().await;
        let res = req(app, "GET", "/health", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_form_is_public() {
        let (app, _) = test_router().await;
        let res = req(app, "GET", "/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn anonymous_index_redirects_to_login() {
        let (app, _) = test_router().await;
        let res = req(app, "GET", "/", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
    }

    #[tokio::test]
    async fn bad_credentials_return_401() {
        let (app, _) = test_router().await;
        let res = req(
            app,
            "POST",
            "/login",
            login_headers(),
            Some(Body::from("email=admin@example.com&password=WRONG")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn good_credentials_set_cookie_and_redirect() {
        let (app, _) = test_router().await;
        let res = req(
            app,
            "POST",
            "/login",
            login_headers(),
            Some(Body::from("email=admin%40example.com&password=test-pass-9f3a")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let set_cookie = res
            .headers()
            .get(SET_COOKIE)
            .expect("Set-Cookie on successful login")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            set_cookie.starts_with("morinet_session="),
            "got: {set_cookie}"
        );
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Lax"));
    }

    #[tokio::test]
    async fn login_only_works_against_db_users() {
        let (app, _) = test_router().await;
        // An unknown user must get 401 (no env-only admin anymore).
        let res = req(
            app,
            "POST",
            "/login",
            login_headers(),
            Some(Body::from("email=stranger@example.com&password=x")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn index_lists_accessible_vaults() {
        let (app, _) = test_router().await;
        let cookie = login_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "GET", "/", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("/vault/demo/"), "index lists demo: {body}");
        assert!(body.contains("Demo vault"), "index shows name: {body}");
    }

    #[tokio::test]
    async fn admin_reads_vault_files() {
        let (app, _) = test_router().await;
        let cookie = login_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app.clone(), "GET", "/vault/demo/", headers.clone(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("MoriNet demo vault"));

        let res = req(app, "GET", "/vault/demo/notes.html", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("notes page"));
    }

    #[tokio::test]
    async fn path_traversal_is_blocked() {
        let (app, _) = test_router().await;
        let cookie = login_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "GET", "/vault/demo/../etc/passwd", headers, None).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn tampered_cookie_is_redirected_to_login() {
        let (app, _) = test_router().await;
        let evil = "morinet_session=aGF4.bogus.sig".parse().unwrap();
        let headers = HeaderMap::from_iter([(COOKIE, evil)]);
        let res = req(app, "GET", "/vault/demo/", headers, None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
    }

    #[tokio::test]
    async fn reader_without_acl_gets_403_on_vault() {
        let (app, state) = test_router().await;
        // Create a reader with no ACL grants.
        state
            .store
            .create_user("reader@example.com", "pw", "reader")
            .await
            .unwrap();

        // Reader can log in.
        let res = req(
            app.clone(),
            "POST",
            "/login",
            login_headers(),
            Some(Body::from("email=reader%40example.com&password=pw")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let set_cookie = res
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let cookie = set_cookie.split(';').next().unwrap().to_string();
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);

        // But vault access is forbidden (REQ-VAULT-004).
        let res = req(app, "GET", "/vault/demo/", headers, None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    // ---------- Phase 4: admin-ui ----------

    async fn admin_cookie(app: &Router) -> String {
        login_cookie(app).await // fixture admin is role=admin
    }

    async fn reader_cookie(app: &Router, state: &SharedState) -> String {
        state
            .store
            .create_user("plain@example.com", "pw", "reader")
            .await
            .unwrap();
        let res = req(
            app.clone(),
            "POST",
            "/login",
            login_headers(),
            Some(Body::from("email=plain%40example.com&password=pw")),
        )
        .await;
        let set_cookie = res
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        set_cookie.split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn admin_pages_require_admin_role() {
        let (app, state) = test_router().await;
        let reader = reader_cookie(&app, &state).await;
        let headers = HeaderMap::from_iter([(COOKIE, reader.parse().unwrap())]);
        for uri in ["/admin", "/admin/users", "/admin/vaults"] {
            let res = req(app.clone(), "GET", uri, headers.clone(), None).await;
            assert_eq!(
                res.status(),
                StatusCode::FORBIDDEN,
                "{uri} must be 403 for readers"
            );
        }
    }

    #[tokio::test]
    async fn admin_sees_users_page_and_dashboard() {
        let (app, _) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);

        let res = req(app.clone(), "GET", "/admin", headers.clone(), None).await;
        assert_eq!(res.status(), StatusCode::OK);

        let res = req(app, "GET", "/admin/users", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("admin@example.com"));
    }

    #[tokio::test]
    async fn invite_creates_reader_with_temp_password() {
        let (app, state) = test_router().await;
        let cookie = admin_cookie(&app).await;

        let res = req(
            app,
            "POST",
            "/admin/users/invite",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from("email=newbie@example.com&role=reader")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("newbie@example.com"), "confirmation: {body}");
        assert!(
            body.contains("Temporary password"),
            "temp password shown once"
        );

        // The user exists with role=reader and can log in with the temp password.
        let user = state
            .store
            .verify_login("newbie@example.com", "pw")
            .await
            .unwrap();
        assert!(user.is_none(), "placeholder pw must not work");

        // Extract the temp password from the page and verify it works.
        let start = body.find("<code>").unwrap() + 6;
        let end = body[start..].find("</code>").unwrap() + start;
        let temp_password = body[start..end].to_string();
        let user = state
            .store
            .verify_login("newbie@example.com", &temp_password)
            .await
            .unwrap()
            .expect("temp password verifies");
        assert_eq!(user.role, "reader");
    }

    #[tokio::test]
    async fn admin_assigns_a_group_read_on_a_vault() {
        let (app, state) = test_router().await;
        let root = seed_dav_vault_named(&state, "admin-assigns-a-group-read-on-a-vault").await;
        state
            .store
            .create_user("reader@example.com", "Read!pw123", "reader")
            .await
            .unwrap();
        let cookie = admin_cookie(&app).await;

        // Before any assignment the reader is refused on the WebDAV surface.
        assert_eq!(
            state
                .store
                .access_for("reader@example.com", "dav")
                .await
                .unwrap(),
            crate::store::Access::None
        );

        // Create a group, put the reader in it, grant it 'read'.
        let gid = state.store.create_group("Readers").await.unwrap();
        state
            .store
            .add_group_member(gid, "reader@example.com")
            .await
            .unwrap();
        let res = req(
            app.clone(),
            "POST",
            "/admin/vaults/dav/groups",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from(format!("group_id={gid}&role=read"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER, "assignment redirects");
        assert_eq!(
            state
                .store
                .access_for("reader@example.com", "dav")
                .await
                .unwrap(),
            crate::store::Access::Read,
            "the group grant now grants access"
        );

        // Revoke it.
        let res = req(
            app,
            "POST",
            "/admin/vaults/dav/groups",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from(format!("group_id={gid}&role=none"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state
                .store
                .access_for("reader@example.com", "dav")
                .await
                .unwrap(),
            crate::store::Access::None,
            "revoked immediately"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    // ---------- Phase 5: invitation + reset over HTTP ----------

    /// Extract the `/invite/{token}` token from a captured invitation email.
    fn invite_token_from(mail: &crate::email::Email) -> String {
        let marker = "/invite/";
        let start = mail.body_html.find(marker).expect("invite link in body") + marker.len();
        let end = mail.body_html[start..]
            .find(['"', '<'])
            .expect("token ends at quote/tag");
        mail.body_html[start..start + end].to_string()
    }

    fn reset_token_from(mail: &crate::email::Email) -> String {
        let marker = "/reset/";
        let start = mail.body_html.find(marker).expect("reset link in body") + marker.len();
        let end = mail.body_html[start..]
            .find(['"', '<'])
            .expect("token ends at quote/tag");
        mail.body_html[start..start + end].to_string()
    }

    fn email_cfg() -> RuntimeConfig {
        RuntimeConfig {
            smtp: smtp_cfg(),
            ..test_config()
        }
    }

    #[tokio::test]
    async fn admin_invite_emails_a_link_instead_of_creating_the_user() {
        let (app, state, sender) = test_router_with_email(email_cfg()).await;
        let cookie = admin_cookie(&app).await;

        let res = req(
            app,
            "POST",
            "/admin/users/invite",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from("email=newcomer@example.com&role=reader")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("Invitation email sent"), "page: {body}");
        assert!(
            !body.contains("Temporary password"),
            "no temp password when SMTP is on: {body}"
        );

        // Exactly one email, addressed correctly, carrying a link.
        assert_eq!(sender.count(), 1);
        let mail = sender.only();
        assert_eq!(mail.to, "newcomer@example.com");
        assert!(mail.body_html.contains("/invite/"));

        // The user does NOT exist yet — only after they set a password.
        assert!(
            state
                .store
                .role_for("newcomer@example.com")
                .await
                .unwrap()
                .is_none(),
            "no account before acceptance"
        );
    }

    #[tokio::test]
    async fn invite_link_creates_the_account_on_submit() {
        let (app, state, sender) = test_router_with_email(email_cfg()).await;
        let cookie = admin_cookie(&app).await;
        req(
            app.clone(),
            "POST",
            "/admin/users/invite",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from("email=newcomer@example.com&role=reader")),
        )
        .await;
        let token = invite_token_from(&sender.only());

        // GET the invite page: it shows the login + role.
        let res = req(
            app.clone(),
            "GET",
            &format!("/invite/{token}"),
            HeaderMap::new(),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let page = to_bytes(res.into_body(), 8192).await.unwrap();
        let page = String::from_utf8_lossy(&page);
        assert!(page.contains("newcomer@example.com"), "invite page: {page}");
        assert!(page.contains("reader"));

        // POST the chosen password (tampered email field must be ignored).
        let res = req(
            app.clone(),
            "POST",
            &format!("/invite/{token}"),
            login_headers(),
            Some(Body::from("email=attacker%40evil.fr&password=Ch0osen!pw")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let page = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&page).contains("Welcome to MoriNet"));

        // Account exists with the INVITED email and role (not the tampered one).
        let users = state.store.list_users().await.unwrap();
        assert!(
            users
                .iter()
                .any(|u| u.email == "newcomer@example.com" && u.role == "reader"),
            "invited identity created"
        );
        assert!(!users.iter().any(|u| u.email == "attacker@evil.fr"));
        // Chosen password works.
        assert!(state
            .store
            .verify_login("newcomer@example.com", "Ch0osen!pw")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn invite_link_is_single_use() {
        let (app, state, sender) = test_router_with_email(email_cfg()).await;
        let cookie = admin_cookie(&app).await;
        req(
            app.clone(),
            "POST",
            "/admin/users/invite",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from("email=newcomer@example.com&role=reader")),
        )
        .await;
        let token = invite_token_from(&sender.only());

        let ok = req(
            app.clone(),
            "POST",
            &format!("/invite/{token}"),
            login_headers(),
            Some(Body::from(
                "email=newcomer%40example.com&password=First!pw1",
            )),
        )
        .await;
        assert_eq!(ok.status(), StatusCode::OK);
        // Replay refused.
        let replay = req(
            app.clone(),
            "POST",
            &format!("/invite/{token}"),
            login_headers(),
            Some(Body::from(
                "email=newcomer%40example.com&password=Other!pw2",
            )),
        )
        .await;
        assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
        // First password stands.
        assert!(state
            .store
            .verify_login("newcomer@example.com", "First!pw1")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn invalid_invite_link_shows_error_page() {
        let (app, _state, _sender) = test_router_with_email(email_cfg()).await;
        let res = req(
            app,
            "GET",
            "/invite/not-a-real-token",
            HeaderMap::new(),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let page = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&page).contains("invalid"));
    }

    #[tokio::test]
    async fn password_reset_flow_emails_link_and_updates_password() {
        let (app, state, sender) = test_router_with_email(email_cfg()).await;
        // A reader account to reset.
        state
            .store
            .create_user("reader@example.com", "Old!pw123", "reader")
            .await
            .unwrap();

        // Request the reset.
        let res = req(
            app.clone(),
            "POST",
            "/reset",
            login_headers(),
            Some(Body::from("email=reader%40example.com")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(sender.count(), 1);
        let mail = sender.only();
        assert_eq!(mail.to, "reader@example.com");
        assert!(mail.body_html.contains("/reset/"));
        let token = reset_token_from(&mail);

        // GET reset page, then submit a new password.
        let res = req(
            app.clone(),
            "GET",
            &format!("/reset/{token}"),
            HeaderMap::new(),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let res = req(
            app.clone(),
            "POST",
            &format!("/reset/{token}"),
            login_headers(),
            Some(Body::from("password=Fresh!pw456")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);

        // New password works, old one is dead.
        assert!(state
            .store
            .verify_login("reader@example.com", "Fresh!pw456")
            .await
            .unwrap()
            .is_some());
        assert!(state
            .store
            .verify_login("reader@example.com", "Old!pw123")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn reset_for_unknown_login_is_silent_and_sends_nothing() {
        let (app, _state, sender) = test_router_with_email(email_cfg()).await;
        let res = req(
            app,
            "POST",
            "/reset",
            login_headers(),
            Some(Body::from("email=ghost%40example.com")),
        )
        .await;
        // Same answer as a valid login — no enumeration.
        assert_eq!(res.status(), StatusCode::OK);
        let page = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&page).contains("If that account exists"));
        assert_eq!(sender.count(), 0, "no email for an unknown login");
    }

    #[tokio::test]
    async fn reset_rejects_short_password() {
        let (app, state, sender) = test_router_with_email(email_cfg()).await;
        state
            .store
            .create_user("reader@example.com", "Old!pw123", "reader")
            .await
            .unwrap();
        req(
            app.clone(),
            "POST",
            "/reset",
            login_headers(),
            Some(Body::from("email=reader%40example.com")),
        )
        .await;
        let token = reset_token_from(&sender.only());
        let res = req(
            app,
            "POST",
            &format!("/reset/{token}"),
            login_headers(),
            Some(Body::from("password=short")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // Token was NOT consumed by the rejected attempt.
        assert!(state
            .store
            .validate_reset_token(&token_hash(&token))
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn invite_accept_rejects_short_password() {
        let (app, state, sender) = test_router_with_email(email_cfg()).await;
        let cookie = admin_cookie(&app).await;
        req(
            app.clone(),
            "POST",
            "/admin/users/invite",
            {
                let mut h = login_headers();
                h.insert(COOKIE, cookie.parse().unwrap());
                h
            },
            Some(Body::from("email=newcomer@example.com&role=reader")),
        )
        .await;
        let token = invite_token_from(&sender.only());
        let res = req(
            app,
            "POST",
            &format!("/invite/{token}"),
            login_headers(),
            Some(Body::from("email=newcomer%40example.com&password=short")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // No account created, token still live.
        assert!(state
            .store
            .validate_invitation(&token_hash(&token))
            .await
            .unwrap()
            .is_some());
    }

    // ---------- Phase 6: WebDAV ----------

    fn basic(user: &str, pass: &str) -> HeaderMap {
        let raw = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        HeaderMap::from_iter([(
            axum::http::header::AUTHORIZATION,
            format!("Basic {raw}").parse().unwrap(),
        )])
    }

    /// A vault whose stored path is `<src>/book`, with real mdbook sources.
    ///
    /// `name` must be unique per test: cargo runs them in parallel and a shared
    /// fixture path lets one test delete another's directory.
    async fn seed_dav_vault_named(state: &SharedState, name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("morinet-dav-vault-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("book.toml"), "[book]\ntitle = \"T\"\n").unwrap();
        std::fs::write(root.join("src").join("SUMMARY.md"), "# Summary\n").unwrap();
        std::fs::write(root.join("src").join("note.md"), "# Note\n\nhello\n").unwrap();
        std::fs::create_dir_all(root.join("book")).unwrap();
        std::fs::write(root.join("book").join("index.html"), "<html>built</html>").unwrap();
        state
            .store
            .seed_vault("dav", "DAV vault", &root.join("book"))
            .await
            .unwrap();
        root
    }

    async fn dav_request(
        app: Router,
        method: &str,
        uri: &str,
        auth: HeaderMap,
        body: Option<&str>,
    ) -> Response {
        let b = Body::from(body.unwrap_or("").to_string());
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Depth", "1");
        for (k, v) in auth.iter() {
            builder = builder.header(k, v);
        }
        app.oneshot(builder.body(b).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn webdav_requires_authentication() {
        let (app, state) = test_router().await;
        seed_dav_vault_named(&state, "webdav-requires-authentication").await;
        let res = dav_request(app, "PROPFIND", "/webdav/dav", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let challenge = res
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            challenge.starts_with("Basic realm="),
            "challenge: {challenge}"
        );
    }

    #[tokio::test]
    async fn webdav_rejects_bad_credentials() {
        let (app, state) = test_router().await;
        seed_dav_vault_named(&state, "webdav-rejects-bad-credentials").await;
        let res = dav_request(
            app,
            "PROPFIND",
            "/webdav/dav",
            basic("admin@example.com", "wrong"),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn propfind_lists_the_vault_source_depth1() {
        let (app, state) = test_router().await;
        let root = seed_dav_vault_named(&state, "propfind-lists-the-vault-source-depth1").await;
        let res = dav_request(
            app,
            "PROPFIND",
            "/webdav/dav",
            basic("admin@example.com", "[REDACTED-DEV-PASSWORD]"),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::MULTI_STATUS);
        let body = to_bytes(res.into_body(), 16384).await.unwrap();
        let xml = String::from_utf8_lossy(&body);
        assert!(xml.contains("<D:multistatus"), "xml body: {xml}");
        assert!(xml.contains("src/"), "src dir listed: {xml}");
        assert!(xml.contains("book.toml"), "book.toml listed: {xml}");
        // The compiled book/ must NOT be the exposed surface.
        assert!(
            !xml.contains("/book/index.html"),
            "built files not served: {xml}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn propfind_depth0_returns_only_the_collection() {
        let (app, state) = test_router().await;
        seed_dav_vault_named(&state, "propfind-depth0-returns-only-the-collection").await;
        // No Depth header at all => depth 0 (RFC 4918 default).
        let mut b = Request::builder().method("PROPFIND").uri("/webdav/dav");
        for (k, v) in basic("admin@example.com", "[REDACTED-DEV-PASSWORD]").iter() {
            b = b.header(k, v);
        }
        let res = app.oneshot(b.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::MULTI_STATUS);
        let body = to_bytes(res.into_body(), 16384).await.unwrap();
        let xml = String::from_utf8_lossy(&body);
        assert!(!xml.contains("book.toml"), "depth 0 hides children: {xml}");
        assert!(
            xml.contains("<D:collection/>"),
            "the collection itself is returned"
        );
    }

    #[tokio::test]
    async fn webdav_get_returns_raw_markdown() {
        let (app, state) = test_router().await;
        seed_dav_vault_named(&state, "webdav-get-returns-raw-markdown").await;
        let res = dav_request(
            app,
            "GET",
            "/webdav/dav/src/note.md",
            basic("admin@example.com", "[REDACTED-DEV-PASSWORD]"),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(ct.contains("markdown"), "content-type: {ct}");
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("hello"));
    }

    #[tokio::test]
    async fn webdav_rejects_path_traversal() {
        let (app, state) = test_router().await;
        seed_dav_vault_named(&state, "webdav-rejects-path-traversal").await;
        for uri in [
            "/webdav/dav/../../etc/passwd",
            "/webdav/dav/src/../../../etc/passwd",
        ] {
            let res = dav_request(
                app.clone(),
                "GET",
                uri,
                basic("admin@example.com", "[REDACTED-DEV-PASSWORD]"),
                None,
            )
            .await;
            assert!(
                res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::NOT_FOUND,
                "traversal {uri} must not succeed, got {}",
                res.status()
            );
        }
    }

    #[tokio::test]
    async fn reader_without_acl_is_denied_on_webdav() {
        let (app, state) = test_router().await;
        seed_dav_vault_named(&state, "reader-without-acl-is-denied-on-webdav").await;
        state
            .store
            .create_user("reader@example.com", "Read!pw123", "reader")
            .await
            .unwrap();
        let res = dav_request(
            app,
            "PROPFIND",
            "/webdav/dav",
            basic("reader@example.com", "Read!pw123"),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "no ACL, no access");
    }

    #[tokio::test]
    async fn reader_put_is_forbidden_even_with_acl() {
        let (app, state) = test_router().await;
        let root = seed_dav_vault_named(&state, "reader-put-is-forbidden-even-with-acl").await;
        state
            .store
            .create_user("reader@example.com", "Read!pw123", "reader")
            .await
            .unwrap();
        state
            .store
            .grant_acl("reader@example.com", "dav")
            .await
            .unwrap();
        let res = dav_request(
            app,
            "PUT",
            "/webdav/dav/src/evil.md",
            basic("reader@example.com", "Read!pw123"),
            Some("pwned"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "readers never write");
        assert!(!root.join("src/evil.md").exists(), "no file written");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn admin_put_writes_the_source_file() {
        let (app, state) = test_router().await;
        let root = seed_dav_vault_named(&state, "admin-put-writes-the-source-file").await;
        let res = dav_request(
            app,
            "PUT",
            "/webdav/dav/src/new-note.md",
            basic("admin@example.com", "[REDACTED-DEV-PASSWORD]"),
            Some("# Fresh note\n\nwritten over webdav\n"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let written = std::fs::read_to_string(root.join("src/new-note.md")).expect("file written");
        assert!(
            written.contains("written over webdav"),
            "content: {written}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn admin_put_percent_encoded_traversal_is_refused() {
        // Found by a real curl pass: `PUT ..%2fescaped.md` returned 201 and
        // created a literal file named "..%2fescaped.md" in the vault root.
        // The path is now percent-decoded before the traversal check.
        let (app, state) = test_router().await;
        let root =
            seed_dav_vault_named(&state, "admin-put-percent-encoded-traversal-is-refused").await;
        for uri in [
            "/webdav/dav/..%2fescaped.md",
            "/webdav/dav/%2e%2e%2fescaped2.md",
            "/webdav/dav/src/..%2f..%2fescaped3.md",
        ] {
            let res = dav_request(
                app.clone(),
                "PUT",
                uri,
                basic("admin@example.com", "[REDACTED-DEV-PASSWORD]"),
                Some("pwned"),
            )
            .await;
            assert_ne!(
                res.status(),
                StatusCode::CREATED,
                "encoded traversal {uri} refused"
            );
        }
        // Nothing landed anywhere.
        assert!(!root.join("escaped.md").exists());
        assert!(!root.join("escaped2.md").exists());
        assert!(!root.parent().unwrap().join("escaped3.md").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn admin_put_traversal_is_refused() {
        let (app, state) = test_router().await;
        let root = seed_dav_vault_named(&state, "admin-put-traversal-is-refused").await;
        let res = dav_request(
            app,
            "PUT",
            "/webdav/dav/../escaped.md",
            basic("admin@example.com", "[REDACTED-DEV-PASSWORD]"),
            Some("nope"),
        )
        .await;
        assert_ne!(res.status(), StatusCode::CREATED, "traversal write refused");
        assert!(!root.parent().unwrap().join("escaped.md").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    // ---------- Phase 7: directory picker (REQ-UX-001..007) ----------

    /// Build a router whose data_dir is a throwaway tree with subdirs.
    ///
    /// `name` must be unique per test: cargo runs tests in parallel and a
    /// shared fixture path lets one test delete another's directory.
    async fn browse_router_named(name: &str) -> (Router, SharedState, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("morinet-browse-http-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("vaults/notes/src")).unwrap();
        std::fs::create_dir_all(root.join("repos")).unwrap();
        std::fs::write(root.join("vaults/notes/book.toml"), "[book]").unwrap();

        let mut cfg = test_config();
        cfg.data_dir = root.clone();
        let store = fixture_store().await;
        let state: SharedState = Arc::new(AppState {
            codec: SessionCodec::new(cfg.cookie_secret.clone()),
            cfg,
            store,
            email: Arc::new(InMemorySender::new()),
        });
        (
            build_router_with_email(state.cfg.clone(), state.store.clone(), state.email.clone()),
            state,
            root,
        )
    }

    async fn get_json(app: Router, uri: &str, headers: HeaderMap) -> (StatusCode, String) {
        let res = req(app, "GET", uri, headers, None).await;
        let status = res.status();
        let body = to_bytes(res.into_body(), 32768).await.unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn browse_requires_admin() {
        let (app, _state, root) = browse_router_named("auth").await;
        // Anonymous -> admin gate rejects.
        let (status, _) = get_json(app.clone(), "/admin/browse", HeaderMap::new()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // A reader is authenticated but not an admin.
        let cookie = reader_cookie(&app, &_state).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());
        let (status, _) = get_json(app, "/admin/browse", h).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "readers never browse the fs");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn browse_lists_the_data_root_for_an_admin() {
        let (app, state, root) = browse_router_named("root").await;
        let cookie = admin_cookie(&app).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());
        let (status, body) = get_json(app, "/admin/browse", h).await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["current"], "/");
        assert!(v["parent"].is_null(), "root has no parent");
        let names: Vec<&str> = v["dirs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"vaults"), "vaults listed: {body}");
        assert!(names.contains(&"repos"), "repos listed: {body}");
        let _ = state;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn browse_descends_and_reports_the_parent() {
        let (app, _state, root) = browse_router_named("descend").await;
        let cookie = admin_cookie(&app).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());

        let (status, body) =
            get_json(app.clone(), "/admin/browse?path=/vaults/notes", h.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["current"], "/vaults/notes");
        assert_eq!(v["parent"], "/vaults");
        let dirs = v["dirs"].as_array().unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0]["name"], "src");
        assert_eq!(dirs[0]["path"], "/vaults/notes/src");

        // A directory holding book.toml is flagged as a vault.
        let (_, body) = get_json(app.clone(), "/admin/browse?path=/vaults", h.clone()).await;
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let notes = v["dirs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == "notes")
            .unwrap();
        assert_eq!(notes["is_vault"], true, "book.toml detected: {body}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn browse_refuses_to_escape_the_data_root() {
        let (app, _state, root) = browse_router_named("escape").await;
        let cookie = admin_cookie(&app).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());
        for attack in [
            "/../../etc",
            "/vaults/../../../etc",
            "/%2e%2e%2f%2e%2e%2fetc",
            "/..",
        ] {
            let (status, _) = get_json(
                app.clone(),
                &format!("/admin/browse?path={attack}"),
                h.clone(),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "escape {attack} refused");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn browse_reports_a_missing_directory() {
        let (app, _state, root) = browse_router_named("missing").await;
        let cookie = admin_cookie(&app).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());
        let (status, body) = get_json(app, "/admin/browse?path=/nope", h).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("no such directory"), "message: {body}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn browse_survives_percent_encoded_query_values() {
        // Regression: a raw request target containing '%' made the server close
        // the connection instead of answering. A legitimate client sending
        // %20 (a folder named "my notes") hit the same path.
        let (app, _state, root) = browse_router_named("pct-query").await;
        // The dir must exist BEFORE the first request so it is listed.
        std::fs::create_dir_all(root.join("my notes/sub")).unwrap();
        let cookie = admin_cookie(&app).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());

        let cases = [
            "/admin/browse?path=%2e%2e%2fetc",
            "/admin/browse?path=..%2f..%2fetc",
            "/admin/browse?path=/my%20notes",
            "/admin/browse?path=%2f..%2fetc",
        ];
        for uri in cases {
            let res = req(app.clone(), "GET", uri, h.clone(), None).await;
            assert!(
                res.status().is_success() || res.status().is_client_error(),
                "{uri} must get an HTTP answer, got {}",
                res.status()
            );
        }

        // The escaped folder is reachable, and `current` comes back ENCODED —
        // it is meant to be pasted straight back into ?path=, so it has to be
        // URL-safe. The filesystem lookup used the decoded form.
        let (status, body) = get_json(app, "/admin/browse?path=/my%20notes", h).await;
        assert_eq!(status, StatusCode::OK, "%20 decoded: {body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["current"], "/my%20notes", "current is URL-safe");
        // It really walked INTO the decoded folder: the child shows up and its
        // path is encoded so it can be clicked straight back.
        let dirs = v["dirs"].as_array().unwrap();
        assert_eq!(dirs.len(), 1, "descended into the decoded folder: {body}");
        assert_eq!(dirs[0]["name"], "sub");
        assert_eq!(dirs[0]["path"], "/my%20notes/sub");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn browse_percent_encodes_its_output_paths() {
        // A directory whose name needs escaping must come back encoded.
        let (app, _state, root) = browse_router_named("encoding").await;
        std::fs::create_dir_all(root.join("my notes")).unwrap();
        let cookie = admin_cookie(&app).await;
        let mut h = HeaderMap::new();
        h.insert(COOKIE, cookie.parse().unwrap());
        let (status, body) = get_json(app, "/admin/browse", h).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("my notes"), "name intact: {body}");
        assert!(body.contains("%20"), "path is percent-encoded: {body}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn admin_can_delete_a_vault() {
        let (app, state) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);

        let res = req(
            app,
            "POST",
            "/admin/vaults/demo/delete",
            headers,
            Some(Body::empty()),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let vaults = state.store.list_all_vaults().await.unwrap();
        assert!(vaults.is_empty(), "demo vault deleted");
    }
}
