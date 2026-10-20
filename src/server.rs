//! Axum router: health, login (single hardcoded admin), and gated vault serving.

use std::sync::Arc;

use axum::extract::{Form, Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use tower_http::services::ServeDir;

use crate::config::RuntimeConfig;
use crate::session::{SessionCodec, DEFAULT_TTL_SECS};

pub struct AppState {
    pub cfg: RuntimeConfig,
    pub codec: SessionCodec,
}

pub type SharedState = Arc<AppState>;

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Sign in</title></head>
<body>
  <h1>MoriNet</h1>
  <form method="post" action="/login">
    <label>Email <input type="email" name="email" required></label>
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
fn parse_cookies(header: &str) -> Option<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();
    for pair in header.split(';') {
        let pair = pair.trim();
        let (name, value) = pair.split_once('=')?;
        map.insert(name.trim().to_string(), value.trim().to_string());
    }
    Some(map)
}

pub fn build_router(cfg: RuntimeConfig) -> Router {
    let state: SharedState = Arc::new(AppState {
        codec: SessionCodec::new(cfg.cookie_secret.clone()),
        cfg,
    });

    // Public routes.
    let public = Router::new()
        .route("/health", get(health))
        .route("/login", get(login_form).post(login_submit))
        .with_state(state.clone());

    // Protected routes: index + compiled vault serving.
    let mut protected = Router::new()
        .route("/", get(index))
        .with_state(state.clone());
    if let Some(book_dir) = state.cfg.vault_book_dir.clone() {
        protected = protected.nest_service("/vault", ServeDir::new(book_dir));
    }
    let protected = protected
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

async fn index() -> Redirect {
    Redirect::to("/vault/")
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
    if params.email == state.cfg.admin_email && params.password == state.cfg.admin_password {
        let token = state
            .codec
            .sign(&params.email, unix_now(), DEFAULT_TTL_SECS);
        let cookie = format!(
            "{}={token}; HttpOnly; Path=/; SameSite=Lax",
            state.cfg.cookie_name
        );
        let headers: HeaderMap = HeaderMap::from_iter([(SET_COOKIE, cookie.parse().unwrap())]);
        (headers, Redirect::to("/")).into_response()
    } else {
        (StatusCode::UNAUTHORIZED, Html(LOGIN_HTML)).into_response()
    }
}

/// Auth middleware: valid signed session cookie required, otherwise redirect to /login.
async fn require_session(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    let authenticated = req
        .headers()
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_cookies)
        .and_then(|cookies| cookies.get(&state.cfg.cookie_name).cloned())
        .and_then(|token| state.codec.verify(&token, unix_now()))
        .is_some();

    if authenticated {
        next.run(req).await
    } else {
        Redirect::to("/login").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt; // oneshot

    fn test_config(vault_dir: Option<std::path::PathBuf>) -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            base_url: "http://localhost:8080".to_string(),
            cookie_secret: b"test-secret-0123456789abcdef".to_vec(),
            cookie_name: "morinet_session".to_string(),
            admin_email: "admin@example.com".to_string(),
            admin_password: "[REDACTED-DEV-PASSWORD]".to_string(),
            vault_book_dir: vault_dir,
        }
    }

    fn fixture_vault() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("morinet-vault-{}", std::process::id()));
        let book = dir.join("book");
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(
            book.join("index.html"),
            "<html><body><h1>MoriNet demo vault</h1></body></html>",
        )
        .unwrap();
        dir.join("book")
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

    fn cookie_for(state: &AppState, email: &str) -> String {
        format!(
            "{}={}",
            state.cfg.cookie_name,
            state.codec.sign(email, unix_now(), 3600)
        )
    }

    #[tokio::test]
    async fn health_is_public_and_ok() {
        let app = build_router(test_config(None));
        let res = req(app, "GET", "/health", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_form_is_public() {
        let app = build_router(test_config(None));
        let res = req(app, "GET", "/login", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn anonymous_index_redirects_to_login() {
        let app = build_router(test_config(None));
        let res = req(app, "GET", "/", HeaderMap::new(), None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
    }

    #[tokio::test]
    async fn bad_credentials_return_401() {
        let app = build_router(test_config(None));
        let res = req(
            app,
            "POST",
            "/login",
            HeaderMap::from_iter([(
                axum::http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded".parse().unwrap(),
            )]),
            Some(Body::from("email=admin@example.com&password=WRONG")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn good_credentials_set_cookie_and_redirect() {
        let app = build_router(test_config(None));
        let res = req(
            app,
            "POST",
            "/login",
            HeaderMap::from_iter([(
                axum::http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded".parse().unwrap(),
            )]),
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
    async fn cookie_grants_access_to_index_and_vault() {
        let book = fixture_vault();
        let state = Arc::new(AppState {
            codec: SessionCodec::new(b"test-secret-0123456789abcdef".to_vec()),
            cfg: test_config(Some(book)),
        });
        let app = build_router(state.cfg.clone());
        let cookie = cookie_for(&state, "admin@example.com");
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);

        let res = req(app.clone(), "GET", "/", headers.clone(), None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/vault/");

        let res = req(app.clone(), "GET", "/vault/", headers.clone(), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("MoriNet demo vault"));
    }

    #[tokio::test]
    async fn tampered_cookie_is_redirected_to_login() {
        let book = fixture_vault();
        let state = Arc::new(AppState {
            codec: SessionCodec::new(b"test-secret-0123456789abcdef".to_vec()),
            cfg: test_config(Some(book)),
        });
        let app = build_router(state.cfg.clone());
        let evil = format!("{}={}.forged", state.cfg.cookie_name, "aGF4.bogus.sig");
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, evil.parse().unwrap())]);
        let res = req(app, "GET", "/vault/", headers, None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
    }
}
