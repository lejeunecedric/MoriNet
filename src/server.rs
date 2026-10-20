//! Axum router: health, login (SQLite + bcrypt), multi-vault index and gated
//! per-slug vault serving with ACL checks (Phase 2).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Form, Path, Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::config::RuntimeConfig;
use crate::session::{Claims, SessionCodec, DEFAULT_TTL_SECS};
use crate::store::Store;

pub struct AppState {
    pub cfg: RuntimeConfig,
    pub codec: SessionCodec,
    pub store: Store,
}

pub type SharedState = Arc<AppState>;

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Sign in</title></head>
<body>
  <h1>MoriNet</h1>
  <!-- novalidate: the login field accepts usernames, not just emails (v1) -->
  <form method="post" action="/login" novalidate>
    <label>Login <input type="text" name="email" required></label>
    <label>Password <input type="password" name="password" required></label>
    <button type="submit">Sign in</button>
  </form>
</body>
</html>"#;

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
    let state: SharedState = Arc::new(AppState {
        codec: SessionCodec::new(cfg.cookie_secret.clone()),
        cfg,
        store,
    });

    // Public routes.
    let public = Router::new()
        .route("/health", get(health))
        .route("/login", get(login_form).post(login_submit))
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
        .route("/admin/vaults/create", post(admin_create_vault))
        .route("/admin/vaults/{slug}/perms", get(admin_perms))
        .route("/admin/vaults/{slug}/acl", post(admin_update_acl))
        .route("/admin/vaults/{slug}/delete", post(admin_delete_vault))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin))
        .with_state(state.clone());

    public.merge(protected).merge(admin)
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
    let vaults = state
        .store
        .list_vaults_for(&claims.email)
        .await
        .unwrap_or_default();
    let mut items = String::new();
    for v in &vaults {
        items.push_str(&format!(
            r#"<li><a href="/vault/{}/">{}</a></li>"#,
            html_escape(&v.slug),
            html_escape(&v.name)
        ));
    }
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Vaults</title></head>
<body>
  <h1>MoriNet</h1>
  <p>Signed in as {}</p>
  <ul>{items}</ul>
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
    // ACL gate (REQ-VAULT-004).
    match state.store.acl_allows(email, slug).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(e) => {
            eprintln!("acl_allows failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // Resolve book dir for this slug.
    let vaults = state.store.list_vaults_for(email).await.unwrap_or_default();
    let Some(vault) = vaults.iter().find(|v| v.slug == slug) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let book_dir = std::path::PathBuf::from(&vault.book_path);

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
<head><meta charset="utf-8"><title>MoriNet — {title}</title></head>
<body>
  <h1>MoriNet admin</h1>
  <nav><a href="/">Vaults</a> | <a href="/admin/users">Users</a> | <a href="/admin/vaults">Vaults mgmt</a></nav>
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

async fn admin_invite(State(state): State<SharedState>, Form(p): Form<InviteParams>) -> Response {
    // Temp password shown once to the admin (email delivery comes in Phase 5).
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
            (StatusCode::BAD_REQUEST, Html(body)).into_response()
        }
    }
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
            r#"<p style="color:red">{}</p><p><a href="/admin/vaults">Back to vaults</a></p>"#,
            html_escape(msg)
        )),
    )
        .into_response()
}

async fn admin_perms(State(state): State<SharedState>, Path(slug): Path<String>) -> Response {
    let users = state.store.list_users().await.unwrap_or_default();
    let mut rows = String::new();
    for u in &users {
        let allowed = state
            .store
            .acl_allows(&u.email, &slug)
            .await
            .unwrap_or(false);
        let action = if allowed { "revoke" } else { "grant" };
        rows.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td><form method="post" action="/admin/vaults/{}/acl" style="display:inline">
<input type="hidden" name="user_email" value="{}">
<input type="hidden" name="action" value="{action}">
<button type="submit">{action}</button></form></td></tr>
"#,
            html_escape(&u.email),
            if allowed { "granted" } else { "denied" },
            html_escape(&slug),
            html_escape(&u.email)
        ));
    }
    let body = format!(
        r#"<p>Vault: <strong>{}</strong></p>
<table border="1" cellpadding="4"><tr><th>User</th><th>Access</th><th>Action</th></tr>
{rows}</table>
<p><a href="/admin/vaults">Back to vaults</a></p>"#,
        html_escape(&slug)
    );
    admin_page("Vault permissions", body).await
}

#[derive(Debug, Deserialize)]
struct AclParams {
    user_email: String,
    action: String,
}

async fn admin_update_acl(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Form(p): Form<AclParams>,
) -> Response {
    let result = match p.action.as_str() {
        "grant" => state.store.grant_acl(&p.user_email, &slug).await,
        "revoke" => state.store.revoke_acl(&p.user_email, &slug).await,
        _ => return error_page(&format!("unknown action '{}'", p.action)),
    };
    match result {
        Ok(()) => {
            Redirect::to(&format!("/admin/vaults/{}/perms", html_escape(&slug))).into_response()
        }
        Err(e) => error_page(&format!("ACL update failed: {e}")),
    }
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

    fn test_config() -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            base_url: "http://localhost:8080".to_string(),
            cookie_secret: b"test-secret-0123456789abcdef".to_vec(),
            cookie_name: "morinet_session".to_string(),
            admin_email: "admin@example.com".to_string(),
            admin_password: "Passw0rd!".to_string(),
            vault_book_dir: None, // Phase 2: book dirs live in the vaults table
        }
    }

    async fn fixture_store() -> Store {
        let store = Store::connect_memory().await.expect("memory store");
        store
            .ensure_admin("admin@example.com", "Passw0rd!")
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
        let state: SharedState = Arc::new(AppState {
            codec: SessionCodec::new(cfg.cookie_secret.clone()),
            cfg,
            store,
        });
        (build_router(state.cfg.clone(), state.store.clone()), state)
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
            Some(Body::from("email=admin%40example.com&password=Passw0rd%21")),
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
            Some(Body::from("email=admin%40example.com&password=Passw0rd%21")),
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
    async fn admin_can_grant_and_revoke_acl() {
        let (app, state) = test_router().await;
        state
            .store
            .create_user("reader@example.com", "pw", "reader")
            .await
            .unwrap();
        let cookie = admin_cookie(&app).await;

        // Grant via the admin endpoint.
        let mut headers = login_headers();
        headers.insert(COOKIE, cookie.parse().unwrap());
        let res = req(
            app.clone(),
            "POST",
            "/admin/vaults/demo/acl",
            headers.clone(),
            Some(Body::from("user_email=reader@example.com&action=grant")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(state
            .store
            .acl_allows("reader@example.com", "demo")
            .await
            .unwrap());

        // Revoke.
        let res = req(
            app.clone(),
            "POST",
            "/admin/vaults/demo/acl",
            headers,
            Some(Body::from("user_email=reader@example.com&action=revoke")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(!state
            .store
            .acl_allows("reader@example.com", "demo")
            .await
            .unwrap());
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
