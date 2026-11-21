# Phase 17 Pattern Map — Generic OIDC SSO

Anchors verified against the working tree on 2026-11-04. Verbatim excerpts, not summaries.
All line numbers are from the current `HEAD` state of the files named.

> **Correction to the task brief:** the repo does **not** use rusqlite. The DB layer is
> **sqlx 0.8** (`sqlite`, `runtime-tokio`, `migrate` features — Cargo.toml:36), async
> `sqlx::query`/`query_as` throughout `src/store.rs`. Phase 17 store helpers must be
> async sqlx, not rusqlite.

---

## 1. Config: `FileConfig` sections + env-wins merge (`src/config.rs`)

### 1a. `FileConfig` — every section is `#[serde(default)]` (config.rs:18-38)

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub server: ServerSection,
    #[serde(default)]
    pub session: SessionSection,
    #[serde(default)]
    pub storage: StorageSection,
    #[serde(default)]
    pub vaults: VaultsSection,
    /// Phase 5: outbound email. Absent/empty disables delivery — the app
    /// stays usable and the admin falls back to the manual flow.
    #[serde(default)]
    pub smtp: SmtpConfig,
    /// Bootstrap admin credentials (fallback when the env vars are absent).
    #[serde(default)]
    pub admin: AdminSection,
    /// Phase 12: locked-account guard (demo instance). Absent = nobody locked.
    #[serde(default)]
    pub security: SecuritySection,
}
```

`[oidc]` joins this struct as `#[serde(default)] pub oidc: OidcSection` — the
`smtp`/`security` doc-comment pattern ("absent = feature disabled") is exactly D-06's
"disabled by default, fully removable".

### 1b. Representative section shapes (config.rs:43-57)

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SecuritySection {
    #[serde(default)]
    pub locked_accounts: Vec<String>,
}

/// `[admin]` section: bootstrap admin credentials written by `morinet init`.
/// Env vars `MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD` take precedence.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdminSection {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}
```

`OidcSection` = `Default` + all-`Option`/`Vec` fields (issuer, client_id, client_secret,
allowed_email_domains, groups_claim, group mapping table, sso_label) so an absent
`[oidc]` deserializes to "disabled" with zero special-casing.

### 1c. ENV_* consts (config.rs:10-16)

```rust
pub const DEFAULT_COOKIE_NAME: &str = "morinet_session";
pub const ENV_COOKIE_SECRET: &str = "MORINET_COOKIE_SECRET";
pub const ENV_ADMIN_EMAIL: &str = "MORINET_ADMIN_EMAIL";
pub const ENV_ADMIN_PASSWORD: &str = "MORINET_ADMIN_PASSWORD";
/// Phase 12 (D-01): comma-separated emails whose vault mutations are refused
/// on the public demo instance. Wins over the `[security]` file section.
pub const ENV_LOCKED_ACCOUNTS: &str = "MORINET_LOCKED_ACCOUNTS";
```

Phase 17 adds e.g. `ENV_OIDC_ISSUER`, `ENV_OIDC_CLIENT_ID`, `ENV_OIDC_CLIENT_SECRET`.

### 1d. Env-wins merge — two distinct precedents (config.rs:168-215)

**Secret pattern (hard-error when missing)** — `MORINET_ADMIN_PASSWORD`, config.rs:181-184:

```rust
    let admin_password = std::env::var(ENV_ADMIN_PASSWORD)
        .ok()
        .or_else(|| cfg.admin.password.clone().filter(|s| !s.is_empty()))
        .ok_or_else(|| anyhow::anyhow!("missing admin password: set {ENV_ADMIN_PASSWORD}"))?;
```

**Opt-in pattern (never a hard error, blank-env ≠ shadow)** — `locked_accounts`,
config.rs:185-198. **This is the one `[oidc]` must copy** (D-06: absent section must
never break boot):

```rust
    // Phase 12 (D-01): env wins when set; never a hard error — empty default.
    // Review fix (MEDIUM): a set-but-blank env value normalizes to an empty
    // list — treat it as UNSET so it cannot silently shadow (fail open) the
    // file section's lock list.
    let locked_accounts = std::env::var(ENV_LOCKED_ACCOUNTS)
        .ok()
        .map(|raw| normalize_locked_list(raw.split(',')))
        .filter(|v| !v.is_empty())
        .or_else(|| {
            let from_file =
                normalize_locked_list(cfg.security.locked_accounts.iter().map(String::as_str));
            (!from_file.is_empty()).then_some(from_file)
        })
        .unwrap_or_default();
```

Client-secret resolution: env-wins like `admin_password` (`env.or(file.filter(non-empty))`)
but **no `ok_or_else` hard error** — if `[oidc]` is otherwise configured and the secret
resolves to `None`, either fail resolve with an oidc-specific message or keep SSO
disabled; the fail-closed posture says: configured-but-secretless ⇒ SSO disabled +
startup warning, never a half-enabled flow.

### 1e. `RuntimeConfig` assembly (config.rs:127-144, 200-214)

```rust
pub struct RuntimeConfig {
    pub bind_addr: String,
    pub base_url: String,
    pub cookie_secret: Vec<u8>,
    pub cookie_name: String,
    pub admin_email: String,
    pub admin_password: String,
    /// Phase 5: outbound email config (empty = delivery disabled).
    pub smtp: SmtpConfig,
    ...
    pub locked_accounts: Vec<String>,
}
```

resolved fields are appended in `Ok(RuntimeConfig { ... })` at config.rs:200-214.
Phase 17 adds `pub oidc: Option<OidcRuntime>` (or an `OidcConfig` with an `is_enabled()`
like `SmtpConfig::is_usable()` — the smtp precedent at config.rs:314 `cfg.smtp.is_usable()`
is the "section present ⇒ feature on" idiom to mirror).

**Test precedent for env-mutating config tests:** `static ENV_LOCK: std::sync::Mutex<()>`
serialize pattern, config.rs:221-223 — reuse for any `[oidc]` env tests.

---

## 2. Session mint + verify (`src/session.rs`) and the `login_submit` cookie call site

### 2a. Codec signatures (session.rs:13-53)

```rust
pub const DEFAULT_TTL_SECS: u64 = 24 * 3600;

pub struct SessionCodec { secret: Vec<u8> }

#[derive(Debug, Clone)]
pub struct Claims {
    pub email: String,
    pub expires_at: u64,
}

    pub fn new(secret: impl Into<Vec<u8>>) -> Self { ... }

    /// Sign a session token for `email`, valid from `now` for `ttl_secs`.
    pub fn sign(&self, email: &str, now: u64, ttl_secs: u64) -> String { ... }

    /// Verify a token against `now`. Returns the claims on success, None on any
    /// tampering, bad encoding, expired token, or wrong signature.
    pub fn verify(&self, token: &str, now: u64) -> Option<Claims> { ... }
```

### 2b. The exact success path the SSO callback must end at (server.rs:433-457)

```rust
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
```

D-02: `/sso/callback` ends with **byte-identical** cookie construction —
`state.codec.sign(email, unix_now(), DEFAULT_TTL_SECS)` + the same
`"{name}={token}; HttpOnly; Path=/; SameSite=Lax"` format string. Note the cookie
carries the **email string** — for SSO-provisioned users, sign the local `users.email`
row's email, not the raw IdP claim.

`unix_now()` lives at server.rs:133-138. Cookie parsing on the way in:
`parse_cookies` (server.rs:141-147) + `require_session` (server.rs:462-482) — untouched
by Phase 17.

---

## 3. Login page: `login_form` + `LOGIN_HTML` (D-14 SSO button insertion point)

### 3a. Handler (server.rs:423-425) — **static, stateless today**

```rust
async fn login_form() -> Html<&'static str> {
    Html(LOGIN_HTML)
}
```

### 3b. The HTML (server.rs:73-87)

```html
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
```

**Flag (D-14):** `login_form` takes no `State`, so it cannot see whether `[oidc]` is
configured. To conditionally render the SSO button + "ou" divider **above** the password
form, `login_form` must become `async fn login_form(State(state): State<SharedState>)`
and build a `String` (two static variants concatenated, or `format!` with an
`SSO_BLOCK` fragment inserted before the `<form>`). The failure path at server.rs:455
(`Html(LOGIN_HTML)` in `login_submit`) renders the same page and must get the same
treatment — both call sites share the constant. `sso_label` is user-config input →
`html_escape` it (escape helper used everywhere, e.g. server.rs:104).

---

## 4. Account page: "lié via SSO" indicator row (D-13)

`account_page` (server.rs:1214-1219) delegates to `account_html`
(server.rs:1223-1295). The **Profile panel** is the insertion point
(server.rs:1269-1276):

```html
  <section class="panel">
    <h2>Profile</h2>
    <p><strong>{label}</strong><br><small>{email}</small></p>
    <form method="post" action="/account/name" class="stacked">
      <div class="field field-wide"><label for="display-name">Display name</label><input id="display-name" type="text" name="name" value="{label}" placeholder="display name"></div>
      <button type="submit">Save</button>
    </form>
  </section>
```

Pattern to follow: `account_html` already does an async store lookup per render
(server.rs:1224-1236: `display_name_for`, then `user_id_for` + `list_api_tokens`,
each `.await.ok().flatten()` / `.unwrap_or_default()` — a lookup failure degrades the
page, never errors it). A `store.identity_for_user(user_id)` lookup joined the same way
feeds an optional `<p>Linked via SSO ({iss})</p>` row after the `<p><strong>{label}…`
line. `account_html` is also re-rendered by `create_account_token` (server.rs:1323), so
the indicator appears there too for free.

---

## 5. Groups: store surface the group-sync (D-09/D-10/D-15) will call (`src/store.rs`)

All async sqlx, `&self`, `anyhow::Result`:

```rust
// store.rs:1028
pub async fn create_group(&self, name: &str) -> anyhow::Result<i64>
// store.rs:1036
pub async fn list_groups(&self) -> anyhow::Result<Vec<(i64, String, i64, Option<i64>)>>
    // (id, name, member count, personal_vault_limit) — limit NULL = unlimited
// store.rs:1059
pub async fn rename_group(&self, group_id: i64, name: &str) -> anyhow::Result<()>
// store.rs:1069
pub async fn delete_group(&self, group_id: i64) -> anyhow::Result<()>  // cascades members + ACLs
// store.rs:1077
pub async fn add_group_member(&self, group_id: i64, email: &str) -> anyhow::Result<bool>
    // INSERT OR IGNORE; false = unknown email
// store.rs:1093
pub async fn remove_group_member(&self, group_id: i64, email: &str) -> anyhow::Result<bool>
// store.rs:1107
pub async fn group_members(&self, group_id: i64) -> anyhow::Result<Vec<String>>  // emails, sorted
```

Group-sync diff loop = `list_groups()` (map configured names → id) +
`group_members(id)` (current) + `add_group_member`/`remove_group_member` (delta).
There is **no** "memberships of a user" query yet — D-09's per-user recompute will want
a new one (mirror `group_members` with the join reversed:
`SELECT m.group_id FROM group_members m WHERE m.user_id = ?`).

Role ↔ group interplay (the D-07 safety net):

```rust
// store.rs:441 — set_user_role: role change also fixes plan group
pub async fn set_user_role(&self, email: &str, role: &str) -> anyhow::Result<()> {
    anyhow::ensure!(role == "admin" || role == "reader", "unknown role {role}");
    ...
}

// store.rs:420 — anti-lockout fallback (D-07 analogue at the store level)
/// Anti-lockout fallback: if NO account has the admin role at boot, create
/// (or re-promote) the well-known `admin`/`admin` account. ...
pub async fn ensure_fallback_admin(&self) -> anyhow::Result<()>
```

Also relevant: `ensure_builtin_groups` (store.rs:376, `free`/`admin` plan groups),
`ensure_default_memberships` (store.rs:395 — admins always in `admin`, groupless
readers in `free`), `create_user` (store.rs:349 — new users auto-join `free`/`admin`
plan group; **auto-provisioned SSO readers get this for free** if provisioning calls
`create_user`). Note `create_user` requires a password — SSO-provisioned users need an
impossible hash or a new `create_user_without_password`; keep `role='reader'` (D-04).
Admin UI surface for groups: `admin_groups` handler server.rs:1897, routes
server.rs:255-260.

---

## 6. Migrations: dir listing, additive-ALTER exemplar, application mechanism

`migrations/` (next free number = **0012**, confirming D-11):

```
0001_users.sql            0005_group_quota.sql    0009_vault_display_options.sql
0002_vaults.sql           0006_vault_source.sql   0010_vault_changes.sql
0003_tokens.sql           0007_api_tokens.sql     0011_vault_changes_forced_actor.sql
0004_groups.sql           0008_shares_stats.sql
```

Additive exemplar — `migrations/0011_vault_changes_forced_actor.sql` (verbatim, whole file):

```sql
-- Phase 15 (CONF-04, D-05): force-overwrite audit columns.
--
--   forced  1 when the write overwrote a conflicting file after the user
--           explicitly confirmed "Upload anyway" (D-02/D-03); 0 otherwise.
--           Force-overwrites are recorded as `modified` rows — the kind
--           vocabulary from migration 0010 stays exactly added/modified/
--           removed (D-05, CHECK constraint untouched).
--   actor   email of the user who performed the write, populated on WebDAV
--           PUTs, upload updates and force-overwrites (D-06); '' when the
--           actor is unknown (e.g. git refresh paths).

ALTER TABLE vault_changes ADD COLUMN forced INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vault_changes ADD COLUMN actor TEXT NOT NULL DEFAULT '';
```

For a **new table** (`user_identities`), the exemplar is `0010_vault_changes.sql`:
`CREATE TABLE IF NOT EXISTS` + `CHECK`-locked vocabulary + a named
`CREATE INDEX IF NOT EXISTS idx_...`. `0001_users.sql` shows the UNIQUE-column shape
(`email TEXT NOT NULL UNIQUE`) that `UNIQUE(provider, iss, sub)` will copy.

Application: **compile-time embedded** via sqlx's `migrate!` macro —
`src/store.rs:117` and `:127` (both `Store::connect` paths):

```rust
sqlx::migrate!("./migrations").run(&pool).await?;
```

Dropping `0012_user_identities.sql` in the dir is sufficient; no registration code.
Migration-shape tests exist as precedent: `vault_changes_migration_shape`
(store.rs:3232), `shares_stats_migration_shape` (store.rs:1983),
`migrations_create_tables` (store.rs:1638).

---

## 7. Audit/logging precedent for link/demote events

Two existing patterns, both "best-effort, never break the request":

```rust
// store.rs:1372 — access_log insert (fire-and-forget; CHECK-enforced source vocab)
pub async fn log_access(
    &self,
    email: &str,
    slug: &str,
    source: &str,
    path: &str,
) -> anyhow::Result<()>

// store.rs:1441 — vault_changes insert (Phase 13; D-04: errors logged & swallowed)
/// Record one vault file change. Callers treat this as best-effort:
/// errors must be logged and swallowed, never propagated into a response
/// (D-04). `kind` is 'added', 'modified' or 'removed' (CHECK-enforced by
/// migration 0010, D-08); all rows of one refresh share one `event_id`.
/// `actor` is the acting user's email ('' when unknown, D-06); `forced`
/// marks an explicit force-overwrite (D-05, recorded as `modified`).
pub async fn log_vault_change(
    &self,
    slug: &str,
    event_id: &str,
    kind: &str,
    path: &str,
    actor: &str,
    forced: bool,
) -> anyhow::Result<()>
```

Retention sweep precedent: `prune_access_log` (store.rs:1426, 90-day window, "called
once at boot from `main::serve`"). CONTEXT discretion (dedicated `auth_events` table vs
reuse): `vault_changes` is vault-scoped with a CHECK-locked `kind` — a dedicated
`auth_events(ts, kind, actor_email, iss, sub, detail)` table in migration 0012 is the
clean fit; the reusable *pattern* is the insert fn + CHECK vocab + boot prune, not the
table itself. Log actor + iss + sub, never tokens/codes (CONTEXT specifics).

---

## 8. Test harness (`src/server.rs` tests mod) + mock-IdP options

Harness anchors: `test_config` (server.rs:3916-3930, `admin@example.com` /
`test-pass-9f3a`, memory store via `Store::connect_memory`), `test_router`
(server.rs:3952-3969, builds via `build_router_with_email`), raw request driver
(server.rs:4005-4018):

```rust
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
```

Login-driven cookie (server.rs:4027-4046): POST `/login` form body
`email=admin%40example.com&password=test-pass-9f3a`, assert `SEE_OTHER`, pull
`SET_COOKIE`, truncate at `;`. SSO tests reuse this shape: drive `/sso/callback`
with a forged-by-test-IdP code, assert the same cookie appears (D-02 regression).

**No external-HTTP mocking dep exists:** `[dev-dependencies]` is only
`pretty_assertions = "1.4"` (Cargo.toml:45-46); Cargo.lock has no wiremock/mockito/
httpmock and **no reqwest at all**. Options that don't fight rustls:
1. **Local `tokio::net::TcpListener` in-test IdP** serving discovery/JWKS/token over
   plain HTTP on `127.0.0.1:0`, with `openidconnect`'s `ProviderMetadata` fetched from
   it (http URLs need no TLS at all in tests) — matches CONTEXT's "tiny in-test IdP"
   idea; test RSA key generated with the `rsa` crate or a fixed PEM.
2. **`wiremock` crate** (http-only mock server, hyper-based, no TLS stack of its own) —
   same plain-HTTP benefit, nicer API.
3. **Manually constructed `ProviderMetadata`** pointing endpoints at the local listener
   (skips the discovery fetch; CONTEXT explicitly notes `openidconnect` accepts this).
Avoid `mockito`/`httpmock` TLS modes — they'd drag rustls/native-tls config fights.
The mock-IdP dep goes in `[dev-dependencies]`, not `[dependencies]`.

---

## 9. Cargo.toml: current deps + where `openidconnect` lands

Current `[dependencies]` (Cargo.toml:20-43, verbatim):

```toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1.40", features = ["macros", "rt-multi-thread", "sync"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
axum = { version = "0.8", features = ["multipart"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["fs"] }
hmac = "0.12"
sha2 = "0.10"
async-trait = "0.1"
hex = "0.4"
subtle = "2.6"
base64 = "0.22"
serde_json = "1"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "migrate"] }
bcrypt = "0.15"
git2 = { version = "0.19", features = ["vendored-openssl"] }
uuid = { version = "1", features = ["v4"] }
lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1", "tokio1-rustls-tls", "builder"] }
rmcp = { version = "3.7.0", features = ["server", "transport-streamable-http-server"] }
schemars = "1"
rpassword = "7"
```

`openidconnect = { version = "4.0.1", default-features = false, features = ["reqwest", "rustls-tls"] }`
joins this block (D-01: only new dependency). **TLS-stack audit (measured):**
- `reqwest`: **absent from Cargo.lock today** — openidconnect introduces it. Pin its
  rustls path (`rustls-tls`) so it resolves to the existing single
  **rustls 0.23.46** (Cargo.lock has exactly one `rustls` entry, pulled by
  `lettre/tokio1-rustls-tls`). A second rustls major (0.21) or native-tls would be a
  duplicate-stack regression — verify with `cargo tree -d` after adding.
- `git2` vendors OpenSSL but nothing else uses native-tls; keep it that way by
  disabling openidconnect/reqwest default features (native-tls sneaks in via defaults
  on some versions).
- `base64 0.22`, `sha2 0.10`, `hmac 0.12` already present — openidconnect's JWKS
  crypto overlaps, no conflict expected.

---

## 10. Router integration: where `/sso/login` + `/sso/callback` join

Public group in `build_router_with_email` (server.rs:183-208):

```rust
    // Public routes.
    let public = Router::new()
        .route("/health", get(health))
        ...
        .route("/login", get(login_form).post(login_submit))
        .route("/reset", get(reset_request_form).post(reset_request_submit))
        .route("/reset/{token}", get(reset_form).post(reset_submit))
        .route(
            "/invite/{token}",
            get(invite_accept_form).post(invite_accept_submit),
        )
        ...
        .with_state(state.clone());
```

`/sso/login` (GET) and `/sso/callback` (GET) join **this `public` block**, next to
`/login` — i.e. outside `protected`'s `route_layer(require_session)` (server.rs:239-242)
and outside `admin`'s `require_admin` (server.rs:265). Final merge order is
`public.merge(protected).merge(admin).merge(webdav).merge(mcp).merge(api)`
(server.rs:289-295). Handlers take `State(state)` like every public handler does.
When `[oidc]` is absent the routes can either 404 (return not-found response inside the
handler — do NOT conditionally omit routes… actually conditional registration on the
public Router is also fine since it's built once at boot; either way, no gate is
touched).

---

## 11. No-analog items (nothing in-repo to copy)

- **Full OIDC RP flow** (discovery, PKCE S256, auth URL, token exchange, JWKS-verified
  ID token): zero precedent — `src/session.rs`'s HMAC cookie is *not* JWT tooling and
  stays as-is. `openidconnect` 4.0.1 API surface is the reference; D-01 forbids
  hand-rolling.
- **State/nonce storage**: no short-lived-signed-cookie precedent beyond `SessionCodec`
  itself (reusing it for state is viable: `codec.sign(&state, now, 600)` + verify on
  callback — same secret, constant-time verify). CONTEXT leaves cookie-vs-table to the
  planner; the codec is the in-repo primitive either way.
- **Email normalization used by password login (D-12 "same rule")** — **finding: there
  is none.** `verify_login` (store.rs:186-192) is an exact `WHERE email = ?` match
  (SQLite default BINARY collation — case-sensitive); no `LOWER()`/`COLLATE` anywhere
  in store.rs/server.rs; handlers only `.trim()` (e.g. server.rs:2607). The nearest
  lowercase-normalization precedents are `normalize_locked_list`
  (config.rs:148-153: `trim().to_ascii_lowercase()`, drop empties) and
  `vault_slug_from_name` (server.rs:3103). **Flag for planner:** D-12's "normalized
  case, same rule as password login" cannot be satisfied by copying password login —
  either adopt `trim + to_ascii_lowercase` for the SSO email match (diverging from
  password login) or store provisioned emails lowercase and note the divergence.
  Case-insensitive match must be done in SQL (`LOWER(u.email) = LOWER(?)`) or by
  normalizing before lookup — decide once, document in the plan.

---

## 12. Anti-patterns to flag

1. **`Router::fallback` whole-app trap** (Phase 16 lesson, verbatim at api.rs:677-679):
   ```rust
   // Registered BEFORE route_layer so it sits INSIDE the Bearer group:
   // unknown paths answer 401 without a token, 404 problem+json with
   // one. NEVER Router::fallback — a merged router's fallback becomes
   // the whole-app fallback and would change site-wide non-API 404s.
   ```
   Same trap applies to any SSO error handling: use per-handler error pages
   (`error_page`, server.rs) — never a fallback on a merged sub-router.
2. **`std::sync::Mutex` held across `.await`**: `AppState` carries two std mutexes
   (`vault_locks`, `api_rate`, server.rs:179-180). The established discipline is
   `vault_lock()` (server.rs:54-58): *"The std Mutex is released before returning"* —
   grab map entry, clone the inner `Arc<tokio::sync::Mutex<()>>`, drop the std guard,
   then `.lock().await` the async mutex (server.rs:963, :3506). If SSO state/nonce or
   rate-limit state needs an in-memory map, copy this two-tier pattern exactly.
3. **`.planning/STATE.md` ownership**: executors don't touch it (curator/GSD-owned);
   Phase 17 plans/commits must not stage it.
4. **Cookie drift**: any new sign-in path must reuse the exact cookie string from
   §2b — a subtly different `SameSite`/`Path`/`HttpOnly` set is a parallel session
   mechanism (D-02 violation) that silently diverges from `require_session` behavior.
5. **Fail-closed**: unmatched domain, admin-match link, absent admin-group claim ⇒
   refuse (D-04/D-05/D-10); mirror Phase 12's blank-env fail-closed fix (§1d).
