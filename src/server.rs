//! Axum router: health, login (SQLite + bcrypt), multi-vault index and gated
//! per-slug vault serving with ACL checks (Phase 2).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Form, Path, Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
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
  <!-- novalidate: non-email admin usernames like "demo" are valid (v1) -->
  <form method="post" action="/login" novalidate>
    <label>Email <input type="text" name="email" required></label>
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
        .with_state(state);

    public.merge(protected)
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
            admin_password: "[REDACTED-DEV-PASSWORD]".to_string(),
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
        let headers = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
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
        let headers = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
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
        let headers = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
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
        let headers = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);

        // But vault access is forbidden (REQ-VAULT-004).
        let res = req(app, "GET", "/vault/demo/", headers, None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
