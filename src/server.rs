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
use crate::store::{AccessibleVault, Store};
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

/// Public source repository, shown on the admin About page (REQ-ADMIN-007).
const REPO_URL: &str = "https://github.com/lejeunecedric/MoriNet";

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

/// Authenticated variant of `app_page` that keeps the global header (brand +
/// account menu) — issue #5: the Share / Statistics / Edit pages went through
/// the bare shell and the top menu vanished on them. Every signed-in page must
/// render through this so the nav never disappears.
async fn nav_page(state: &SharedState, claims: &Claims, title: &str, body: &str) -> Response {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{t}</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  {header}
  {b}
</body>
</html>"#,
        t = html_escape(title),
        header = account_menu(state, claims).await,
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
        .route("/logout", post(logout))
        .route("/account", get(account_page))
        .route("/account/name", post(set_account_name))
        .route("/account/tokens", post(create_account_token))
        .route("/account/tokens/revoke", post(revoke_account_token))
        .route("/vaults/create", post(load_personal_vault))
        .route("/vaults/{slug}/delete", post(delete_my_vault))
        .route("/vaults/{slug}/refresh", post(refresh_my_vault))
        .route(
            "/vault/{slug}/edit",
            get(vault_edit_page).post(vault_edit_submit),
        )
        .route(
            "/vault/{slug}/share",
            get(vault_share_page).post(vault_share_add),
        )
        .route("/vault/{slug}/share/revoke", post(vault_share_revoke))
        .route("/vault/{slug}/share/hide", post(vault_share_hide))
        .route("/vault/{slug}/stats", get(vault_stats))
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
        .route("/admin/about", get(admin_about))
        .route("/admin/users", get(admin_users))
        .route("/admin/users/invite", post(admin_invite))
        .route("/admin/users/role", post(admin_user_role))
        .route("/admin/vaults", get(admin_vaults))
        .route("/admin/browse", get(admin_browse))
        .route("/admin/groups", get(admin_groups).post(admin_create_group))
        .route("/admin/groups/create", post(admin_create_group))
        .route("/admin/groups/{id}/delete", post(admin_delete_group))
        .route("/admin/groups/{id}/members", post(admin_group_members))
        .route("/admin/groups/{id}/rename", post(admin_rename_group))
        .route("/admin/groups/{id}/limit", post(admin_group_limit))
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

    // MCP (Phase 10): Bearer API token, stateless StreamableHttp (rmcp).
    let mcp = Router::new()
        .route("/mcp", any(mcp_entry))
        .with_state(state.store.clone());

    public
        .merge(protected)
        .merge(admin)
        .merge(webdav)
        .merge(mcp)
}

/// `ANY /mcp` — MCP StreamableHttp endpoint (Phase 10). Auth is a dedicated
/// Bearer API token (never the password, never the session cookie). The rmcp
/// service is stateless: one fresh server instance per request, bound to the
/// user behind the token, so tools can never cross users.
async fn mcp_entry(State(store): State<Store>, req: Request) -> Response {
    let bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());
    let user = match bearer {
        Some(token) => store.user_for_api_token(&token).await.ok().flatten(),
        None => None,
    };
    let Some(user) = user else {
        return (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
            axum::Json(serde_json::json!({
                "error": "a valid Bearer API token is required (create one on /account)"
            })),
        )
            .into_response();
    };
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    use tower::ServiceExt;
    let store2 = store.clone();
    let email = user.email.clone();
    let service = StreamableHttpService::new(
        move || Ok(crate::mcp::MoriMcp::new(store2.clone(), email.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false) // stateless for legacy versions too
            .with_json_response(true),
    );
    match service.oneshot(req).await {
        Ok(resp) => resp.into_response(),
        Err(e) => error_page(&format!("mcp transport error: {e}")),
    }
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
async fn require_admin(State(state): State<SharedState>, mut req: Request, next: Next) -> Response {
    let claims = req
        .headers()
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_cookies)
        .and_then(|cookies| cookies.get(&state.cfg.cookie_name).cloned())
        .and_then(|token| state.codec.verify(&token, unix_now()));

    let is_admin = match claims.as_ref() {
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
        // Expose the verified claims to the handler (shared account menu).
        if let Some(claims) = claims {
            req.extensions_mut().insert(claims);
        }
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

/// `POST /logout` — clear the session cookie and return to /login.
/// The cookie is expired (Max-Age=0) so the browser drops it immediately.
async fn logout(State(state): State<SharedState>) -> Response {
    let cookie = format!(
        "{}={{}}; HttpOnly; Path=/; SameSite=Lax; Max-Age=0",
        state.cfg.cookie_name
    );
    let headers: HeaderMap = HeaderMap::from_iter([(SET_COOKIE, cookie.parse().unwrap())]);
    (headers, Redirect::to("/login")).into_response()
}

/// Render ONE vault row `<li>`. Both index lists call this single helper, so
/// "My vaults" and "Shared with me" rows are structurally identical by
/// construction (10.1-02 Task 1: parity is impossible to break).
///
/// `can_share` is computed by the caller via `store.can_share` — it only
/// matters for shared rows; owner rows always render the full action set.
fn vault_row(a: &AccessibleVault, can_share: bool) -> String {
    let slug = html_escape(&a.vault.slug);
    let name = html_escape(&a.vault.name);
    let tag = if a.personal {
        "private"
    } else {
        a.role.as_str()
    };
    let mut actions = String::new();
    if a.personal {
        // The owner manages their own vault: edit settings, pull upstream
        // updates, share it, inspect stats, or remove it (which frees the
        // quota slot). Quiet styling on Delete — the accent stays on the one
        // primary action per row.
        actions.push_str(&format!(
            r#"<a class="btn-ghost" href="/vault/{s}/edit">Edit</a><form method="post" action="/vaults/{s}/refresh"><button type="submit" class="btn-ghost">Refresh</button></form><a class="btn-ghost" href="/vault/{s}/share">Share</a><a class="btn-ghost" href="/vault/{s}/stats">Statistics</a><form method="post" action="/vaults/{s}/delete" onsubmit="return confirm('Remove this vault from your collection?')"><button type="submit" class="btn-quiet">Delete</button></form>"#,
            s = slug,
        ));
    } else {
        // A sharee / group member sees the role-gated subset: Refresh only
        // for writers, Share only when re-sharing is allowed, Statistics
        // always, and "Remove" (hide from my view) only for direct shares.
        if a.role == "write" {
            actions.push_str(&format!(
                r#"<form method="post" action="/vaults/{s}/refresh"><button type="submit" class="btn-ghost">Refresh</button></form>"#,
                s = slug,
            ));
        }
        if can_share {
            actions.push_str(&format!(
                r#"<a class="btn-ghost" href="/vault/{s}/share">Share</a>"#,
                s = slug,
            ));
        }
        actions.push_str(&format!(
            r#"<a class="btn-ghost" href="/vault/{s}/stats">Statistics</a>"#,
            s = slug,
        ));
        if a.direct_share {
            actions.push_str(&format!(
                r#"<form method="post" action="/vault/{s}/share/hide"><button type="submit" class="btn-quiet">Remove</button></form>"#,
                s = slug,
            ));
        }
    }
    format!(
        r#"<li><a class="vault-name" href="/vault/{s}/">{n}</a> <small>({t})</small><span class="vault-actions">{acts}</span></li>"#,
        s = slug,
        n = name,
        t = tag,
        acts = actions,
    )
}

/// The vault cockpit (10.1-02): three panels — My vaults, Shared with me,
/// Load a vault. Both lists render through `vault_row` (structural parity).
async fn index(State(state): State<SharedState>, Extension(claims): Extension<Claims>) -> Response {
    // The listing uses the group + share model (plan 01): own personal vaults
    // plus everything reached through groups or direct email shares.
    let accessible = state
        .store
        .accessible_vaults(&claims.email)
        .await
        .unwrap_or_default();
    let mut own_items = String::new();
    let mut shared_items = String::new();
    for a in &accessible {
        if a.personal {
            own_items.push_str(&vault_row(a, false));
            own_items.push('\n');
        } else {
            // Per-row gate for the Share button (owner | admin | sharee with
            // allow_reshare). A store error degrades to "no Share button".
            let can_share = state
                .store
                .can_share(&claims.email, &a.vault.slug)
                .await
                .unwrap_or(false);
            shared_items.push_str(&vault_row(a, can_share));
            shared_items.push('\n');
        }
    }
    let list = |items: &str, empty: &str| -> String {
        if items.is_empty() {
            format!(r#"<p class="empty">{empty}</p>"#)
        } else {
            format!(r#"<ul class="vault-list">{items}</ul>"#)
        }
    };
    // Phase 12 (D-05): server-rendered locked flag — the popup script is
    // rendered ONLY for a locked account; other users' pages stay byte-identical.
    let locked_script = if is_locked(&state, &claims.email).await {
        LOCKED_ACCOUNT_JS
    } else {
        ""
    };
    // The shared header carries the admin menu (role=admin only) — no extra
    // admin link here. The "Load a vault" panel is always rendered: it is how
    // a user with no vault yet (e.g. the demo account) loads one (REQ-VLT-005).
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Vaults</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  {header}
  <h1>Vaults</h1>
  <section class="panel">
<h2>My vaults</h2>
{own}
</section>
<section class="panel">
<h2>Shared with me</h2>
{shared}
</section>
<section class="panel">
<h2>Load a vault</h2>
<form method="post" action="/vaults/create" class="stacked">
  <div class="field"><label for="lv-name">Name</label><input id="lv-name" type="text" name="name" placeholder="e.g. My Wiki"></div>
  <div class="field field-wide"><label for="lv-url">GitHub URL</label><input id="lv-url" type="url" name="url" required placeholder="https://github.com/owner/repo"></div>
  <div class="field"><label for="lv-token">Token <small>(optional)</small></label><input id="lv-token" type="text" name="token" placeholder="private repos only"></div>
  <button type="submit">Load vault</button>
</form>
{autofill}{locked}
</section>
</body>
</html>"#,
        header = account_menu(&state, &claims).await,
        own = list(&own_items, "No personal vaults yet."),
        shared = list(&shared_items, "No shared vaults yet."),
        autofill = AUTOFILL_NAME_JS,
        locked = locked_script,
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

/// Serve-time override re-showing mdBook's toc.js chapter numbers (D-09):
/// the exact inverse of the global hide rule in `assets/zoom.css`. Static
/// raw-string const — no interpolation (pitfall #25, T-12-12).
const PAGE_NUMBERS_OVERRIDE: &str =
    r#"<style>.chapter-item a > strong[aria-hidden="true"]{display:inline;}</style>"#;

/// The two inline-theme literals present in EVERY built mdBook page (D-10),
/// verified verbatim on a real built book. Exact-match replace only.
const THEME_LIGHT_LITERAL: &str = r#"default_light_theme = "light";"#;
const THEME_DARK_LITERAL: &str = r#"default_dark_theme = "navy";"#;

/// Review fix (LOW, defense-in-depth): the same whitelist the Edit form
/// enforces at write time (plan 12-02) — the serve-time rewrite only ever
/// substitutes one of these tokens, never a raw column value.
const THEME_WHITELIST: [&str; 5] = ["light", "navy", "rust", "coal", "ayu"];

/// Per-vault display options applied to served HTML at serve time (D-08:
/// no rebuild, no book.toml change). `theme` is whitelist-bounded at write
/// time (plan 12-02); this is a fixed-literal substitution, not templating,
/// and is inert when the literals are absent (T-12-11).
fn apply_vault_display_options(html: &mut String, keep_page_numbers: bool, theme: &str) {
    if keep_page_numbers {
        if let Some(pos) = html.rfind("</body>") {
            html.insert_str(pos, PAGE_NUMBERS_OVERRIDE);
        }
    }
    if !theme.is_empty() && THEME_WHITELIST.contains(&theme) {
        // `contains` guards before any `replace` (T-12-13: no work on pages
        // without the literals, no regex engine).
        if html.contains(THEME_LIGHT_LITERAL) {
            let replacement = format!(r#"default_light_theme = "{theme}";"#);
            *html = html.replace(THEME_LIGHT_LITERAL, &replacement);
        }
        if html.contains(THEME_DARK_LITERAL) {
            let replacement = format!(r#"default_dark_theme = "{theme}";"#);
            *html = html.replace(THEME_DARK_LITERAL, &replacement);
        }
    }
}

/// ACL check + safe file serving from the vault's compiled book directory.
/// Issue #9: every served vault page must carry the vault chrome (zoom +
/// Close vault). Converted vaults already wire `theme/zoom.css` +
/// `theme/morinet.js` via mdBook's `additional-css`/`additional-js`; native
/// mdBook vaults have no `theme/` files at all, so inline the chrome assets
/// at serve time. Pages that already reference the chrome get nothing extra.
///
/// Two distinct passes (D-08, CONTEXT pitfall): PASS 1 injects the chrome
/// (skipped for converted pages that already wire it — the early-return must
/// NOT gate the options); PASS 2 applies the per-vault display options to
/// ALL HTML pages, converted or native.
fn inject_vault_chrome(bytes: Vec<u8>, keep_page_numbers: bool, theme: &str) -> Vec<u8> {
    let Ok(html) = std::str::from_utf8(&bytes) else {
        return bytes; // not UTF-8: leave binary/odd payloads alone
    };
    let mut html = html.to_string();
    // PASS 1 — vault chrome. Converted pages already reference zoom.css /
    // morinet.js: skip injection, but fall through to the option pass.
    if !html.contains("zoom.css") && !html.contains("morinet.js") {
        if let Some(pos) = html.rfind("</body>") {
            let mut out = String::with_capacity(html.len() + 8192);
            out.push_str(&html[..pos]);
            out.push_str("<style>");
            out.push_str(crate::obsidian::tree::ZOOM_CSS);
            out.push_str("</style><script>");
            out.push_str(crate::obsidian::tree::MORINET_JS);
            out.push_str("</script>");
            out.push_str(&html[pos..]);
            html = out;
        }
    }
    // PASS 2 — per-vault display options, for every HTML page.
    apply_vault_display_options(&mut html, keep_page_numbers, theme);
    html.into_bytes()
}

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
                Ok(bytes) => {
                    // Successful serve: record the hit without ever letting a
                    // store failure touch the response.
                    log_web_access(state, email, slug, rel);
                    (
                        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                        inject_vault_chrome(
                            bytes,
                            vault.vault.keep_page_numbers,
                            &vault.vault.theme,
                        ),
                    )
                        .into_response()
                }
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        _ => match tokio::fs::read(&full).await {
            Ok(bytes) => {
                log_web_access(state, email, slug, rel);
                let ct = content_type(rel);
                let body = if ct.starts_with("text/html") {
                    inject_vault_chrome(bytes, vault.vault.keep_page_numbers, &vault.vault.theme)
                } else {
                    bytes
                };
                ([(axum::http::header::CONTENT_TYPE, ct)], body).into_response()
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
) -> Result<(PathBuf, bool, PathBuf, String), StatusCode> {
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
    Ok((src, is_admin, book_dir, entry.vault.name.clone()))
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
    let (src_dir, _is_admin, book_dir, vault_name) =
        match dav_resolve(&state, &claims.email, &slug).await {
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
            // Review LOW-02: on a metadata error assume the file existed
            // ("modified") rather than silently mislabeling as "added".
            let existed = tokio::fs::try_exists(&full).await.unwrap_or(true);
            if tokio::fs::write(&full, &bytes).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            // One event per PUT for the single written path (D-02): added on
            // first write, modified on rewrite. Fire-and-forget like
            // log_web_access — the 201 never depends on it (D-04). WebDAV
            // exposes no DELETE verb, so no `removed` rows here.
            {
                let store = state.store.clone();
                let slug_c = slug.clone();
                let rel_c = rel.clone();
                tokio::spawn(async move {
                    let event_id = uuid::Uuid::new_v4().to_string();
                    let kind = if existed { "modified" } else { "added" };
                    if let Err(e) = store
                        .log_vault_change(&slug_c, &event_id, kind, &rel_c)
                        .await
                    {
                        eprintln!("vault-changes: webdav PUT log failed for {slug_c}: {e}");
                    }
                });
            }
            // Rebuild so the served HTML reflects the new note (AD-10).
            // llm-wiki sources are re-converted first (AD-14).
            rebuild_book(&book_dir, &src_dir, &vault_name);
            StatusCode::CREATED.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

/// Make the change go live (AD-10): llm-wiki sources are re-converted into
/// `.morinet-build/` then built; native sources are built directly; a dir
/// matching neither format has nothing to rebuild. Best-effort: a build
/// failure is logged, the write already happened (AD-14 dispatch).
fn rebuild_book(book_dir: &std::path::Path, src_dir: &std::path::Path, vault_name: &str) {
    use crate::obsidian::convert::{convert, detect, VaultFormat};
    let build_dir = match detect(src_dir) {
        VaultFormat::LlmWiki => match convert(src_dir, vault_name) {
            Ok(r) => r.build_dir,
            Err(e) => {
                eprintln!("webdav: llm-wiki convert failed after PUT: {e}");
                return;
            }
        },
        VaultFormat::Native => src_dir.to_path_buf(),
        VaultFormat::Neither => return,
    };
    match std::process::Command::new("mdbook")
        .arg("build")
        .arg(&build_dir)
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

/// The shared header (Phase 9 nav shell): brand + "My vaults" + the account
/// menu (display name editable, email, logout) + the admin menu (role=admin
/// only). Both dropdowns are native `<details>` — no JS needed.
async fn account_menu(state: &SharedState, claims: &Claims) -> String {
    let display = state
        .store
        .display_name_for(&claims.email)
        .await
        .ok()
        .flatten();
    let label = display.unwrap_or_else(|| claims.email.clone());
    let is_admin = state
        .store
        .role_for(&claims.email)
        .await
        .ok()
        .flatten()
        .as_deref()
        == Some("admin");
    // ONE header control: the account menu carries the Admin entry too. The
    // section links live in the tab bar on admin pages — a separate Admin pill
    // next to a menu that says "admin" read as two admin buttons (clutter).
    let admin_entry = if is_admin {
        r#"<hr class="menu-sep">
        <a class="menu-link menu-admin" href="/admin">&#9881; Settings</a>"#
    } else {
        ""
    };
    // First letter of the display name for the avatar chip.
    let initial = label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    // The email is only shown when it differs from the display name — a demo
    // account like `admin`/`admin` would otherwise print the same string 4x.
    let email_line = if claims.email != label {
        format!(
            r#"<span class="menu-email">{}</span>"#,
            html_escape(&claims.email)
        )
    } else {
        String::new()
    };
    format!(
        r#"<header class="mori-nav">
  <a class="brand" href="/"><span class="brand-mark">&#9638;</span> MoriNet</a>
  <nav>
    <details class="menu">
      <summary class="menu-btn"><span class="avatar">{initial}</span> {label} <span class="caret">&#9662;</span></summary>
      <div class="menu-body">
        <a class="menu-identity" href="/account"><strong>{label}</strong>{email_line}<span class="menu-edit">&#9998; Edit display name</span></a>
        {admin_entry}
        <hr class="menu-sep">
        <form method="post" action="/logout"><button type="submit" class="btn-ghost">Log out</button></form>
      </div>
    </details>
  </nav>
</header>
{menu_js}"#,
        admin_entry = admin_entry,
        initial = html_escape(&initial),
        label = html_escape(&label),
        email_line = email_line,
        menu_js = MENU_JS,
    )
}

/// UTC YYYY-MM-DD from unix seconds — civil-from-days algorithm, no chrono
/// (hand-rolled weekday math bit us once; this one is the reference algorithm).
fn fmt_date(unix: i64) -> String {
    let z = unix.div_euclid(86_400) + 719_468;
    let era = (if z >= 0 { z } else { z - 146_097 + 1 }) / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 146);
    let mp = (5 * doy + 2) / 153;
    let d = doe - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe as i64 + era * 400;
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// `GET /account` — the account page: identity + display-name editing
/// (REQ-NAV-002) + API token management (Phase 10). The rename form lives
/// here, not inside the header dropdown: a menu is for navigation, a form is
/// editing (anti-clutter).
async fn account_page(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
) -> Response {
    Html(account_html(&state, &claims, None).await).into_response()
}

/// Renders the account page. `banner` is the one-time plaintext token shown
/// right after creation — it is NEVER rendered again for any later request.
async fn account_html(state: &SharedState, claims: &Claims, banner: Option<&str>) -> String {
    let display = state
        .store
        .display_name_for(&claims.email)
        .await
        .ok()
        .flatten();
    let label = display.unwrap_or_else(|| claims.email.clone());
    let header = account_menu(state, claims).await;
    let uid = state.store.user_id_for(&claims.email).await.ok().flatten();
    let tokens = match uid {
        Some(uid) => state.store.list_api_tokens(uid).await.unwrap_or_default(),
        None => Vec::new(),
    };
    let rows: String = tokens
        .iter()
        .map(|t| {
            let used = match t.last_used_at {
                Some(ts) => fmt_date(ts),
                None => "never".to_string(),
            };
            format!(
                "<tr><td>{name}</td><td>{created}</td><td>{used}</td><td><form method=\"post\" action=\"/account/tokens/revoke\"><input type=\"hidden\" name=\"id\" value=\"{id}\"><button type=\"submit\">Revoke</button></form></td></tr>",
                name = html_escape(&t.name),
                created = fmt_date(t.created_at),
                used = used,
                id = t.id,
            )
        })
        .collect();
    let banner_html = match banner {
        Some(plain) => format!(
            "<p class=\"banner\">New token <strong>{name}</strong> — copy it now, it is shown only once:<br><code>{plain}</code></p>",
            name = "token",
            plain = html_escape(plain),
        ),
        None => String::new(),
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — Account</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  {header}
  <h1 class="page-title">Account</h1>
  <section class="panel">
    <h2>Profile</h2>
    <p><strong>{label}</strong><br><small>{email}</small></p>
    <form method="post" action="/account/name" class="stacked">
      <div class="field field-wide"><label for="display-name">Display name</label><input id="display-name" type="text" name="name" value="{label}" placeholder="display name"></div>
      <button type="submit">Save</button>
    </form>
  </section>
  <section class="panel">
    <h2>API tokens</h2>
    {banner_html}
    <p>Tokens authenticate the MCP endpoint (<code>/mcp</code>) with <code>Authorization: Bearer …</code>. Shown once, revocable.</p>
    <table>
      <thead><tr><th>Name</th><th>Created</th><th>Last used</th><th></th></tr></thead>
      <tbody>{rows}</tbody>
    </table>
    <form method="post" action="/account/tokens" class="stacked">
      <div class="field field-wide"><label for="token-name">New token name</label><input id="token-name" type="text" name="name" placeholder="e.g. laptop" required></div>
      <button type="submit">Add token</button>
    </form>
  </section>
</body>
</html>"#,
        label = html_escape(&label),
        email = html_escape(&claims.email),
    )
}

#[derive(serde::Deserialize)]
struct TokenParams {
    name: String,
}

#[derive(serde::Deserialize)]
struct TokenRevokeParams {
    id: i64,
}

/// `POST /account/tokens` — mint a named API token; the plaintext is rendered
/// exactly once in the response, only its sha256 lives in the DB.
async fn create_account_token(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Form(p): Form<TokenParams>,
) -> Response {
    let name = p.name.trim();
    if name.is_empty() {
        return error_page("the token needs a name");
    }
    let uid = match state.store.user_id_for(&claims.email).await.ok().flatten() {
        Some(uid) => uid,
        None => return error_page("account not found"),
    };
    match state.store.create_api_token(uid, name).await {
        Ok((_id, plain)) => Html(account_html(&state, &claims, Some(&plain)).await).into_response(),
        Err(e) => error_page(&format!("could not create the token: {e}")),
    }
}

/// `POST /account/tokens/revoke` — delete one of the user's own tokens.
async fn revoke_account_token(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Form(p): Form<TokenRevokeParams>,
) -> Response {
    if let Some(uid) = state.store.user_id_for(&claims.email).await.ok().flatten() {
        let _ = state.store.revoke_api_token(uid, p.id).await;
    }
    Redirect::to("/account").into_response()
}

/// `POST /account/name` — the signed-in user renames themselves (REQ-NAV-002).
async fn set_account_name(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Form(p): Form<NameParams>,
) -> Response {
    let name = p.name.trim();
    if let Err(e) = state.store.set_display_name(&claims.email, name).await {
        return error_page(&format!("could not save the display name: {e}"));
    }
    Redirect::to("/").into_response()
}

async fn admin_page(state: &SharedState, claims: &Claims, title: &str, body: String) -> Response {
    let header = account_menu(state, claims).await;
    // Mark the current section in the pill nav ("Vault permissions" → Vaults).
    let active = |pill: &str| {
        if title == pill || title.starts_with(pill.trim_end_matches('s')) {
            " class=\"active\""
        } else {
            ""
        }
    };
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>MoriNet — {title}</title>
<link rel="stylesheet" href="/assets/style.css"></head>
<body>
  {header}
  <h1 class="page-title">Settings</h1>
  <p class="settings-exit"><a href="/">Exit Settings</a></p>
  <nav class="subnav"><a{users} href="/admin/users">Users</a><a{groups} href="/admin/groups">Groups</a><a{vaults} href="/admin/vaults">Vaults</a><a{about} href="/admin/about">About</a></nav>
  <h2>{title}</h2>
  {body}
</body>
</html>"#,
        users = active("Users"),
        groups = active("Groups"),
        vaults = active("Vaults"),
        about = active("About"),
    ))
    .into_response()
}

/// `GET /admin/about` — installed version + source repo (REQ-ADMIN-007).
/// The version comes from `crate::version()` (compile-time `CARGO_PKG_VERSION`),
/// so it tracks every release automatically — nothing to hardcode.
async fn admin_about(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
) -> Response {
    let body = format!(
        r#"<p><strong>morinet {version}</strong></p>
<p>Source: <a href="{repo}">{repo}</a></p>"#,
        version = crate::version(),
        repo = REPO_URL
    );
    admin_page(&state, &claims, "About", body).await
}

async fn admin_dashboard(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
) -> Response {
    let users = state.store.list_users().await.unwrap_or_default();
    let vaults = state.store.list_all_vaults().await.unwrap_or_default();
    let body = format!("<p>{} user(s), {} vault(s)</p>", users.len(), vaults.len());
    admin_page(&state, &claims, "Dashboard", body).await
}

/// `POST /admin/users/role` — switch a user between reader and admin
/// (REQ-GRP-007). The store also fixes the plan group (`free` / `admin`).
async fn admin_user_role(
    State(state): State<SharedState>,
    Form(p): Form<UserRoleParams>,
) -> Response {
    match state
        .store
        .set_user_role(p.email.trim(), p.role.trim())
        .await
    {
        Ok(()) => Redirect::to("/admin/users").into_response(),
        Err(e) => {
            let body = format!(
                r#"<p style="color:red">Could not set the role: {}</p>
<p><a href="/admin/users">Back</a></p>"#,
                html_escape(&e.to_string())
            );
            (StatusCode::BAD_REQUEST, Html(body)).into_response()
        }
    }
}

async fn admin_users(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
) -> Response {
    let users = state.store.list_users().await.unwrap_or_default();
    let mut rows = String::new();
    for u in &users {
        let reader_sel = if u.role != "admin" { " selected" } else { "" };
        let admin_sel = if u.role == "admin" { " selected" } else { "" };
        rows.push_str(&format!(
            r#"<tr><td>{email}</td><td>{role}</td><td>
<form method="post" action="/admin/users/role" style="display:inline">
  <input type="hidden" name="email" value="{email}">
  <select name="role"><option value="reader"{reader_sel}>reader</option><option value="admin"{admin_sel}>admin</option></select>
  <button type="submit">Set role</button>
</form></td></tr>
"#,
            email = html_escape(&u.email),
            role = html_escape(&u.role),
        ));
    }
    let body = format!(
        r#"<table border="1" cellpadding="4"><tr><th>Email</th><th>Role</th><th>Change role</th></tr>
{rows}</table>
<h3>Invite a user</h3>
<form method="post" action="/admin/users/invite" novalidate>
  <label>Login <input type="text" name="email" required></label>
  <label>Role <select name="role"><option value="reader">reader</option><option value="admin">admin</option></select></label>
  <button type="submit">Invite</button>
</form>"#
    );
    admin_page(&state, &claims, "Users", body).await
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

async fn admin_invite(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Form(p): Form<InviteParams>,
) -> Response {
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
        return admin_page(&state, &claims, "Invitation sent", body).await;
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
            admin_page(&state, &claims, "User invited", body).await
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
                // The vault form wants an ABSOLUTE server path; `path` above is
                // root-relative but starts with `/`, which would REPLACE the
                // base in `join` — strip it first.
                "abs": root.join(d.path.trim_start_matches('/')).display().to_string(),
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

/// `POST /admin/groups/{id}/limit` — quota input (empty = unlimited).
#[derive(Debug, Deserialize)]
struct GroupLimitParams {
    #[serde(default)]
    limit: String,
}

/// `POST /account/name` — display-name input (REQ-NAV-002).
#[derive(Debug, Deserialize)]
struct NameParams {
    #[serde(default)]
    name: String,
}

/// `POST /vaults/create` — personal vault load input (REQ-VLT-005).
#[derive(Debug, Deserialize)]
struct LoadVaultParams {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    token: String,
}

/// `POST /admin/users/role` — role-switch input (REQ-GRP-007).
#[derive(Debug, Deserialize)]
struct UserRoleParams {
    #[serde(default)]
    email: String,
    #[serde(default)]
    role: String,
}

async fn admin_groups(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
) -> Response {
    let groups = state.store.list_groups().await.unwrap_or_default();
    let users = state.store.list_users().await.unwrap_or_default();

    let mut blocks = String::new();
    for (gid, name, count, limit) in &groups {
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
            r#"<h3>{name} <small>({count} member(s), quota: {quota})</small></h3>
<form method="post" action="/admin/groups/{gid}/rename" style="display:inline">
  <input type="text" name="name" value="{name}" aria-label="group name">
  <button type="submit">Rename</button>
</form>
<form method="post" action="/admin/groups/{gid}/delete" style="display:inline;margin-left:8px">
  <button type="submit">Delete group</button>
</form>
<form method="post" action="/admin/groups/{gid}/limit" style="display:inline;margin-left:8px">
  <input type="number" name="limit" min="0" value="{limit_val}" style="width:5em" aria-label="personal vault limit">
  <button type="submit">Set quota</button>
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
            quota = match limit {
                Some(n) => format!("{n} personal vault(s)"),
                None => "unlimited".to_string(),
            },
            limit_val = limit.map(|n| n.to_string()).unwrap_or_default(),
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
    admin_page(&state, &claims, "Groups", body).await
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

/// The server-side directory picker (Phase 7 / REQ-UX-001): a native
/// `<dialog>` that lists the allowed root via /admin/browse, descends into
/// dirs and fills the vault form on selection. No framework, no build step.
const PICKER_JS: &str = r#"(function () {
  function slugify(s) {
    return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  }
  function loadPicker(path) {
    fetch("/admin/browse?path=" + encodeURIComponent(path))
      .then(function (r) { return r.json(); })
      .then(function (d) {
        if (d.error) { alert(d.error); return; }
        document.getElementById("picker-path").textContent = d.current;
        var ul = document.getElementById("picker-list");
        ul.innerHTML = "";
        (d.dirs || []).forEach(function (dir) {
          var li = document.createElement("li");
          var desc = document.createElement("button");
          desc.type = "button";
          desc.textContent = dir.name + " \u25B8";
          desc.className = "picker-descend";
          desc.addEventListener("click", function () { loadPicker(dir.path); });
          var sel = document.createElement("button");
          sel.type = "button";
          sel.textContent = "Select";
          sel.className = "picker-select";
          sel.addEventListener("click", function () {
            // Scope to the vault form: a bare [name="name"] would hit the
            // account menu's rename input in the shared header (pitfall).
            var form = document.querySelector('form[action="/admin/vaults/create"]');
            document.getElementById("lo-path").value = dir.abs || dir.path;
            var slug = form.querySelector('[name="slug"]');
            var name = form.querySelector('[name="name"]');
            if (slug && !slug.value) slug.value = slugify(dir.name);
            if (name && !name.value) name.value = dir.name;
            document.getElementById("dir-picker").close();
          });
          li.appendChild(desc);
          li.appendChild(sel);
          if (dir.is_vault) {
            var tag = document.createElement("small");
            tag.textContent = " (already a vault)";
            li.appendChild(tag);
          }
          ul.appendChild(li);
        });
        var up = document.getElementById("picker-up");
        up.style.display = d.parent === null || d.parent === undefined ? "none" : "";
        up.onclick = function () { loadPicker(d.parent); };
      });
  }
  window.openPicker = function () {
    loadPicker("/");
    document.getElementById("dir-picker").showModal();
  };
})();"#;

/// Menu ergonomics: one open menu at a time, Escape/outside-click closes
/// (native `<details>` does none of that on its own).
const MENU_JS: &str = r#"<script>
// menu-ergonomics: one open menu at a time; Escape or an outside click closes it.
(function () {
  var menus = function () { return Array.prototype.slice.call(document.querySelectorAll("details.menu")); };
  document.addEventListener("click", function (e) {
    menus().forEach(function (d) {
      if (d.open && !d.contains(e.target)) d.removeAttribute("open");
    });
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") menus().forEach(function (d) { d.removeAttribute("open"); });
  });
})();
</script>"#;

/// Inline script on the index: the "Name" field defaults to the repository
/// directory name, live as the URL is typed or pasted. A name typed by hand is
/// never overwritten (UX feedback, 2026-10-21).
const AUTOFILL_NAME_JS: &str = r#"<script>
// morinet-autofill-name: default the vault name to the repository directory.
(function () {
  var url = document.querySelector('input[name="url"]');
  var name = document.querySelector('input[name="name"]');
  if (!url || !name) return;
  var touched = false;
  name.addEventListener("input", function () { touched = true; });
  function repoOf(u) {
    var t = (u || "").trim().replace(/\/+$/, "");
    if (!t || t.indexOf("://") === -1) return "";
    var last = t.split("/").pop();
    return last.replace(/\.git$/, "");
  }
  function sync() {
    if (touched) return;
    var r = repoOf(url.value);
    if (r) name.value = r;
  }
  url.addEventListener("input", sync);
  url.addEventListener("paste", function () { setTimeout(sync, 0); });
  sync();
})();
</script>"#;

/// Phase 12 (D-04/D-05): rendered ONLY for a locked account — intercepts the
/// Load-vault submit and the Delete confirm with the required popup. UX only;
/// the server-side `is_locked` gates are the security boundary (T-12-01).
/// Raw-string const: brace-bearing JS never goes inside a `format!` (pitfall #25).
const LOCKED_ACCOUNT_JS: &str = r#"<script>
// morinet-locked-account: demo guard popup (UX only; the server refuses too).
(function () {
  var MSG = "demo account can't add or suppress vaults";
  function block(form) {
    // Strip the inline confirm FIRST: removeAttribute("onsubmit") unregisters
    // the handler slot, so assigning the property before stripping would be
    // wiped (checker fix W-4, order corrected — see 12-01-SUMMARY).
    form.removeAttribute("onsubmit");
    form.onsubmit = function (e) {
      e.preventDefault();
      alert(MSG);
      return false;
    };
  }
  function init() {
    var load = document.querySelector('form[action="/vaults/create"]');
    if (load) block(load);
    var dels = document.querySelectorAll('form[action$="/delete"]');
    for (var i = 0; i < dels.length; i++) block(dels[i]);
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
</script>"#;

/// The `origin` URL of a local git clone (used to recover a vault's upstream
/// when it was loaded before the source URL was recorded).
fn git_remote_url(clone_dir: &std::path::Path) -> Option<String> {
    let repo = git2::Repository::open(clone_dir).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?.to_string();
    // Normalize file:// URLs to a plain path-like URL the clone can re-use.
    Some(url)
}

/// The repository/directory name from a GitHub-style URL: the last path
/// segment, without a `.git` suffix (UX: the vault name defaults to it).
fn repo_name_from_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() || !trimmed.contains("://") {
        return String::new();
    }
    let last = trimmed.rsplit('/').next().unwrap_or("");
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

/// `POST /vaults/{slug}/delete` — the owner removes their personal vault.
/// Owner-only (the store enforces it); the quota slot is freed at once.
async fn delete_my_vault(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    if is_locked(&state, &claims.email).await {
        return error_page("demo account can't add or suppress vaults");
    }
    match state
        .store
        .delete_personal_vault(&claims.email, &slug)
        .await
    {
        Ok(true) => Redirect::to("/").into_response(),
        Ok(false) => StatusCode::FORBIDDEN.into_response(),
        Err(e) => error_page(&format!("could not delete the vault: {e}")),
    }
}

/// `POST /vaults/{slug}/refresh` — re-pull the vault's upstream and rebuild so
/// pages added after the load appear without reloading (REQ-VLT-005).
/// Owner gate for the Edit page (LOCKED decision: owner-only, admins keep
/// their global power). A `personal` row in `accessible_vaults` is only ever
/// returned to the vault's owner (plan 01), so "has a personal row for this
/// slug" is exactly "is the owner" — no new store query needed.
async fn can_edit_vault(state: &SharedState, email: &str, slug: &str) -> bool {
    let role = state.store.role_for(email).await.ok().flatten();
    if role.as_deref() == Some("admin") {
        return true;
    }
    state
        .store
        .accessible_vaults(email)
        .await
        .unwrap_or_default()
        .iter()
        .any(|a| a.personal && a.vault.slug == slug)
}

/// Phase 12 (D-01/D-02): is `email` a locked (demo) account? Case-insensitive
/// against `cfg.locked_accounts`; admins are NEVER locked. Async because the
/// admin exemption needs `store.role_for` (checker fix W-3).
async fn is_locked(state: &SharedState, email: &str) -> bool {
    if state.cfg.locked_accounts.is_empty() {
        return false;
    }
    let role = state.store.role_for(email).await.ok().flatten();
    if role.as_deref() == Some("admin") {
        return false;
    }
    state
        .cfg
        .locked_accounts
        .iter()
        .any(|locked| locked.eq_ignore_ascii_case(email))
}

/// `POST /vault/{slug}/edit` fields. `allow_reshare` follows HTML checkbox
/// semantics: present (any value) = on, absent = off.
#[derive(Debug, Deserialize)]
struct EditForm {
    name: String,
    #[serde(default)]
    url: String,
    allow_reshare: Option<String>,
    /// Checkbox semantics: field present = on, absent = off (like
    /// `allow_reshare`). REQ-VOPT-001.
    #[serde(default)]
    keep_page_numbers: Option<String>,
    /// mdBook theme name, `""` = auto (REQ-VOPT-002). Validated against the
    /// server-side whitelist in `vault_edit_submit` — reject, never coerce.
    #[serde(default)]
    theme: String,
}

/// `GET /vault/{slug}/edit` — owner-only settings form (REQ-VLT-009).
async fn vault_edit_page(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    if !can_edit_vault(&state, &claims.email, &slug).await {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let accessible = state
        .store
        .accessible_vaults(&claims.email)
        .await
        .unwrap_or_default();
    let Some(a) = accessible.iter().find(|a| a.vault.slug == slug) else {
        return error_page("vault not found");
    };
    let source = state
        .store
        .vault_source(&slug)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let reshare = state.store.allow_reshare(&slug).await.unwrap_or(false);
    let keep = state.store.keep_page_numbers(&slug).await.unwrap_or(false);
    let theme = state.store.theme(&slug).await.unwrap_or_default();
    // D-11: the theme select renders only for converted (llm-wiki pipeline)
    // vaults — converted ⇔ book_path contains the `.morinet-build` path
    // component (precedent: `source_dir_of` in webdav.rs). A native vault's
    // owner edits book.toml directly. The page-numbers checkbox shows for
    // every personal vault.
    let converted = std::path::Path::new(&a.vault.book_path)
        .components()
        .any(|c| c.as_os_str() == ".morinet-build");
    let theme_field = if converted {
        let sel = |v: &str| if theme == v { " selected" } else { "" };
        format!(
            r#"<div class="field"><label for="ev-theme">Theme</label><select id="ev-theme" name="theme"><option value=""{t_auto}>auto</option><option value="light"{t_light}>light</option><option value="navy"{t_navy}>navy</option><option value="rust"{t_rust}>rust</option><option value="coal"{t_coal}>coal</option><option value="ayu"{t_ayu}>ayu</option></select></div>"#,
            t_auto = sel(""),
            t_light = sel("light"),
            t_navy = sel("navy"),
            t_rust = sel("rust"),
            t_coal = sel("coal"),
            t_ayu = sel("ayu"),
        )
    } else {
        String::new()
    };
    let body = format!(
        r#"<section class="panel">
<h2>Edit vault</h2>
<form method="post" action="/vault/{s}/edit" class="stacked">
  <div class="field"><label for="ev-name">Name</label><input id="ev-name" type="text" name="name" value="{n}" required></div>
  <div class="field field-wide"><label for="ev-url">Source URL <small>(next Refresh pulls this repo)</small></label><input id="ev-url" type="url" name="url" value="{u}" placeholder="https://github.com/owner/repo"></div>
  <div class="field"><label><input id="ev-reshare" type="checkbox" name="allow_reshare"{c}> Allow sharees to re-share this vault</label></div>
  <div class="field"><label><input id="ev-keep" type="checkbox" name="keep_page_numbers"{k}> Keep chapter page numbers</label></div>{tf}
  <button type="submit">Save</button>
</form>
</section>
"#,
        s = html_escape(&slug),
        n = html_escape(&a.vault.name),
        u = html_escape(&source),
        c = if reshare { " checked" } else { "" },
        k = if keep { " checked" } else { "" },
        tf = theme_field,
    );
    nav_page(&state, &claims, "Edit vault", &body).await
}

/// `POST /vault/{slug}/edit` — persist rename / re-source / re-share toggle,
/// then return to the index (303 so a refresh does not resubmit).
async fn vault_edit_submit(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
    Form(p): Form<EditForm>,
) -> Response {
    if !can_edit_vault(&state, &claims.email, &slug).await {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    if is_locked(&state, &claims.email).await {
        return error_page("demo account can't add or suppress vaults");
    }
    // D-07: server-side theme whitelist — an unknown value rejects the whole
    // POST (400), it is never coerced to a default. `""` = mdBook auto.
    const THEMES: [&str; 6] = ["", "light", "navy", "rust", "coal", "ayu"];
    if !THEMES.contains(&p.theme.as_str()) {
        return error_page(&format!("unknown theme \"{}\"", p.theme));
    }
    let name = p.name.trim();
    if name.is_empty() {
        return error_page("the name is required");
    }
    if let Err(e) = state.store.rename_vault(&slug, name).await {
        return error_page(&format!("could not rename the vault: {e}"));
    }
    // An empty URL field means "keep the current source" — never wipe it.
    let url = p.url.trim();
    if !url.is_empty() {
        if let Err(e) = state.store.set_vault_source(&slug, url).await {
            return error_page(&format!("could not set the source URL: {e}"));
        }
    }
    if let Err(e) = state
        .store
        .set_allow_reshare(&slug, p.allow_reshare.is_some())
        .await
    {
        return error_page(&format!("could not update the re-share setting: {e}"));
    }
    if let Err(e) = state
        .store
        .set_keep_page_numbers(&slug, p.keep_page_numbers.is_some())
        .await
    {
        return error_page(&format!("could not update the page-numbers setting: {e}"));
    }
    if let Err(e) = state.store.set_theme(&slug, &p.theme).await {
        return error_page(&format!("could not update the theme: {e}"));
    }
    Redirect::to("/").into_response()
}

/// `POST /vault/{slug}/share` fields: sharee address + role.
#[derive(Debug, Deserialize)]
struct ShareAddForm {
    #[serde(default)]
    email: String,
    #[serde(default)]
    role: String,
}

/// `POST /vault/{slug}/share/revoke` fields.
#[derive(Debug, Deserialize)]
struct ShareEmailForm {
    #[serde(default)]
    email: String,
}

/// `GET /vault/{slug}/share` — share table + add form (REQ-VLT-010). Gated by
/// `can_share`: owner, admin, or a sharee of a re-shareable vault.
async fn vault_share_page(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    if !state
        .store
        .can_share(&claims.email, &slug)
        .await
        .unwrap_or(false)
    {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let shares = state.store.list_shares(&slug).await.unwrap_or_default();
    let mut rows = String::new();
    for (email, role, hidden, created) in &shares {
        // Unknown addresses get a pending hint: the sharer typed the address
        // themselves, so this is not cross-user enumeration (T-10.1-08).
        let pending = match state.store.user_exists(email).await {
            Ok(false) => " <small class=\"muted\">pending — not registered yet</small>",
            _ => "",
        };
        let hidden_mark = if *hidden {
            " <small class=\"muted\">hidden from your view</small>"
        } else {
            ""
        };
        rows.push_str(&format!(
            r#"<tr><td>{e}{p}{h}</td><td>{r}</td><td>{c}</td><td><form method="post" action="/vault/{s}/share/revoke"><input type="hidden" name="email" value="{e}"><button type="submit" class="btn-quiet">Revoke</button></form></td></tr>"#,
            e = html_escape(email),
            p = pending,
            h = hidden_mark,
            r = html_escape(role),
            c = html_escape(created),
            s = html_escape(&slug),
        ));
    }
    let table = if rows.is_empty() {
        r#"<p class="empty">Not shared with anyone yet.</p>"#.to_string()
    } else {
        format!(
            r#"<table><thead><tr><th>Sharee</th><th>Role</th><th>Since</th><th></th></tr></thead><tbody>{rows}</tbody></table>"#
        )
    };
    let name = state
        .store
        .vault_name(&slug)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| slug.clone());
    let body = format!(
        r#"<section class="panel">
<h2>Share "{n}"</h2>
{table}
<form method="post" action="/vault/{s}/share" class="stacked">
  <div class="field"><label for="sh-email">Email</label><input id="sh-email" type="email" name="email" required></div>
  <div class="field"><label for="sh-role">Role</label><select id="sh-role" name="role"><option value="reader">reader</option><option value="writer">writer</option></select></div>
  <button type="submit">Share</button>
</form>
</section>
"#,
        n = html_escape(&name),
        s = html_escape(&slug),
    );
    nav_page(&state, &claims, "Share vault", &body).await
}

/// `POST /vault/{slug}/share` — upsert a share (email may be unregistered;
/// the page shows it as pending until the account exists).
async fn vault_share_add(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
    Form(p): Form<ShareAddForm>,
) -> Response {
    if !state
        .store
        .can_share(&claims.email, &slug)
        .await
        .unwrap_or(false)
    {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    if is_locked(&state, &claims.email).await {
        return error_page("demo account can't add or suppress vaults");
    }
    let email = p.email.trim();
    if email.is_empty() {
        return error_page("the email is required");
    }
    let role = if p.role == "writer" {
        "writer"
    } else {
        "reader"
    };
    if let Err(e) = state.store.share_vault(&slug, email, role).await {
        return error_page(&format!("could not share the vault: {e}"));
    }
    // Safe: share_vault succeeded, so `slug` is a registered vault slug.
    Redirect::to(&format!("/vault/{slug}/share")).into_response()
}

/// `POST /vault/{slug}/share/revoke` — delete a share row outright.
async fn vault_share_revoke(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
    Form(p): Form<ShareEmailForm>,
) -> Response {
    if !state
        .store
        .can_share(&claims.email, &slug)
        .await
        .unwrap_or(false)
    {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    if is_locked(&state, &claims.email).await {
        return error_page("demo account can't add or suppress vaults");
    }
    let email = p.email.trim();
    if email.is_empty() {
        return error_page("the email is required");
    }
    if let Err(e) = state.store.revoke_share(&slug, email).await {
        return error_page(&format!("could not revoke the share: {e}"));
    }
    Redirect::to(&format!("/vault/{slug}/share")).into_response()
}

/// `POST /vault/{slug}/share/hide` — "remove from my view": sets the hidden
/// flag on the CALLER's own direct share only. Any direct sharee may do it;
/// the owner's share list is untouched.
async fn vault_share_hide(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    let is_direct_sharee = state
        .store
        .accessible_vaults(&claims.email)
        .await
        .unwrap_or_default()
        .iter()
        .any(|a| a.vault.slug == slug && a.direct_share);
    if !is_direct_sharee {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    if let Err(e) = state.store.hide_share(&slug, &claims.email).await {
        return error_page(&format!("could not hide the vault: {e}"));
    }
    Redirect::to("/").into_response()
}

/// Recursive byte sum over a directory. Unreadable entries are skipped and a
/// missing dir yields 0 — the stats panel degrades to "—", never a 500.
fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            total += dir_size_bytes(&p);
        } else if let Ok(m) = std::fs::metadata(&p) {
            total += m.len();
        }
    }
    total
}

/// Human-readable size: plain bytes below 1 KiB, then KB/MB/GB with one decimal.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Fire-and-forget web hit (REQ-VLT-012): the store error is dropped inside
/// the spawned task, so a logging failure can never change the response.
fn log_web_access(state: &AppState, email: &str, slug: &str, path: &str) {
    let store = state.store.clone();
    let email = email.to_string();
    let slug = slug.to_string();
    let path = path.to_string();
    tokio::spawn(async move {
        let _ = store.log_access(&email, &slug, "web", &path).await;
    });
}

/// `GET /vault/{slug}/stats` (REQ-VLT-011). Owner/admin: full panel. Any
/// other viewer WITH access (direct share hidden=0 or group grant — proven by
/// their `accessible_vaults` row): ONLY their own hit count. No access: 403.
async fn vault_stats(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    let is_admin = state
        .store
        .role_for(&claims.email)
        .await
        .ok()
        .flatten()
        .as_deref()
        == Some("admin");
    let accessible = state
        .store
        .accessible_vaults(&claims.email)
        .await
        .unwrap_or_default();
    let Some(entry) = accessible.iter().find(|a| a.vault.slug == slug) else {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    };
    if !(entry.personal || is_admin) {
        // Sharee branch: a single line, nothing from the owner panel (T-10.1-07).
        let n = state
            .store
            .stats_own_count(&slug, &claims.email)
            .await
            .unwrap_or(0);
        let body = format!(
            r#"<section class="panel">
<h2>Statistics</h2>
<p>You have accessed this vault {n} time(s).</p>
</section>
"#
        );
        return nav_page(&state, &claims, "Statistics", &body).await;
    }
    let book_dir = PathBuf::from(&entry.vault.book_path);
    let pages = webdav::source_dir_of(&book_dir)
        .map(|d| crate::mcp::list_md_files(&d).len().to_string())
        .unwrap_or_else(|| "—".to_string());
    let size = if book_dir.is_dir() {
        human_size(dir_size_bytes(&book_dir))
    } else {
        "—".to_string()
    };
    let last_refresh = state
        .store
        .vault_last_refreshed(&slug)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "—".to_string());
    let source = state
        .store
        .vault_source(&slug)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    // Paths in the tables come from the request URL (attacker-controlled) —
    // escape them like every other interpolated string (T-10.1-09).
    let day_rows: String = state
        .store
        .stats_per_day(&slug)
        .await
        .unwrap_or_default()
        .iter()
        .map(|(d, src, n)| {
            format!(
                r#"<tr><td>{d}</td><td>{s}</td><td>{n}</td></tr>"#,
                d = html_escape(d),
                s = html_escape(src),
                n = n
            )
        })
        .collect();
    let top_rows: String = state
        .store
        .stats_top_pages(&slug)
        .await
        .unwrap_or_default()
        .iter()
        .map(|(p, n)| {
            format!(
                r#"<tr><td>{p}</td><td>{n}</td></tr>"#,
                p = html_escape(p),
                n = n
            )
        })
        .collect();
    // Wiki changes section (D-06): best-effort like the sibling stats
    // queries — a read error degrades to the empty state, never breaks the
    // page. ts/kind/path all get html_escape: paths come from repo filenames
    // and WebDAV PUT targets (attacker-controlled, T-13-08, same rule as
    // T-10.1-09 above).
    let changes = state
        .store
        .recent_vault_changes(&slug, 5, 30)
        .await
        .unwrap_or_default();
    // Group consecutive rows sharing event_id (query is already newest-first).
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for (event_id, ts, kind, path) in changes {
        // D-08: display prettification only; stored vocabulary stays exact.
        let mut label = kind;
        if let Some(first) = label.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let row = format!(
            r#"<tr><td>{k}</td><td>{p}</td></tr>"#,
            k = html_escape(&label),
            p = html_escape(&path)
        );
        match groups.last_mut() {
            Some((eid, _, rows)) if *eid == event_id => rows.push(row),
            _ => groups.push((event_id, ts, vec![row])),
        }
    }
    let changes_html: String = if groups.is_empty() {
        "<p>No changes recorded yet.</p>".to_string()
    } else {
        groups
            .iter()
            .map(|(_, ts, rows)| {
                format!(
                    r#"<h3>{ts} — {n} change(s)</h3>
<table><thead><tr><th>Change</th><th>Path</th></tr></thead><tbody>{rows}</tbody></table>"#,
                    ts = html_escape(ts),
                    n = rows.len(),
                    rows = rows.concat(),
                )
            })
            .collect()
    };
    let body = format!(
        r#"<section class="panel">
<h2>Statistics</h2>
<dl class="stats">
  <dt>Pages</dt><dd>{pages}</dd>
  <dt>Disk size</dt><dd>{size}</dd>
  <dt>Last refresh</dt><dd>{lr}</dd>
  <dt>Source</dt><dd>{src}</dd>
</dl>
<h2>Activity by day</h2>
<table><thead><tr><th>Day</th><th>Source</th><th>Hits</th></tr></thead><tbody>{day_rows}</tbody></table>
<h2>Top pages</h2>
<table><thead><tr><th>Page</th><th>Hits</th></tr></thead><tbody>{top_rows}</tbody></table>
<h2>Wiki changes</h2>
{changes_html}
</section>
"#,
        pages = pages,
        size = size,
        lr = html_escape(&last_refresh),
        src = html_escape(&source),
        day_rows = day_rows,
        top_rows = top_rows,
        changes_html = changes_html,
    );
    nav_page(&state, &claims, "Statistics", &body).await
}

/// HEAD oid of a clone dir; `None` if the dir is missing or unborn.
fn git_head_oid(repo_dir: &std::path::Path) -> Option<git2::Oid> {
    let repo = git2::Repository::open(repo_dir).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    Some(commit.id())
}

/// Diff two commit trees → `(kind, repo-relative path)` per changed file
/// (D-01). No `find_similar`: renames surface as removed + added (D-08).
fn diff_commit_trees(
    repo_dir: &std::path::Path,
    old: git2::Oid,
    new: git2::Oid,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    let repo = git2::Repository::open(repo_dir)?;
    let old_tree = repo.find_commit(old)?.tree()?;
    let new_tree = repo.find_commit(new)?.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;
    let mut out = Vec::new();
    for delta in diff.deltas() {
        let kind = match delta.status() {
            git2::Delta::Added => "added",
            git2::Delta::Modified => "modified",
            git2::Delta::Deleted => "removed",
            _ => continue,
        };
        let path = match delta.status() {
            git2::Delta::Deleted => delta.old_file().path(),
            _ => delta.new_file().path(),
        };
        if let Some(p) = path {
            // Review LOW-01: git paths are slash-separated on every platform,
            // so no backslash normalization — a backslash is a legal Linux
            // filename character and must survive untouched.
            out.push((kind, p.to_string_lossy().into_owned()));
        }
    }
    Ok(out)
}

/// Diff → rows: one uuid v4 `event_id` per refresh (D-03), one row per
/// changed path. Errors propagate to the caller's best-effort `Err` arm (D-04).
async fn record_vault_changes(
    store: &Store,
    slug: &str,
    repo_dir: &std::path::Path,
    old: git2::Oid,
    new: git2::Oid,
) -> anyhow::Result<()> {
    let changes = diff_commit_trees(repo_dir, old, new)?;
    if changes.is_empty() {
        return Ok(());
    }
    let event_id = uuid::Uuid::new_v4().to_string();
    for (kind, path) in changes {
        store.log_vault_change(slug, &event_id, kind, &path).await?;
    }
    Ok(())
}

async fn refresh_my_vault(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    let access = match state.store.access_for(&claims.email, &slug).await {
        Ok(a) => a,
        Err(e) => return error_page(&format!("access check failed: {e}")),
    };
    if !access.can_write() {
        return StatusCode::FORBIDDEN.into_response();
    }
    // The upstream URL is normally remembered at load time. For a vault loaded
    // before that column existed, recover it from the clone's git remote.
    let url = match state.store.vault_source(&slug).await.ok().flatten() {
        Some(u) => u,
        None => match git_remote_url(&state.cfg.data_dir.join("repos").join(&slug)) {
            Some(u) => {
                let _ = state.store.set_vault_source(&slug, &u).await;
                u
            }
            None => {
                return error_page(
                    "this vault has no upstream to refresh (it was not loaded from GitHub)",
                )
            }
        },
    };
    let Some(name) = state.store.vault_name(&slug).await.ok().flatten() else {
        return error_page("unknown vault");
    };
    use crate::sources::VaultSource as _;
    let repo_dir = state.cfg.data_dir.join("repos").join(&slug);
    let old_head = git_head_oid(&repo_dir);
    let src = crate::sources::GitSource::github(url, None, repo_dir.clone());
    let book_dir = match src.sync().and_then(|d| src.build(&d, &name)) {
        Ok(b) => b,
        Err(e) => return error_page(&format!("refresh failed: {e}")),
    };
    match state.store.seed_vault(&slug, &name, &book_dir).await {
        Ok(()) => {
            // Best-effort change log (D-04): only when HEAD actually moved;
            // any failure is eprintln'd and the redirect stays byte-identical.
            if let (Some(old), Some(new)) = (old_head, git_head_oid(&repo_dir)) {
                if old != new {
                    if let Err(e) =
                        record_vault_changes(&state.store, &slug, &repo_dir, old, new).await
                    {
                        eprintln!("vault-changes: refresh log failed for {slug}: {e}");
                    }
                }
            }
            Redirect::to(&format!("/vault/{slug}/")).into_response()
        }
        Err(e) => error_page(&format!("could not register the refreshed book: {e}")),
    }
}

/// `POST /vaults/create` — a signed-in user loads a GitHub vault into their
/// PERSONAL space (REQ-VLT-005/008). The quota is checked first so the common
/// "limit reached" answer never clones anything; the slug is derived from the
/// name, and the build runs the llm-wiki converter when needed (AD-14).
async fn load_personal_vault(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Form(p): Form<LoadVaultParams>,
) -> Response {
    if is_locked(&state, &claims.email).await {
        return error_page("demo account can't add or suppress vaults");
    }
    let url = p.url.trim();
    if url.is_empty() {
        return error_page("the GitHub URL is required");
    }
    // The name defaults to the repository directory name; the user may rename it.
    let name = {
        let typed = p.name.trim();
        if typed.is_empty() {
            repo_name_from_url(url)
        } else {
            typed.to_string()
        }
    };
    if name.is_empty() {
        return error_page("the name is required (it defaults to the repository name)");
    }
    // Quota first — no network on refusal (REQ-VLT-008).
    match state
        .store
        .effective_personal_vault_limit(&claims.email)
        .await
    {
        Ok(Some(limit)) => {
            let used = state
                .store
                .count_personal_vaults(&claims.email)
                .await
                .unwrap_or(0);
            if used >= limit {
                return error_page(&format!(
                    "personal vault limit reached: {used}/{limit} — ask an admin for a larger group"
                ));
            }
        }
        Ok(None) => {}
        Err(e) => return error_page(&format!("quota check failed: {e}")),
    }
    // Slug from the name: lowercase, [a-z0-9-] only (safe in /vault/<slug>/).
    let slug: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        return error_page("the name must contain letters or digits");
    }
    // Clone + build (the shared dispatch converts llm-wiki vaults, AD-14).
    use crate::sources::VaultSource as _;
    let data_dir = state.cfg.data_dir.clone();
    let token = (!p.token.trim().is_empty()).then(|| p.token.trim().to_string());
    let src = crate::sources::GitSource::github(
        url.to_string(),
        token,
        data_dir.join("repos").join(&slug),
    );
    let book_dir = match src.sync().and_then(|d| src.build(&d, &name)) {
        Ok(b) => b,
        Err(e) => return error_page(&format!("sync/build failed: {e}")),
    };
    match state
        .store
        .create_personal_vault(&claims.email, &slug, &name, &book_dir)
        .await
    {
        Ok(()) => {
            // Remember the upstream so the vault can be refreshed later.
            let _ = state.store.set_vault_source(&slug, url).await;
            Redirect::to(&format!("/vault/{slug}/")).into_response()
        }
        Err(e) if e.to_string().contains("UNIQUE") => error_page(&format!(
            "the slug '{slug}' is already taken — rename the vault"
        )),
        Err(e) => error_page(&format!("could not register the vault: {e}")),
    }
}

/// `POST /admin/groups/{id}/limit` — set the personal-vault quota (REQ-GRP-005).
/// Empty input = unlimited (NULL). A negative or non-numeric value is refused.
async fn admin_group_limit(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Form(p): Form<GroupLimitParams>,
) -> Response {
    let raw = p.limit.trim();
    let limit = if raw.is_empty() {
        None
    } else {
        match raw.parse::<i64>() {
            Ok(n) if n >= 0 => Some(n),
            _ => {
                let body = r#"<p style="color:red">Quota must be a number (or empty for unlimited).</p>
<p><a href="/admin/groups">Back</a></p>"#;
                return (StatusCode::BAD_REQUEST, Html(body.to_string())).into_response();
            }
        }
    };
    if let Err(e) = state.store.set_group_limit(id, limit).await {
        let body = format!(
            r#"<p style="color:red">Could not set the quota: {}</p>
<p><a href="/admin/groups">Back</a></p>"#,
            html_escape(&e.to_string())
        );
        return (StatusCode::INTERNAL_SERVER_ERROR, Html(body)).into_response();
    }
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

async fn admin_vaults(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
) -> Response {
    let vaults = state.store.list_all_vaults().await.unwrap_or_default();
    use crate::obsidian::convert::{detect, VaultFormat};
    let mut rows = String::new();
    for v in &vaults {
        // PRD-6: a vault matching neither llm-wiki nor mdBook format is flagged.
        let warn = match webdav::source_dir_of(std::path::Path::new(&v.book_path)) {
            Some(src) if matches!(detect(&src), VaultFormat::Neither) => {
                " <em class=\"vault-warn\">⚠ LLM-Wiki not detected: this vault might not work as expected</em>"
            }
            _ => "",
        };
        rows.push_str(&format!(
            r#"<tr><td><a href="/admin/vaults/{s}/perms">{s}</a></td><td>{n}{warn}</td><td><form method="post" action="/admin/vaults/{s}/delete" style="display:inline"><button type="submit">Delete</button></form></td></tr>
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
  <span id="lo-fields"><label>Book dir path <input type="text" name="path" id="lo-path"></label> <button type="button" onclick="openPicker()">Browse…</button></span>
  <span id="gh-fields" style="display:none"><label>Git URL <input type="text" name="url"></label><label>Token <input type="text" name="token"></label></span>
  <button type="submit">Add</button>
</form>
<p><em>Note:</em> a vault is an <strong>llm-wiki / Obsidian</strong> directory (wikilinks + frontmatter) — a local path or a git repo; MoriNet converts and builds it automatically, re-syncing every 5 min. mdBook-native repos keep working. A directory matching neither format is flagged.</p>
<dialog id="dir-picker">
  <div class="picker-bar">
    <button type="button" id="picker-up">\u2191 Up</button>
    <span id="picker-path">/</span>
  </div>
  <ul id="picker-list"></ul>
  <button type="button" onclick="document.getElementById('dir-picker').close()">Cancel</button>
</dialog>
<script>{picker}</script>"#,
        picker = PICKER_JS,
    );
    admin_page(&state, &claims, "Vaults", body).await
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
    let data_dir = state.cfg.data_dir.clone();
    let result = match p.kind.as_str() {
        "local" => {
            let src = LocalSource {
                path: PathBuf::from(&p.path),
            };
            src.sync().and_then(|d| src.build(&d, &p.name))
        }
        "github" => {
            let src = GitSource::github(
                p.url.clone(),
                (!p.token.is_empty()).then(|| p.token.clone()),
                data_dir.join("repos").join(&p.slug),
            );
            src.sync().and_then(|d| src.build(&d, &p.name))
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

async fn admin_perms(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
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
    for (gid, name, _count, _limit) in &groups {
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
    admin_page(&state, &claims, "Vault permissions", body).await
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
            admin_password: "test-pass-9f3a".to_string(),
            smtp: Default::default(), // Phase 5: no SMTP in tests unless asked
            data_dir: std::env::temp_dir().join("morinet-test-data"),
            vault_book_dir: None, // Phase 2: book dirs live in the vaults table
            locked_accounts: Vec::new(), // Phase 12: guard off unless a test opts in
        }
    }

    async fn fixture_store() -> Store {
        let store = Store::connect_memory().await.expect("memory store");
        store
            .ensure_admin("admin@example.com", "test-pass-9f3a")
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
            Some(Body::from(
                "email=admin%40example.com&password=test-pass-9f3a",
            )),
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
            Some(Body::from(
                "email=admin%40example.com&password=test-pass-9f3a",
            )),
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

    // ---------- Phase 10.1: 3-section index (10.1-02 Task 1) ----------

    fn cookie_hdr(cookie: &str) -> HeaderMap {
        HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())])
    }

    async fn body_text(res: Response) -> String {
        String::from_utf8_lossy(&to_bytes(res.into_body(), 65536).await.unwrap()).to_string()
    }

    /// Seed `pal@example.com` (reader) owning a personal vault `mine`, plus a
    /// direct share of the fixture `demo` vault at the given role — one row
    /// per index section.
    async fn seed_owner_and_shared_share(state: &SharedState, share_role: &str) {
        state
            .store
            .create_user("pal@example.com", "pw", "reader")
            .await
            .unwrap();
        let book = std::env::temp_dir().join(format!(
            "morinet-vault-{}-mine-{}",
            std::process::id(),
            share_role
        ));
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(
            book.join("index.html"),
            "<html><body>mine page</body></html>",
        )
        .unwrap();
        state
            .store
            .create_personal_vault("pal@example.com", "mine", "My Book", &book)
            .await
            .unwrap();
        state
            .store
            .share_vault("demo", "pal@example.com", share_role)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn index_has_three_sections() {
        let (app, _) = test_router().await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert_eq!(
            body.matches("<section class=\"panel\"").count(),
            3,
            "exactly three panels: {body}"
        );
        assert!(
            body.contains("<h2>My vaults</h2>"),
            "my-vaults heading: {body}"
        );
        assert!(
            body.contains("<h2>Shared with me</h2>"),
            "shared heading: {body}"
        );
        assert!(
            body.contains("<h2>Load a vault</h2>"),
            "load heading: {body}"
        );
    }

    #[tokio::test]
    async fn index_row_parity_between_sections() {
        // Both lists render through the SAME vault_row helper: identical class
        // structure and the Statistics action on every row.
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("/vault/mine/"), "own row: {body}");
        assert!(body.contains("/vault/demo/"), "shared row: {body}");
        // Structural parity: two rows, same markup skeleton, both with the
        // Statistics action (the one action every row carries).
        assert_eq!(body.matches("class=\"vault-name\"").count(), 2, "{body}");
        assert_eq!(body.matches("class=\"vault-actions\"").count(), 2, "{body}");
        assert_eq!(body.matches(">Statistics</a>").count(), 2, "{body}");
    }

    #[tokio::test]
    async fn index_shared_writer_row_has_refresh() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "writer").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(
            body.contains("action=\"/vaults/demo/refresh\""),
            "writer share row has Refresh: {body}"
        );
    }

    #[tokio::test]
    async fn index_shared_reader_row_has_no_refresh_no_edit() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(
            !body.contains("action=\"/vaults/demo/refresh\""),
            "reader share row has no Refresh: {body}"
        );
        assert!(
            !body.contains("/vault/demo/edit"),
            "reader share row has no Edit: {body}"
        );
        // The sharee's OWN row still carries the owner actions.
        assert!(
            body.contains("/vault/mine/edit"),
            "own row has Edit: {body}"
        );
    }

    // ---------- Phase 10.1: Edit page (10.1-02 Task 2) ----------

    /// Personal vault `mine` owned by `pal@example.com` (no share).
    async fn seed_edit_fixture(state: &SharedState) {
        state
            .store
            .create_user("pal@example.com", "pw", "reader")
            .await
            .unwrap();
        let book = std::env::temp_dir().join(format!(
            "morinet-vault-{}-edit-{}",
            std::process::id(),
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(
            book.join("index.html"),
            "<html><body>mine page</body></html>",
        )
        .unwrap();
        state
            .store
            .create_personal_vault("pal@example.com", "mine", "My Book", &book)
            .await
            .unwrap();
    }

    fn form_headers(cookie: &str) -> HeaderMap {
        let mut h = login_headers();
        h.insert(COOKIE, cookie.parse().unwrap());
        h
    }

    #[tokio::test]
    async fn edit_page_owner_shows_form() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/vault/mine/edit", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("name=\"name\""), "name field: {body}");
        assert!(body.contains("name=\"url\""), "url field: {body}");
        assert!(
            body.contains("name=\"allow_reshare\""),
            "allow_reshare checkbox: {body}"
        );
        assert!(body.contains("My Book"), "name prefilled: {body}");
    }

    #[tokio::test]
    async fn edit_page_sharee_forbidden() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/vault/demo/edit", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn edit_submit_renames_resources_and_sets_reshare() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app.clone(),
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from(
                "name=New+Name&url=https%3A%2F%2Fgithub.com%2Fo%2Fr&allow_reshare=on",
            )),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let vaults = state
            .store
            .accessible_vaults("pal@example.com")
            .await
            .unwrap();
        assert_eq!(vaults[0].vault.name, "New Name", "renamed in store");
        assert_eq!(
            state.store.vault_source("mine").await.unwrap().as_deref(),
            Some("https://github.com/o/r"),
            "source URL persisted"
        );
        assert!(state.store.allow_reshare("mine").await.unwrap());
        // The index row shows the new name.
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(body.contains("New Name"), "index shows new name: {body}");
    }

    #[tokio::test]
    async fn edit_submit_without_checkbox_clears_reshare() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        state.store.set_allow_reshare("mine", true).await.unwrap();
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app,
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from("name=My+Book&url=")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            !state.store.allow_reshare("mine").await.unwrap(),
            "absent checkbox clears the toggle"
        );
    }

    // ---------- Phase 12 / PLAN 12-02 Task 2: display options on the Edit page ----------

    #[tokio::test]
    async fn edit_page_shows_display_options() {
        // A vault loaded through the llm-wiki pipeline (converted): the Edit
        // page shows the page-numbers checkbox AND the theme select (D-11).
        let (app, state) = test_router().await;
        state
            .store
            .create_user("disp@example.com", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(2)).await.unwrap();
        state
            .store
            .add_group_member(g, "disp@example.com")
            .await
            .unwrap();
        let url = wiki_repo("dispwiki-edit");
        let cookie = login_as(&app, "disp@example.com", "pw").await;
        let res = req(
            app.clone(),
            "POST",
            "/vaults/create",
            form_headers(&cookie),
            Some(Body::from(format!("name=DispWiki&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(
            app,
            "GET",
            "/vault/dispwiki/edit",
            cookie_hdr(&cookie),
            None,
        )
        .await;
        let body = body_text(res).await;
        assert!(
            body.contains("name=\"keep_page_numbers\""),
            "page-numbers checkbox missing: {body}"
        );
        assert!(body.contains("name=\"theme\""), "theme select missing");
        for opt in ["light", "navy", "rust", "coal", "ayu"] {
            assert!(
                body.contains(&format!("option value=\"{opt}\"")),
                "theme option {opt} missing"
            );
        }
        assert!(
            body.contains("<option value=\"\""),
            "auto option (value='') missing"
        );
    }

    #[tokio::test]
    async fn edit_page_native_vault_hides_theme_select() {
        // D-11: a native (non-converted) personal vault gets the checkbox but
        // no theme select — its owner edits book.toml directly.
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/vault/mine/edit", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(
            body.contains("name=\"keep_page_numbers\""),
            "page-numbers checkbox missing: {body}"
        );
        assert!(
            !body.contains("name=\"theme\""),
            "theme select must not render for a native vault"
        );
    }

    #[tokio::test]
    async fn edit_submit_persists_display_options() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app,
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from(
                "name=My+Book&url=&keep_page_numbers=on&theme=coal",
            )),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(state.store.keep_page_numbers("mine").await.unwrap());
        assert_eq!(state.store.theme("mine").await.unwrap(), "coal");
    }

    #[tokio::test]
    async fn edit_submit_without_checkbox_clears_keep_page_numbers() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        state
            .store
            .set_keep_page_numbers("mine", true)
            .await
            .unwrap();
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app,
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from("name=My+Book&url=")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            !state.store.keep_page_numbers("mine").await.unwrap(),
            "absent checkbox clears the toggle"
        );
    }

    #[tokio::test]
    async fn edit_submit_rejects_bad_theme() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        state.store.set_theme("mine", "coal").await.unwrap();
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app,
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from("name=My+Book&url=&theme=hacker")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // REJECT, not coerce: the stored theme is untouched.
        assert_eq!(state.store.theme("mine").await.unwrap(), "coal");
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
    async fn logout_clears_the_cookie_and_redirects_to_login() {
        // REQ-ACC-003: POST /logout expires the session cookie and returns to /login.
        let (app, _) = test_router().await;
        let cookie = login_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "POST", "/logout", headers, None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
        let set_cookie = res.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
        assert!(
            set_cookie.contains("Max-Age=0"),
            "cookie must be expired: {set_cookie}"
        );
    }

    // ---------- personal vault loading (REQ-VLT-005/008) ----------

    /// Log in with arbitrary credentials and return the session cookie.
    async fn login_as(app: &Router, email: &str, password: &str) -> String {
        let res = req(
            app.clone(),
            "POST",
            "/login",
            login_headers(),
            Some(Body::from(format!(
                "email={}&password={}",
                email.replace('@', "%40"),
                password
            ))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        res.headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    /// A tiny llm-wiki git repo the loader can clone over `file://` (no network).
    fn wiki_repo(test_name: &str) -> String {
        let root = std::env::temp_dir().join(format!("morinet-wikirepo-{test_name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("entities")).unwrap();
        std::fs::write(root.join("index.md"), "# Home\n\nSee [[king-crimson]].\n").unwrap();
        std::fs::write(
            root.join("entities/king-crimson.md"),
            "---\ntitle: King Crimson\ntype: entity\n---\n\nKC.\n",
        )
        .unwrap();
        for cmd in [
            vec!["git", "init", "-q"],
            vec!["git", "add", "."],
            vec![
                "git",
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "init",
            ],
        ] {
            let out = std::process::Command::new(cmd[0])
                .args(&cmd[1..])
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{cmd:?} failed: {out:?}");
        }
        format!("file://{}", root.display())
    }

    /// Add a commit to a fixture repo created by `wiki_repo`.
    fn wiki_repo_commit(test_name: &str, file: &str, content: &str) {
        let root = std::env::temp_dir().join(format!("morinet-wikirepo-{test_name}"));
        std::fs::write(root.join(file), content).unwrap();
        for cmd in [
            vec!["git", "add", "."],
            vec![
                "git",
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "update",
            ],
        ] {
            let out = std::process::Command::new(cmd[0])
                .args(&cmd[1..])
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{cmd:?} failed: {out:?}");
        }
    }

    #[tokio::test]
    async fn owner_can_delete_their_personal_vault_and_reuse_the_quota() {
        // REQ-VLT-005 follow-up: without a delete, the 2-vault quota could be
        // exhausted for life. Delete is owner-only and frees the slot.
        let (app, state) = test_router().await;
        state
            .store
            .create_user("demo@x.fr", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(1)).await.unwrap();
        state.store.add_group_member(g, "demo@x.fr").await.unwrap();
        let url = wiki_repo("pv-delete");
        let cookie = login_as(&app, "demo@x.fr", "pw").await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/vaults/create",
            headers.clone(),
            Some(Body::from(format!("name=Del One&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        // Quota (1) is now full.
        let res = req(
            app.clone(),
            "POST",
            "/vaults/create",
            headers.clone(),
            Some(Body::from(format!("name=Del Two&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // Delete the vault → slot free again, index stops listing it.
        let res = req(
            app.clone(),
            "POST",
            "/vaults/del-one/delete",
            headers.clone(),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state
                .store
                .count_personal_vaults("demo@x.fr")
                .await
                .unwrap(),
            0
        );
        let res = req(app.clone(), "GET", "/", headers.clone(), None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("Del One (private)"));
        // The freed slot works again.
        let res = req(
            app,
            "POST",
            "/vaults/create",
            headers,
            Some(Body::from(format!("name=Del Two&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn non_owner_cannot_delete_a_personal_vault() {
        let (app, state) = test_router().await;
        state
            .store
            .create_user("a@x.fr", "pw", "reader")
            .await
            .unwrap();
        state
            .store
            .create_user("b@x.fr", "pw", "reader")
            .await
            .unwrap();
        state
            .store
            .create_personal_vault(
                "a@x.fr",
                "mine",
                "Mine",
                std::path::Path::new("/tmp/mine/book"),
            )
            .await
            .unwrap();
        let cookie = login_as(&app, "b@x.fr", "pw").await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(app, "POST", "/vaults/mine/delete", headers, None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            state.store.count_personal_vaults("a@x.fr").await.unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn refresh_pulls_the_latest_commit() {
        // The loaded vault remembers its source; refresh re-pulls and rebuilds,
        // so new upstream pages appear without reloading.
        let (app, state) = test_router().await;
        state
            .store
            .create_user("demo@x.fr", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(2)).await.unwrap();
        state.store.add_group_member(g, "demo@x.fr").await.unwrap();
        let url = wiki_repo("pv-refresh");
        let cookie = login_as(&app, "demo@x.fr", "pw").await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/vaults/create",
            headers.clone(),
            Some(Body::from(format!("name=Refresh One&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        // A new page lands upstream after the load.
        wiki_repo_commit("pv-refresh", "new-page.md", "# New Page\n\nFresh.\n");
        let res = req(
            app.clone(),
            "POST",
            "/vaults/refresh-one/refresh",
            headers.clone(),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            res.headers().get("location").unwrap(),
            "/vault/refresh-one/"
        );
        let res = req(
            app,
            "GET",
            "/vault/refresh-one/new-page.html",
            headers,
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("Fresh"));
    }

    // ---------- Phase 13 / PLAN 13-01 Task 2: vault_changes capture ----------

    /// Stage everything (including deletions) and commit; return the new HEAD oid.
    fn commit_all(repo: &git2::Repository, msg: &str) -> git2::Oid {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("test", "test@test").unwrap();
        let parents: Vec<git2::Commit> = match repo.head() {
            Ok(h) => vec![h.peel_to_commit().unwrap()],
            Err(_) => Vec::new(),
        };
        let refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &refs)
            .unwrap()
    }

    #[test]
    fn diff_commit_trees_maps_kinds() {
        // D-01/D-08: added/modified/removed map exactly, repo-relative paths.
        let dir = std::env::temp_dir().join("morinet-difftree-maps-kinds");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        // Commit 1: a.md + b.md.
        std::fs::write(dir.join("a.md"), "one\n").unwrap();
        std::fs::write(dir.join("b.md"), "b\n").unwrap();
        let oid1 = commit_all(&repo, "c1");
        // Commit 2: adds c.md, modifies a.md, deletes b.md.
        std::fs::write(dir.join("a.md"), "two\n").unwrap();
        std::fs::write(dir.join("c.md"), "c\n").unwrap();
        // Review LOW-01: a backslash is a legal Linux filename character —
        // git paths are slash-separated, so it must survive untouched.
        std::fs::write(dir.join("back\\slash.md"), "x\n").unwrap();
        std::fs::remove_file(dir.join("b.md")).unwrap();
        let oid2 = commit_all(&repo, "c2");

        let mut changes = diff_commit_trees(&dir, oid1, oid2).unwrap();
        changes.sort();
        assert_eq!(
            changes,
            vec![
                ("added", "back\\slash.md".to_string()),
                ("added", "c.md".to_string()),
                ("modified", "a.md".to_string()),
                ("removed", "b.md".to_string()),
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Login + load a git-sourced vault; returns the cookie+form headers.
    async fn load_git_vault(app: &Router, email: &str, name: &str, url: &str) -> HeaderMap {
        let cookie = login_as(app, email, "pw").await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/vaults/create",
            headers.clone(),
            Some(Body::from(format!("name={name}&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        headers
    }

    #[tokio::test]
    async fn refresh_records_vault_changes() {
        // A refresh whose HEAD moved writes one grouped vault_changes event;
        // the response itself is unchanged (D-01, D-04).
        let (app, state) = test_router().await;
        state
            .store
            .create_user("chg@x.fr", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(2)).await.unwrap();
        state.store.add_group_member(g, "chg@x.fr").await.unwrap();
        let url = wiki_repo("pv-refresh-chg");
        let headers = load_git_vault(&app, "chg@x.fr", "Refresh Chg", &url).await;
        // Upstream moves: one new page + one modified page.
        wiki_repo_commit("pv-refresh-chg", "new-page.md", "# New Page\n\nFresh.\n");
        wiki_repo_commit("pv-refresh-chg", "index.md", "# Home\n\nChanged.\n");
        let res = req(
            app.clone(),
            "POST",
            "/vaults/refresh-chg/refresh",
            headers,
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let rows = state
            .store
            .recent_vault_changes("refresh-chg", 5, 30)
            .await
            .unwrap();
        let mut kinds: Vec<(String, String)> =
            rows.iter().map(|r| (r.2.clone(), r.3.clone())).collect();
        kinds.sort();
        assert!(
            kinds.contains(&("added".to_string(), "new-page.md".to_string())),
            "expected added new-page.md, got {kinds:?}"
        );
        assert!(
            kinds.contains(&("modified".to_string(), "index.md".to_string())),
            "expected modified index.md, got {kinds:?}"
        );
        // All rows of the refresh share ONE event_id (D-03).
        let event_ids: std::collections::HashSet<&str> =
            rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(event_ids.len(), 1, "one refresh = one event_id");
    }

    #[tokio::test]
    async fn refresh_noop_records_nothing() {
        // HEAD did not move → zero vault_changes rows (D-01).
        let (app, state) = test_router().await;
        state
            .store
            .create_user("noop@x.fr", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(2)).await.unwrap();
        state.store.add_group_member(g, "noop@x.fr").await.unwrap();
        let url = wiki_repo("pv-refresh-noop");
        let headers = load_git_vault(&app, "noop@x.fr", "Refresh Noop", &url).await;
        let res = req(
            app.clone(),
            "POST",
            "/vaults/refresh-noop/refresh",
            headers,
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let rows = state
            .store
            .recent_vault_changes("refresh-noop", 5, 30)
            .await
            .unwrap();
        assert!(rows.is_empty(), "no-op refresh must log nothing");
    }

    #[tokio::test]
    async fn webdav_put_records_change() {
        // D-02: one event per PUT for the single written path — added on first
        // write, modified on rewrite; the 201 response never depends on it.
        let (app, state) = test_router().await;
        let src = seed_wiki_vault_named(&state, "wikidav-chg").await;
        let res = dav_request(
            app.clone(),
            "PUT",
            "/webdav/wikidav-chg/concepts/new.md",
            basic("admin@example.com", "test-pass-9f3a"),
            Some("---\ntitle: New Page\ntype: concept\n---\n\nFresh.\n"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        // The record is spawned fire-and-forget: poll briefly.
        let mut rows = Vec::new();
        for _ in 0..50 {
            rows = state
                .store
                .recent_vault_changes("wikidav-chg", 5, 30)
                .await
                .unwrap();
            if !rows.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(rows.len(), 1, "one event per PUT");
        assert_eq!(
            (rows[0].2.as_str(), rows[0].3.as_str()),
            ("added", "concepts/new.md")
        );
        let first_event = rows[0].0.clone();
        // Same path again → modified, its own event_id.
        let res = dav_request(
            app,
            "PUT",
            "/webdav/wikidav-chg/concepts/new.md",
            basic("admin@example.com", "test-pass-9f3a"),
            Some("---\ntitle: New Page\ntype: concept\n---\n\nTwice.\n"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let mut rows2 = Vec::new();
        for _ in 0..50 {
            rows2 = state
                .store
                .recent_vault_changes("wikidav-chg", 5, 30)
                .await
                .unwrap();
            if rows2.len() >= 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(rows2.len(), 2);
        assert_eq!(
            (rows2[0].2.as_str(), rows2[0].3.as_str()),
            ("modified", "concepts/new.md"),
            "newest row first"
        );
        assert_ne!(rows2[0].0, first_event, "each PUT gets its own event_id");
        std::fs::remove_dir_all(&src).ok();
    }

    #[tokio::test]
    async fn the_upstream_url_is_recovered_from_the_git_remote() {
        // A vault loaded BEFORE the source URL was recorded has an empty
        // source_url; the refresh recovers it from the clone's git remote.
        let url = wiki_repo("remote-url");
        let root = std::env::temp_dir().join("morinet-wikirepo-remote-url");
        let clone = std::env::temp_dir().join("morinet-clone-remote-url");
        let _ = std::fs::remove_dir_all(&clone);
        let out = std::process::Command::new("git")
            .args(["clone", "-q", &url, clone.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success(), "clone failed: {out:?}");
        let found = git_remote_url(&clone).expect("remote url");
        assert!(
            found.contains("morinet-wikirepo-remote-url"),
            "remote url points at the repo: {found}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn vault_name_is_derived_from_the_repo_url() {
        // UX (2026-10-21): the "Name" field defaults to the repository
        // directory name (the last path segment of the GitHub URL), and the
        // user can still rename it before loading.
        assert_eq!(
            repo_name_from_url("https://github.com/lejeunecedric/B3Wiki"),
            "B3Wiki"
        );
        assert_eq!(
            repo_name_from_url("https://github.com/lejeunecedric/B3Wiki/"),
            "B3Wiki"
        );
        assert_eq!(
            repo_name_from_url("https://github.com/lejeunecedric/B3Wiki.git"),
            "B3Wiki"
        );
        assert_eq!(
            repo_name_from_url("https://gitlab.com/grp/sub/proj"),
            "proj"
        );
        assert_eq!(repo_name_from_url(""), "");
        assert_eq!(repo_name_from_url("not a url"), "");
    }

    #[tokio::test]
    async fn the_load_form_prefills_the_name_from_the_url() {
        // The index ships a tiny inline script: typing/pasting a URL fills the
        // name field (unless the user already typed one).
        let (app, _) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "GET", "/", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("morinet-autofill-name"),
            "autofill script present"
        );
        assert!(text.contains("input[name=\"url\"]"), "url field present");
    }

    // ---------- Phase 9 nav shell: shared header + account menu ----------

    #[tokio::test]
    async fn the_account_menu_shows_name_email_and_logout() {
        // REQ-NAV-002: the shared header carries the account menu — display
        // name (editable), email and a logout button.
        let (app, state) = test_router().await;
        state
            .store
            .set_display_name("admin@example.com", "Cédric")
            .await
            .unwrap();
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app.clone(), "GET", "/", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("Cédric"), "display name shown: {text}");
        assert!(text.contains("admin@example.com"), "email shown: {text}");
        assert!(
            text.contains("href=\"/account\""),
            "account page link: {text}"
        );
        assert!(text.contains("action=\"/logout\""), "logout button: {text}");
        // The rename form lives on the account page, not in the dropdown.
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app.clone(), "GET", "/account", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("action=\"/account/name\""),
            "rename form on /account: {text}"
        );
    }

    #[tokio::test]
    async fn a_user_renames_themselves_from_the_account_menu() {
        let (app, state) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/account/name",
            headers.clone(),
            Some(Body::from("name=Alice")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            state
                .store
                .display_name_for("admin@example.com")
                .await
                .unwrap()
                .as_deref(),
            Some("Alice")
        );
        let res = req(app, "GET", "/", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("Alice"));
    }

    #[tokio::test]
    async fn the_admin_menu_only_appears_for_admins() {
        // REQ-NAV-004: the admin menu is rendered ONLY for role=admin.
        let (app, state) = test_router().await;
        state
            .store
            .create_user("reader@x.fr", "pw", "reader")
            .await
            .unwrap();
        let cookie = login_as(&app, "reader@x.fr", "pw").await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app.clone(), "GET", "/", headers.clone(), None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            !text.contains("href=\"/admin\""),
            "no admin entry for a reader: {text}"
        );
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "GET", "/", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        // The admin entry is a single link — the section links live in the tab
        // bar on admin pages (no duplicated nav lists).
        assert!(
            text.contains("href=\"/admin\""),
            "admin entry for an admin: {text}"
        );
    }

    // ---------- Phase 7: server-side directory picker (REQ-UX-001) ----------

    #[tokio::test]
    async fn the_vaults_page_carries_the_directory_picker() {
        // REQ-UX-001: adding a local vault no longer requires typing a server
        // path — the picker lists the allowed root and fills the form.
        let (app, _) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app.clone(), "GET", "/admin/vaults", headers.clone(), None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("id=\"dir-picker\""), "picker dialog: {text}");
        assert!(text.contains("openPicker()"), "browse button wired: {text}");
        // And the listing API answers for the admin.
        let res = req(app, "GET", "/admin/browse?path=/", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("\"dirs\""), "json listing: {text}");
        assert!(text.contains("\"current\""), "json current path: {text}");
        assert!(text.contains("\"abs\""), "json absolute path: {text}");
    }

    #[tokio::test]
    async fn loading_a_vault_requires_a_session() {
        let (app, _) = test_router().await;
        let res = req(
            app,
            "POST",
            "/vaults/create",
            login_headers(),
            Some(Body::from("name=X&url=file:///nope")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
    }

    #[tokio::test]
    async fn loading_a_vault_is_refused_when_the_quota_is_reached() {
        // REQ-VLT-008: the quota is checked BEFORE anything is cloned — a
        // "limit reached" answer never touches the network (the URL below is
        // garbage; a clone attempt would say "sync/build failed" instead).
        let (app, state) = test_router().await;
        state
            .store
            .create_user("demo@x.fr", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(0)).await.unwrap();
        state.store.add_group_member(g, "demo@x.fr").await.unwrap();
        let cookie = login_as(&app, "demo@x.fr", "pw").await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app,
            "POST",
            "/vaults/create",
            headers,
            Some(Body::from("name=B3Wiki&url=file:///does-not-exist")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("limit reached"), "quota message: {text}");
        assert!(!text.contains("sync/build"), "must not have cloned: {text}");
    }

    #[tokio::test]
    async fn loading_a_vault_registers_a_personal_vault() {
        // REQ-VLT-005: a signed-in user loads a GitHub-style vault into their
        // personal space; it lands in "My vaults" and counts against the quota.
        let (app, state) = test_router().await;
        state
            .store
            .create_user("demo@x.fr", "pw", "reader")
            .await
            .unwrap();
        let g = state.store.create_group("free").await.unwrap();
        state.store.set_group_limit(g, Some(2)).await.unwrap();
        state.store.add_group_member(g, "demo@x.fr").await.unwrap();
        let url = wiki_repo("load-ok");
        let cookie = login_as(&app, "demo@x.fr", "pw").await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/vaults/create",
            headers.clone(),
            Some(Body::from(format!("name=B3Wiki&url={url}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/vault/b3wiki/");
        assert_eq!(
            state
                .store
                .count_personal_vaults("demo@x.fr")
                .await
                .unwrap(),
            1
        );
        // The loaded vault appears in the user's index.
        let res = req(app, "GET", "/", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("B3Wiki"),
            "index lists the loaded vault: {text}"
        );
    }

    #[tokio::test]
    async fn admin_can_set_a_group_quota_from_the_ui() {
        // REQ-GRP-005: the admin groups page edits the quota — a number pins it,
        // an empty input restores unlimited, garbage is refused (pitfall #14:
        // asserted at the route level, not just the store).
        let (app, _) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        // Create a group, then find its id in the form actions of the page.
        let res = req(
            app.clone(),
            "POST",
            "/admin/groups/create",
            headers.clone(),
            Some(Body::from("name=free")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app.clone(), "GET", "/admin/groups", headers.clone(), None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(
            html.contains("quota: unlimited"),
            "fresh group is unlimited: {html}"
        );
        let gid = html
            .split("/admin/groups/")
            .filter_map(|chunk| chunk.split('/').next().unwrap().parse::<i64>().ok())
            .next()
            .expect("a group id in the page");

        // Pin the quota at 5.
        let res = req(
            app.clone(),
            "POST",
            &format!("/admin/groups/{gid}/limit"),
            headers.clone(),
            Some(Body::from("limit=5")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app.clone(), "GET", "/admin/groups", headers.clone(), None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        assert!(
            String::from_utf8_lossy(&body).contains("quota: 5 personal vault(s)"),
            "quota shown after set"
        );

        // Garbage is refused.
        let res = req(
            app.clone(),
            "POST",
            &format!("/admin/groups/{gid}/limit"),
            headers.clone(),
            Some(Body::from("limit=abc")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // Empty restores unlimited.
        let res = req(
            app.clone(),
            "POST",
            &format!("/admin/groups/{gid}/limit"),
            headers.clone(),
            Some(Body::from("limit=")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app, "GET", "/admin/groups", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("quota: unlimited"));
    }

    #[tokio::test]
    async fn admin_can_switch_a_user_role_from_the_ui() {
        // REQ-GRP-007: POST /admin/users/role flips the role and the plan
        // group (demoted users land in `free`), asserted at the route level.
        let (app, _) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        // Invite a reader, then demote/switch through the route.
        let res = req(
            app.clone(),
            "POST",
            "/admin/users/invite",
            headers.clone(),
            Some(Body::from("email=demo%40example.com&role=reader")),
        )
        .await;
        // No SMTP in tests → the temp-password flow answers 200 with the
        // one-time password page (SMTP would 303 back to the list).
        assert!(res.status().is_success() || res.status() == StatusCode::SEE_OTHER);
        let res = req(
            app.clone(),
            "POST",
            "/admin/users/role",
            headers.clone(),
            Some(Body::from("email=demo%40example.com&role=admin")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app.clone(), "GET", "/admin/users", headers.clone(), None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("demo@example.com"));
        // Unknown role is refused.
        let res = req(
            app,
            "POST",
            "/admin/users/role",
            headers,
            Some(Body::from("email=demo%40example.com&role=superuser")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn admin_about_shows_version_and_repo() {
        // REQ-ADMIN-007: the About page shows the installed version (from
        // crate::version(), so it tracks every release) + the GitHub repo URL.
        let (app, _) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "GET", "/admin/about", headers, None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 8192).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains(&format!("morinet {}", crate::version())),
            "version shown: {text}"
        );
        assert!(
            text.contains("https://github.com/lejeunecedric/MoriNet"),
            "repo shown: {text}"
        );
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

    /// llm-wiki vault source (bare `[[wikilinks]]` + frontmatter, no book.toml).
    /// Stored path is the generated `<src>/.morinet-build/book` layout (D2).
    async fn seed_wiki_vault_named(state: &SharedState, slug: &str) -> std::path::PathBuf {
        let src = std::env::temp_dir().join(format!("morinet-dav-wiki-{slug}"));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("concepts")).unwrap();
        std::fs::write(
            src.join("index.md"),
            "---\ntitle: Home\n---\n\n# Home\n\nSee [[z]].\n",
        )
        .unwrap();
        std::fs::write(
            src.join("concepts/z.md"),
            "---\ntitle: Z\ntype: concept\n---\n\nZ page.\n",
        )
        .unwrap();
        let build = src.join(".morinet-build");
        std::fs::create_dir_all(build.join("book")).unwrap();
        std::fs::write(build.join("book").join("index.html"), "<html>wiki</html>").unwrap();
        state
            .store
            .seed_vault(slug, "Wiki vault", &build.join("book"))
            .await
            .unwrap();
        src
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
            basic("admin@example.com", "test-pass-9f3a"),
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
        for (k, v) in basic("admin@example.com", "test-pass-9f3a").iter() {
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
            basic("admin@example.com", "test-pass-9f3a"),
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
                basic("admin@example.com", "test-pass-9f3a"),
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
            basic("admin@example.com", "test-pass-9f3a"),
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
    async fn webdav_put_on_llm_wiki_vault_reconverts_the_tree() {
        // REQ-OBSFMT-006 at the ROUTE level (pitfall #14): PUT a note on the
        // SOURCE; the generated tree must reflect it — converted page with meta
        // block + SUMMARY regenerated, not just a file on disk.
        let (app, state) = test_router().await;
        let src = seed_wiki_vault_named(&state, "wikidav").await;
        let res = dav_request(
            app,
            "PUT",
            "/webdav/wikidav/concepts/new.md",
            basic("admin@example.com", "test-pass-9f3a"),
            Some("---\ntitle: New Page\ntype: concept\n---\n\nFresh.\n"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let gen_src = src.join(".morinet-build").join("src");
        let page = std::fs::read_to_string(gen_src.join("concepts/new.md"))
            .expect("new note converted into the build tree");
        assert!(page.contains("<div class=\"note-meta\">"), "got: {page}");
        let summary = std::fs::read_to_string(gen_src.join("SUMMARY.md")).unwrap();
        assert!(
            summary.contains("[New Page](concepts/new.md)"),
            "SUMMARY regenerated: {summary}"
        );
        std::fs::remove_dir_all(&src).ok();
    }

    #[tokio::test]
    async fn admin_vaults_warn_when_format_matches_neither() {
        // PRD-6: the exact warning string, visible on the admin vaults page.
        let (app, state) = test_router().await;
        let plain = std::env::temp_dir().join("morinet-plain-vault-badge");
        let _ = std::fs::remove_dir_all(&plain);
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("page.html"), "<html></html>").unwrap();
        state
            .store
            .seed_vault("plain", "Plain vault", &plain)
            .await
            .unwrap();
        let cookie = admin_cookie(&app).await;
        let headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        let res = req(app, "GET", "/admin/vaults", headers, None).await;
        let body = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        // The warning sits on the Plain vault row (a bare static dir: neither
        // llm-wiki nor mdBook → D2/PRD-6 flags it).
        let plain_row = text
            .lines()
            .find(|l| l.contains("Plain vault"))
            .expect("plain vault row present");
        assert!(
            plain_row.contains("LLM-Wiki not detected"),
            "expected warning on the plain row, got: {plain_row}"
        );
        assert!(
            plain_row.contains("this vault might not work as expected"),
            "exact PRD-6 wording, got: {plain_row}"
        );
        // The demo fixture is the SAME kind (bare compiled HTML, no sources) →
        // also honestly flagged; a vault WITH real sources must not be.
        assert!(text.contains("LLM-Wiki not detected"));
        std::fs::remove_dir_all(&plain).ok();
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
                basic("admin@example.com", "test-pass-9f3a"),
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
            basic("admin@example.com", "test-pass-9f3a"),
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

    // ---------- Phase 10 / PLAN 10-02: MCP endpoint on /mcp ----------

    async fn mcp_jsonrpc(app: &Router, bearer: Option<&str>, body: &str) -> (StatusCode, String) {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert(
            axum::http::header::ACCEPT,
            "application/json, text/event-stream".parse().unwrap(),
        );
        // rmcp validates Host (DNS-rebinding protection) — real clients send it.
        headers.insert(axum::http::header::HOST, "localhost".parse().unwrap());
        headers.insert("MCP-Protocol-Version", "2025-06-18".parse().unwrap());
        if let Some(b) = bearer {
            headers.insert(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {b}").parse().unwrap(),
            );
        }
        let res = req(
            app.clone(),
            "POST",
            "/mcp",
            headers,
            Some(Body::from(body.to_string())),
        )
        .await;
        let status = res.status();
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;

    #[tokio::test]
    async fn mcp_endpoint_requires_a_bearer_token() {
        let (app, _state) = test_router().await;
        let (status, _) = mcp_jsonrpc(&app, None, TOOLS_LIST).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no Bearer → 401");
    }

    #[tokio::test]
    async fn mcp_rejects_an_unknown_or_revoked_token() {
        let (app, state) = test_router().await;
        let (status, _) = mcp_jsonrpc(&app, Some("mn_forged"), TOOLS_LIST).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "forged token → 401");

        let uid = state
            .store
            .verify_login("admin@example.com", "test-pass-9f3a")
            .await
            .unwrap()
            .unwrap()
            .id;
        let (id, plain) = state.store.create_api_token(uid, "doomed").await.unwrap();
        let (status, _) = mcp_jsonrpc(&app, Some(&plain), TOOLS_LIST).await;
        assert_eq!(status, StatusCode::OK, "live token works");
        state.store.revoke_api_token(uid, id).await.unwrap();
        let (status, _) = mcp_jsonrpc(&app, Some(&plain), TOOLS_LIST).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "revoked token → 401");
    }

    #[tokio::test]
    async fn mcp_tools_call_is_bound_to_the_bearer_user() {
        // The per-request factory must bind the tool to the token's user:
        // a reader with no grants gets an empty vault list, never the demo.
        let (app, state) = test_router().await;
        state
            .store
            .create_user("plain@mcp.test", "test-pass-9f3a", "reader")
            .await
            .unwrap();
        let uid = state
            .store
            .verify_login("plain@mcp.test", "test-pass-9f3a")
            .await
            .unwrap()
            .unwrap()
            .id;
        let (_, plain) = state.store.create_api_token(uid, "reader").await.unwrap();
        let (status, body) = mcp_jsonrpc(
            &app,
            Some(&plain),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_vaults","arguments":{}}}"#,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body: {body}");
        assert!(
            body.contains("[]"),
            "reader with no grants sees no vaults: {body}"
        );
        assert!(!body.contains("demo"), "no leak of the demo vault: {body}");
    }

    #[tokio::test]
    async fn mcp_tools_list_exposes_the_four_tools() {
        let (app, state) = test_router().await;
        let uid = state
            .store
            .verify_login("admin@example.com", "test-pass-9f3a")
            .await
            .unwrap()
            .unwrap()
            .id;
        let (_, plain) = state.store.create_api_token(uid, "e2e").await.unwrap();
        let (status, body) = mcp_jsonrpc(&app, Some(&plain), TOOLS_LIST).await;
        assert_eq!(status, StatusCode::OK, "body: {body}");
        for tool in ["list_vaults", "list_notes", "read_note", "search_notes"] {
            assert!(body.contains(tool), "tools/list must expose {tool}: {body}");
        }
    }

    // ---------- Phase 10 / PLAN 10-01: API tokens on /account ----------

    #[tokio::test]
    async fn api_token_creation_shows_the_plaintext_once() {
        let (app, state) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/account/tokens",
            headers.clone(),
            Some(Body::from("name=ci")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        let pos = text
            .find("mn_")
            .expect("plaintext token shown once after creation");
        let plain: String = text[pos..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        assert!(state
            .store
            .user_for_api_token(&plain)
            .await
            .unwrap()
            .is_some());

        // The listing shows the NAME but never replays the plaintext.
        let res = req(app.clone(), "GET", "/account", headers, None).await;
        let body = to_bytes(res.into_body(), 65536).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains(">ci<") || text.contains(">ci</td>") || text.contains("ci</td>"),
            "token name listed: {text}"
        );
        assert!(!text.contains(&plain), "plaintext never replayed");
    }

    #[tokio::test]
    async fn api_token_routes_require_a_session() {
        let (app, _state) = test_router().await;
        let res = req(
            app.clone(),
            "POST",
            "/account/tokens",
            HeaderMap::new(),
            Some(Body::from("name=x")),
        )
        .await;
        // Same convention as every protected POST: anonymous → /login.
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers().get("location").unwrap(), "/login");
    }

    #[tokio::test]
    async fn api_token_can_be_revoked_from_the_account_page() {
        let (app, state) = test_router().await;
        let cookie = admin_cookie(&app).await;
        let uid = state
            .store
            .verify_login("admin@example.com", "test-pass-9f3a")
            .await
            .unwrap()
            .expect("admin logs in")
            .id;
        let (id, plain) = state.store.create_api_token(uid, "doomed").await.unwrap();

        let mut headers: HeaderMap = HeaderMap::from_iter([(COOKIE, cookie.parse().unwrap())]);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let res = req(
            app.clone(),
            "POST",
            "/account/tokens/revoke",
            headers,
            Some(Body::from(format!("id={id}"))),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(
            state
                .store
                .user_for_api_token(&plain)
                .await
                .unwrap()
                .is_none(),
            "revoked from the UI stops resolving"
        );
    }

    // ---------- Phase 10.1: Share pages (10.1-02 Task 3) ----------

    #[tokio::test]
    async fn share_page_owner_lists_and_adds() {
        let (app, state) = test_router().await;
        // pal owns `mine` — the owner manages its share list.
        seed_owner_and_shared_share(&state, "reader").await;
        state
            .store
            .create_user("friend@example.com", "pw", "reader")
            .await
            .unwrap();
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app.clone(),
            "POST",
            "/vault/mine/share",
            form_headers(&cookie),
            Some(Body::from("email=friend%40example.com&role=reader")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(
            app.clone(),
            "POST",
            "/vault/mine/share",
            form_headers(&cookie),
            Some(Body::from("email=nobody%40example.com&role=writer")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app, "GET", "/vault/mine/share", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("friend@example.com"), "sharee listed: {body}");
        assert!(
            body.contains("nobody@example.com"),
            "unknown listed: {body}"
        );
        assert!(body.contains("Revoke"), "revoke button: {body}");
        assert!(
            body.contains("name=\"email\"") && body.contains("name=\"role\""),
            "add form fields: {body}"
        );
        // The pending hint shows ONLY for the unregistered address.
        assert_eq!(
            body.matches("pending").count(),
            1,
            "pending hint only for the unknown email: {body}"
        );
    }

    // Issue #5: Share / Statistics / Edit pages rendered through the bare
    // `app_page` shell, which drops the global header (brand + account menu).
    // Every authenticated page must keep the nav.
    #[tokio::test]
    async fn vault_subpages_keep_global_header() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        // owner: share/stats/edit on own vault; sharee: stats on shared vault
        for path in [
            "/vault/mine/share",
            "/vault/mine/stats",
            "/vault/mine/edit",
            "/vault/demo/stats",
        ] {
            let res = req(app.clone(), "GET", path, cookie_hdr(&cookie), None).await;
            assert_eq!(res.status(), StatusCode::OK, "{path}");
            let body = body_text(res).await;
            assert!(body.contains("mori-nav"), "global header on {path}: {body}");
        }
    }

    // Issue #9: every served vault page must carry the vault chrome (zoom +
    // Close vault), including native mdBook vaults that were not built by the
    // converter. Pages that already ship the chrome must not get it twice.
    #[tokio::test]
    async fn served_vault_pages_get_vault_chrome() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app.clone(),
            "GET",
            "/vault/mine/",
            cookie_hdr(&cookie),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("morinet-zoom") && body.contains("</body>"),
            "chrome injected before </body>: {body}"
        );
        // A page that already references the chrome (converted tree) is untouched.
        let accessible = state
            .store
            .accessible_vaults("pal@example.com")
            .await
            .unwrap();
        let book = std::path::PathBuf::from(
            accessible
                .iter()
                .find(|a| a.vault.slug == "mine")
                .unwrap()
                .vault
                .book_path
                .clone(),
        );
        std::fs::write(
            book.join("index.html"),
            "<html><head><link href=\"theme/zoom.css\"></head><body>page</body></html>",
        )
        .unwrap();
        let res = req(app, "GET", "/vault/mine/", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert_eq!(
            body.matches("morinet-zoom").count(),
            0,
            "no double chrome: {body}"
        );
    }

    // Issue #8: the Settings section needs an explicit way back to the app.
    #[tokio::test]
    async fn admin_pages_offer_exit_button() {
        let (app, _) = test_router().await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/admin/users", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("Exit Settings") && body.contains("href=\"/\""),
            "exit button back to main menu: {body}"
        );
    }

    #[tokio::test]
    async fn share_page_sharee_forbidden_when_reshare_off() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/vault/demo/share", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn share_page_sharee_allowed_when_reshare_on() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        state.store.set_allow_reshare("demo", true).await.unwrap();
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app.clone(),
            "GET",
            "/vault/demo/share",
            cookie_hdr(&cookie),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        // The re-sharee can add their own sub-share.
        let res = req(
            app.clone(),
            "POST",
            "/vault/demo/share",
            form_headers(&cookie),
            Some(Body::from("email=sub%40example.com&role=reader")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app, "GET", "/vault/demo/share", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(body.contains("sub@example.com"), "sub-share listed: {body}");
    }

    #[tokio::test]
    async fn share_revoke_removes_row() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        req(
            app.clone(),
            "POST",
            "/vault/mine/share",
            form_headers(&cookie),
            Some(Body::from("email=bob%40example.com&role=reader")),
        )
        .await;
        let res = req(
            app.clone(),
            "POST",
            "/vault/mine/share/revoke",
            form_headers(&cookie),
            Some(Body::from("email=bob%40example.com")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let res = req(app, "GET", "/vault/mine/share", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(
            !body.contains("bob@example.com"),
            "revoked row is gone: {body}"
        );
    }

    #[tokio::test]
    async fn share_hide_removes_from_own_view_only() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app.clone(),
            "POST",
            "/vault/demo/share/hide",
            form_headers(&cookie),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        // Gone from the sharee's index...
        let res = req(app.clone(), "GET", "/", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(
            !body.contains("/vault/demo/"),
            "hidden vault gone from sharee view: {body}"
        );
        // ...but the owner's share list still shows the row.
        let admin = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/demo/share", cookie_hdr(&admin), None).await;
        let body = body_text(res).await;
        assert!(
            body.contains("pal@example.com"),
            "owner still sees the (hidden) share: {body}"
        );
    }

    #[tokio::test]
    async fn share_page_escapes_email() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        req(
            app.clone(),
            "POST",
            "/vault/mine/share",
            form_headers(&cookie),
            Some(Body::from("email=evi%3Cb%3E%40x.com&role=reader")),
        )
        .await;
        let res = req(app, "GET", "/vault/mine/share", cookie_hdr(&cookie), None).await;
        let body = body_text(res).await;
        assert!(body.contains("&lt;b&gt;"), "email is escaped: {body}");
        assert!(!body.contains("evi<b>"), "no raw markup from email: {body}");
    }

    // ---------- Phase 10.1: Stats page (10.1-02 Task 4) ----------

    #[tokio::test]
    async fn stats_owner_sees_full_panel() {
        let (app, state) = test_router().await;
        // admin is the effective owner of the fixture `demo` vault.
        state
            .store
            .log_access("admin@example.com", "demo", "web", "notes.html")
            .await
            .unwrap();
        state
            .store
            .log_access("admin@example.com", "demo", "web", "notes.html")
            .await
            .unwrap();
        state
            .store
            .log_access("admin@example.com", "demo", "mcp", "vault_list")
            .await
            .unwrap();
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("Pages"), "page count label: {body}");
        assert!(body.contains("Last refresh"), "last refresh label: {body}");
        assert!(body.contains("Top pages"), "top pages table: {body}");
        assert!(body.contains("notes.html"), "top page row: {body}");
        assert!(body.contains(">web<"), "per-day web row: {body}");
        assert!(body.contains(">mcp<"), "per-day mcp row: {body}");
    }

    #[tokio::test]
    async fn stats_sharee_sees_only_own_count() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        for _ in 0..2 {
            state
                .store
                .log_access("pal@example.com", "demo", "web", "index.html")
                .await
                .unwrap();
        }
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("You have accessed this vault 2 time(s)"),
            "own count line: {body}"
        );
        // Negative assertion: nothing from the owner panel leaks (T-10.1-07).
        assert!(!body.contains("Top pages"), "no owner panel: {body}");
        assert!(!body.contains("Last refresh"), "no owner panel: {body}");
    }

    #[tokio::test]
    async fn stats_group_member_sees_only_own_count() {
        let (app, state) = test_router().await;
        state
            .store
            .create_user("grp@example.com", "pw", "reader")
            .await
            .unwrap();
        let gid = state.store.create_group("team").await.unwrap();
        state
            .store
            .add_group_member(gid, "grp@example.com")
            .await
            .unwrap();
        state
            .store
            .set_vault_group_role("demo", gid, "read")
            .await
            .unwrap();
        state
            .store
            .log_access("grp@example.com", "demo", "web", "index.html")
            .await
            .unwrap();
        let cookie = login_as(&app, "grp@example.com", "pw").await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("You have accessed this vault 1 time(s)"),
            "own count line: {body}"
        );
        assert!(!body.contains("Top pages"), "no owner panel: {body}");
    }

    #[tokio::test]
    async fn stats_unrelated_user_forbidden() {
        let (app, state) = test_router().await;
        state
            .store
            .create_user("stranger@example.com", "pw", "reader")
            .await
            .unwrap();
        let cookie = login_as(&app, "stranger@example.com", "pw").await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    // ---------- Phase 13: Wiki changes section (13-02 Task 1) ----------

    #[tokio::test]
    async fn stats_shows_wiki_changes_section() {
        let (app, state) = test_router().await;
        // Event A (older): added + removed. Event B (newer): modified.
        state
            .store
            .log_vault_change("demo", "ev-a", "added", "alpha.md")
            .await
            .unwrap();
        state
            .store
            .log_vault_change("demo", "ev-a", "removed", "gone.md")
            .await
            .unwrap();
        state
            .store
            .log_vault_change("demo", "ev-b", "modified", "beta.md")
            .await
            .unwrap();
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("Wiki changes"), "section heading: {body}");
        for p in ["alpha.md", "gone.md", "beta.md"] {
            assert!(body.contains(p), "change path {p}: {body}");
        }
        // D-08: prettified display labels, rows only inside the section table.
        assert!(body.contains(">Added<"), "Added label: {body}");
        assert!(body.contains(">Modified<"), "Modified label: {body}");
        assert!(body.contains(">Removed<"), "Removed label: {body}");
        // Per-event headers show their counts.
        assert!(body.contains("2 change(s)"), "event A count: {body}");
        assert!(body.contains("1 change(s)"), "event B count: {body}");
        // Newest event first: event B's row precedes event A's rows.
        let pos_b = body.find("beta.md").expect("beta.md present");
        let pos_a = body.find("alpha.md").expect("alpha.md present");
        assert!(pos_b < pos_a, "newest event first: {body}");
    }

    #[tokio::test]
    async fn stats_wiki_changes_empty_state() {
        let (app, _state) = test_router().await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("No changes recorded yet."),
            "D-06 empty state: {body}"
        );
    }

    #[tokio::test]
    async fn stats_wiki_changes_escapes_paths() {
        let (app, state) = test_router().await;
        state
            .store
            .log_vault_change("demo", "ev-x", "added", "<script>alert(1)</script>")
            .await
            .unwrap();
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("&lt;script&gt;"), "escaped path: {body}");
        assert!(
            !body.contains("<script>alert"),
            "raw script payload leaked: {body}"
        );
    }

    #[tokio::test]
    async fn stats_sharee_branch_unchanged() {
        let (app, state) = test_router().await;
        seed_owner_and_shared_share(&state, "reader").await;
        // Even with changes recorded, a sharee must not see the section (D-07).
        state
            .store
            .log_vault_change("demo", "ev-a", "added", "secret.md")
            .await
            .unwrap();
        state
            .store
            .log_access("pal@example.com", "demo", "web", "index.html")
            .await
            .unwrap();
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(app, "GET", "/vault/demo/stats", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("You have accessed this vault 1 time(s)"),
            "own count line: {body}"
        );
        assert!(
            !body.contains("Wiki changes"),
            "no section for sharee: {body}"
        );
        assert!(!body.contains("secret.md"), "no change leak: {body}");
    }

    #[tokio::test]
    async fn web_access_is_logged_best_effort() {
        let (app, state) = test_router().await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/demo/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        // The fire-and-forget spawn lands shortly after the response.
        let mut logged = false;
        for _ in 0..40 {
            let top = state.store.stats_top_pages("demo").await.unwrap();
            if top.iter().any(|(p, _)| p == "index.html") {
                logged = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(logged, "web access row appears in access_log");
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn dir_size_bytes_sums_recursively() {
        let dir = std::env::temp_dir().join(format!(
            "morinet-dirsize-{}-{}",
            std::process::id(),
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "abc").unwrap();
        std::fs::write(dir.join("sub/b.txt"), "defgh").unwrap();
        assert_eq!(dir_size_bytes(&dir), 8);
        assert_eq!(dir_size_bytes(&dir.join("missing")), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---------- Phase 10.1: WebDAV share ACL at route level (Task 4) ----------

    /// `<src>/book` mdbook layout under a per-test slug (pitfall #23: each
    /// test owns its slug AND its directory, parallel tests never collide).
    async fn seed_dav_share_fixture(state: &SharedState, slug: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("morinet-dav-vault-{}-{slug}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("book.toml"), "[book]\ntitle = \"T\"\n").unwrap();
        std::fs::write(root.join("src").join("SUMMARY.md"), "# Summary\n").unwrap();
        std::fs::write(root.join("src").join("note.md"), "# Note\n\nhello\n").unwrap();
        std::fs::create_dir_all(root.join("book")).unwrap();
        std::fs::write(root.join("book").join("index.html"), "<html>built</html>").unwrap();
        state
            .store
            .seed_vault(slug, "DAV share vault", &root.join("book"))
            .await
            .unwrap();
        root
    }

    #[tokio::test]
    async fn webdav_writer_sharee_can_put_reader_cannot() {
        let (app, state) = test_router().await;
        let root = seed_dav_share_fixture(&state, "davshare").await;
        // Writer sharee: PUT succeeds.
        state
            .store
            .create_user("w@example.com", "Write!pw123", "reader")
            .await
            .unwrap();
        state
            .store
            .share_vault("davshare", "w@example.com", "writer")
            .await
            .unwrap();
        let res = dav_request(
            app.clone(),
            "PUT",
            "/webdav/davshare/src/w.md",
            basic("w@example.com", "Write!pw123"),
            Some("# writer sharee\n"),
        )
        .await;
        assert!(
            res.status().is_success(),
            "writer sharee PUT ok, got {}",
            res.status()
        );
        assert!(root.join("src/w.md").exists(), "file written");
        // Reader sharee: 403.
        state
            .store
            .create_user("r@example.com", "Read!pw123", "reader")
            .await
            .unwrap();
        state
            .store
            .share_vault("davshare", "r@example.com", "reader")
            .await
            .unwrap();
        let res = dav_request(
            app.clone(),
            "PUT",
            "/webdav/davshare/src/r.md",
            basic("r@example.com", "Read!pw123"),
            Some("# reader\n"),
        )
        .await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "readers never write");
        assert!(!root.join("src/r.md").exists(), "no file written");
        // Hidden sharee: no access at all (404 via dav_resolve, or 401).
        state
            .store
            .create_user("h@example.com", "Hide!pw123", "reader")
            .await
            .unwrap();
        state
            .store
            .share_vault("davshare", "h@example.com", "writer")
            .await
            .unwrap();
        state
            .store
            .hide_share("davshare", "h@example.com")
            .await
            .unwrap();
        // Hidden sharee: no access at all. The existing `access_for` gate
        // answers 403 before dav_resolve's 404 can fire (same convention as
        // `reader_without_acl_is_denied_on_webdav`); any of 401/403/404 proves
        // the write was refused.
        let res = dav_request(
            app,
            "PUT",
            "/webdav/davshare/src/h.md",
            basic("h@example.com", "Hide!pw123"),
            Some("# hidden\n"),
        )
        .await;
        assert!(
            res.status() == StatusCode::NOT_FOUND
                || res.status() == StatusCode::UNAUTHORIZED
                || res.status() == StatusCode::FORBIDDEN,
            "hidden sharee has no access, got {}",
            res.status()
        );
        assert!(!root.join("src/h.md").exists(), "no file written");
        std::fs::remove_dir_all(&root).ok();
    }

    // ---------- Phase 12 (12-01): locked-account demo guard (REQ-DEMO-001/002) ----------

    /// The EXACT refusal/popup string from REQUIREMENTS (D-03/D-04).
    const LOCK_MSG: &str = "demo account can't add or suppress vaults";

    /// Router whose config locks `demo@example.com` (D-01/D-02 fixture).
    async fn locked_router() -> (Router, SharedState) {
        let mut cfg = test_config();
        cfg.locked_accounts = vec!["demo@example.com".to_string()];
        let (app, state, _sender) = test_router_with_email(cfg).await;
        (app, state)
    }

    /// Locked user `demo@example.com` (creds `test-pass-9f3a`) owning personal
    /// vault `slug`. Each test passes its own slug (pitfalls #16/#23).
    async fn seed_locked_fixture(state: &SharedState, slug: &str) {
        state
            .store
            .create_user("demo@example.com", "test-pass-9f3a", "reader")
            .await
            .unwrap();
        let book = std::env::temp_dir().join(format!(
            "morinet-vault-{}-locked-{}-{}",
            std::process::id(),
            slug,
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(
            book.join("index.html"),
            "<html><body>locked page</body></html>",
        )
        .unwrap();
        state
            .store
            .create_personal_vault("demo@example.com", slug, "Locked Book", &book)
            .await
            .unwrap();
    }

    /// T-12-01: a scripted POST (no JS) to /vaults/create is refused with the
    /// error page carrying the exact string — proves the server gate.
    #[tokio::test]
    async fn locked_account_create_refused() {
        let (app, state) = locked_router().await;
        state
            .store
            .create_user("demo@example.com", "test-pass-9f3a", "reader")
            .await
            .unwrap();
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(
            app,
            "POST",
            "/vaults/create",
            form_headers(&cookie),
            Some(Body::from(
                "name=lockedx&url=https%3A%2F%2Fgithub.com%2Fo%2Fr",
            )),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_text(res).await;
        assert!(
            body.contains(LOCK_MSG),
            "error page carries the exact refusal string: {body}"
        );
        let vaults = state
            .store
            .accessible_vaults("demo@example.com")
            .await
            .unwrap();
        assert!(vaults.is_empty(), "refused load created nothing");
    }

    #[tokio::test]
    async fn locked_account_delete_refused() {
        let (app, state) = locked_router().await;
        seed_locked_fixture(&state, "lockdel").await;
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(
            app,
            "POST",
            "/vaults/lockdel/delete",
            form_headers(&cookie),
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_text(res).await;
        assert!(body.contains(LOCK_MSG), "refusal string: {body}");
        let vaults = state
            .store
            .accessible_vaults("demo@example.com")
            .await
            .unwrap();
        assert!(
            vaults.iter().any(|a| a.vault.slug == "lockdel"),
            "vault survives the refused delete"
        );
    }

    #[tokio::test]
    async fn locked_account_edit_submit_refused() {
        let (app, state) = locked_router().await;
        seed_locked_fixture(&state, "lockedit").await;
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(
            app,
            "POST",
            "/vault/lockedit/edit",
            form_headers(&cookie),
            Some(Body::from("name=Renamed&url=")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_text(res).await;
        assert!(body.contains(LOCK_MSG), "refusal string: {body}");
        let vaults = state
            .store
            .accessible_vaults("demo@example.com")
            .await
            .unwrap();
        assert_eq!(vaults[0].vault.name, "Locked Book", "name unchanged");
    }

    #[tokio::test]
    async fn locked_account_share_add_refused() {
        let (app, state) = locked_router().await;
        seed_locked_fixture(&state, "lockshare").await;
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(
            app,
            "POST",
            "/vault/lockshare/share",
            form_headers(&cookie),
            Some(Body::from("email=friend%40example.com&role=reader")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_text(res).await;
        assert!(body.contains(LOCK_MSG), "refusal string: {body}");
        let shares = state.store.list_shares("lockshare").await.unwrap();
        assert!(shares.is_empty(), "no share row created");
    }

    #[tokio::test]
    async fn locked_account_share_revoke_refused() {
        let (app, state) = locked_router().await;
        seed_locked_fixture(&state, "lockrevoke").await;
        state
            .store
            .share_vault("lockrevoke", "friend@example.com", "reader")
            .await
            .unwrap();
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(
            app,
            "POST",
            "/vault/lockrevoke/share/revoke",
            form_headers(&cookie),
            Some(Body::from("email=friend%40example.com")),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_text(res).await;
        assert!(body.contains(LOCK_MSG), "refusal string: {body}");
        let shares = state.store.list_shares("lockrevoke").await.unwrap();
        assert_eq!(shares.len(), 1, "share survives the refused revoke");
    }

    /// D-02: Refresh is NOT part of the locked scope. The seeded vault has no
    /// upstream, so the handler answers its own "no upstream" error page —
    /// reaching that code at all proves the lock gate let the request through.
    #[tokio::test]
    async fn locked_account_refresh_still_allowed() {
        let (app, state) = locked_router().await;
        seed_locked_fixture(&state, "lockrefresh").await;
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(
            app,
            "POST",
            "/vaults/lockrefresh/refresh",
            form_headers(&cookie),
            None,
        )
        .await;
        let body = body_text(res).await;
        assert!(
            !body.contains(LOCK_MSG),
            "refresh must not hit the lock gate: {body}"
        );
    }

    /// D-02: admins are never locked, even if their email is in the list.
    #[tokio::test]
    async fn locked_admin_email_not_locked() {
        let mut cfg = test_config();
        cfg.locked_accounts = vec!["admin@example.com".to_string()];
        let (app, state, _sender) = test_router_with_email(cfg).await;
        seed_edit_fixture(&state).await;
        let cookie = login_cookie(&app).await;
        let res = req(
            app,
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from("name=Renamed+By+Admin&url=")),
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::SEE_OTHER,
            "admin mutation succeeds despite the lock list"
        );
    }

    /// D-04/D-05: the locked user's home page carries the popup script with
    /// the exact string and the two form selectors.
    #[tokio::test]
    async fn home_page_shows_locked_popup_script() {
        let (app, state) = locked_router().await;
        seed_locked_fixture(&state, "lockhome").await;
        let cookie = login_as(&app, "demo@example.com", "test-pass-9f3a").await;
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains(LOCK_MSG),
            "locked home page carries the alert string: {body}"
        );
        assert!(
            body.contains(r#"form[action="/vaults/create"]"#),
            "script targets the Load-vault form"
        );
        assert!(
            body.contains(r#"form[action$="/delete"]"#),
            "script targets the Delete forms"
        );
    }

    /// D-05: non-locked users see no new script — the page is unregressed.
    #[tokio::test]
    async fn home_page_clean_for_unlocked_user() {
        let (app, _state) = test_router().await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            !body.contains(LOCK_MSG),
            "unlocked home page must not contain the alert string"
        );
    }

    // ---- Plan 12-03: serve-time display options (D-08..D-10) ----

    /// A realistic built-mdBook page: inline theme script carrying the two
    /// exact literals (D-10) and toc.js chapter-item markup (D-09), modeled
    /// on `data/repos/b3wiki/.morinet-build/book/index.html`.
    const MDBOOK_PAGE: &str = r#"<!DOCTYPE html>
<html class="light no-js" dir="ltr" lang="en">
<head><script>
(function testFunction() {
    var default_light_theme = "light";
    var default_dark_theme = "navy";
    var preferredTheme;
})();
</script></head>
<body>
<nav role="sidebar"><ul class="chapter"><li class="chapter-item expanded"><a href="intro.html"><strong aria-hidden="true">1.</strong>Intro</a></li></ul></nav>
</body></html>
"#;

    #[test]
    fn display_options_theme_rewrite_applied() {
        let mut html = MDBOOK_PAGE.to_string();
        apply_vault_display_options(&mut html, false, "coal");
        assert!(
            html.contains(r#"default_light_theme = "coal";"#)
                && html.contains(r#"default_dark_theme = "coal";"#),
            "both literals rewritten to coal: {html}"
        );
        assert!(
            !html.contains(r#"default_light_theme = "light";"#)
                && !html.contains(r#"default_dark_theme = "navy";"#),
            "original literals gone: {html}"
        );
        // Nothing else touched: the chapter markup is intact.
        assert!(html.contains(r#"<strong aria-hidden="true">1.</strong>Intro"#));
        assert!(!html.contains(PAGE_NUMBERS_OVERRIDE));
    }

    #[test]
    fn display_options_theme_rewrite_inert_when_absent() {
        let page = "<html><body>plain page, no mdBook script</body></html>";
        let mut html = page.to_string();
        apply_vault_display_options(&mut html, false, "rust");
        assert_eq!(html, page, "page without literals is byte-identical");
    }

    /// Review fix (LOW, defense-in-depth): even if a bad value ever reached the
    /// column (direct DB write, future bug), the serve-time rewrite must only
    /// ever substitute a whitelist token — never attacker bytes.
    #[test]
    fn display_options_non_whitelist_theme_is_ignored() {
        let mut html = MDBOOK_PAGE.to_string();
        apply_vault_display_options(&mut html, false, r#"coal"; alert(1); //"#);
        assert!(
            html.contains(r#"default_light_theme = "light";"#)
                && html.contains(r#"default_dark_theme = "navy";"#),
            "non-whitelist theme must leave both literals untouched: {html}"
        );
        assert!(!html.contains("alert(1)"), "no attacker bytes injected");
    }

    #[test]
    fn display_options_page_numbers_style_injected_before_body_end() {
        let mut html = MDBOOK_PAGE.to_string();
        apply_vault_display_options(&mut html, true, "");
        let expected = format!("{PAGE_NUMBERS_OVERRIDE}</body>");
        assert!(
            html.contains(&expected),
            "override style immediately before </body>: {html}"
        );
        assert!(
            !html.contains(r#"default_light_theme = "coal";"#),
            "theme untouched when unset"
        );
    }

    #[test]
    fn display_options_off_is_inert() {
        let mut html = MDBOOK_PAGE.to_string();
        apply_vault_display_options(&mut html, false, "");
        assert_eq!(html, MDBOOK_PAGE, "both options off: byte-identical");
    }

    #[test]
    fn display_options_runs_on_chrome_early_return_page() {
        // Converted vault: the page already wires zoom.css + morinet.js, so
        // the chrome pass early-returns — the option pass must still run.
        let page = MDBOOK_PAGE.replace(
            "</head>",
            "<link rel=\"stylesheet\" href=\"theme/zoom.css\"><script src=\"theme/morinet.js\"></script></head>",
        );
        let out = inject_vault_chrome(page.into_bytes(), true, "coal");
        let body = String::from_utf8(out).unwrap();
        assert!(
            body.contains(r#"default_light_theme = "coal";"#)
                && body.contains(r#"default_dark_theme = "coal";"#),
            "theme rewrite applied despite chrome early-return: {body}"
        );
        assert!(
            body.contains(PAGE_NUMBERS_OVERRIDE),
            "page-numbers override applied despite chrome early-return"
        );
        assert!(
            !body.contains("morinet-zoom"),
            "chrome not injected twice: {body}"
        );
    }

    /// Seed a vault whose book dir holds `page` as its index.html. Slug must
    /// be unique per test (pitfalls #16/#23: parallel tests, shared tmpdir).
    async fn seed_options_vault(state: &SharedState, slug: &str, page: &str) {
        let dir = std::env::temp_dir().join(format!("morinet-opt-{slug}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), page).unwrap();
        state
            .store
            .seed_vault(slug, "Options vault", &dir)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn serve_page_numbers_off_hides_by_default() {
        let (app, state) = test_router().await;
        seed_options_vault(&state, "optoff", MDBOOK_PAGE).await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/optoff/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        // D-09/D-12 "off" state: no override — the global zoom.css hide rule
        // remains the only rule touching chapter numbers.
        assert!(
            !body.contains(PAGE_NUMBERS_OVERRIDE),
            "default vault must not get the override: {body}"
        );
    }

    #[tokio::test]
    async fn serve_page_numbers_on_injects_override() {
        let (app, state) = test_router().await;
        seed_options_vault(&state, "opton", MDBOOK_PAGE).await;
        state
            .store
            .set_keep_page_numbers("opton", true)
            .await
            .unwrap();
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/opton/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        let chrome = body
            .find("morinet-zoom")
            .expect("vault chrome present (both passes coexist)");
        let override_pos = body
            .find(PAGE_NUMBERS_OVERRIDE)
            .expect("override style injected");
        let body_end = body.rfind("</body>").unwrap();
        assert!(
            chrome < override_pos && override_pos < body_end,
            "override appears after chrome injection and before </body>"
        );
    }

    #[tokio::test]
    async fn serve_theme_rewrite_applied() {
        let (app, state) = test_router().await;
        seed_options_vault(&state, "opttheme", MDBOOK_PAGE).await;
        state.store.set_theme("opttheme", "rust").await.unwrap();
        let cookie = login_cookie(&app).await;
        // Directory→index branch and direct file branch both apply the pass.
        for uri in ["/vault/opttheme/", "/vault/opttheme/index.html"] {
            let res = req(app.clone(), "GET", uri, cookie_hdr(&cookie), None).await;
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            let body = body_text(res).await;
            assert!(
                body.contains(r#"default_light_theme = "rust";"#)
                    && body.contains(r#"default_dark_theme = "rust";"#),
                "theme rewritten in {uri}: {body}"
            );
            assert!(
                !body.contains(r#"default_light_theme = "light";"#)
                    && !body.contains(r#"default_dark_theme = "navy";"#),
                "original literals gone in {uri}"
            );
        }
    }

    #[tokio::test]
    async fn serve_theme_unset_leaves_page_untouched() {
        let (app, state) = test_router().await;
        seed_options_vault(&state, "optunset", MDBOOK_PAGE).await;
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/optunset/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains(r#"default_light_theme = "light";"#)
                && body.contains(r#"default_dark_theme = "navy";"#),
            "theme unset: both original literals intact: {body}"
        );
    }

    #[tokio::test]
    async fn serve_converted_vault_page_still_gets_options() {
        let (app, state) = test_router().await;
        // Simulates the converted-vault early-return path: the page already
        // wires morinet.js, so chrome injection skips it — the option pass
        // must still rewrite the theme literals.
        let page = MDBOOK_PAGE.replace(
            "</head>",
            "<script src=\"theme/morinet.js\"></script></head>",
        );
        seed_options_vault(&state, "optconv", &page).await;
        state.store.set_theme("optconv", "coal").await.unwrap();
        state
            .store
            .set_keep_page_numbers("optconv", true)
            .await
            .unwrap();
        let cookie = login_cookie(&app).await;
        let res = req(app, "GET", "/vault/optconv/", cookie_hdr(&cookie), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains(r#"default_light_theme = "coal";"#)
                && body.contains(r#"default_dark_theme = "coal";"#),
            "converted page still gets the theme rewrite: {body}"
        );
        assert!(
            body.contains(PAGE_NUMBERS_OVERRIDE),
            "converted page still gets the page-numbers override"
        );
        assert!(
            !body.contains("morinet-zoom"),
            "chrome not double-injected on converted page"
        );
    }
}
