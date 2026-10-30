# ARCHITECTURE research — v1.2 Team Access (OIDC SSO + read-only REST API + ETag conflict detection)

Researched 2026-10-30 against `src/server.rs` (8033 L), `src/store.rs` (3327 L), `src/config.rs`, `src/session.rs`, `src/webdav.rs`, `src/mcp.rs`, `migrations/0001..0010`.

## 1. Current architecture facts that constrain v1.2

- **Router shape** (`build_router_with_email`, server.rs:142–250+): five branches merged into one axum `Router` —
  - `public` (no middleware): `/health`, `/login`, `/reset*`, `/invite/{token}`, `/assets/style.css`
  - `protected` (`route_layer(require_session)`): cookie-auth HTML pages, `/vault/{slug}/*`
  - `admin` (`route_layer(require_admin)`, self-contained cookie check)
  - `webdav` (`route_layer(require_dav_auth)`, HTTP Basic, catch-all `any(webdav_entry)` because PROPFIND is not a router verb)
  - `mcp` (no middleware — `mcp_entry` (server.rs:260) self-authenticates `Authorization: Bearer` via `store.user_for_api_token`)
  All branches share `SharedState = Arc<AppState { codec: SessionCodec, cfg: RuntimeConfig, store: Store, email }>`.
- **Session layer** (`src/session.rs`): stateless HMAC-signed cookie `morinet_session` = `b64url(email).expiry.hmac` — **no DB session table, provider-agnostic** (claims carry only email + expiry). `login_submit` (server.rs:392) signs with `state.codec.sign(email, now, DEFAULT_TTL_SECS)` and sets `HttpOnly; Path=/; SameSite=Lax`.
- **Config** (`src/config.rs`): `FileConfig` (serde TOML sections, all `#[serde(default)]`) → `resolve()` → `RuntimeConfig`; env-wins pattern (`MORINET_*` env > file section, blank-env treated as unset — see `locked_accounts` fix at config.rs:189). Precedent to copy exactly for `[oidc]`.
- **Store** (`src/store.rs`, sqlx SQLite, `sqlx::migrate!`, next migration = **0011**):
  - `users(id, email UNIQUE, password_hash NOT NULL, role CHECK('admin','reader'), created_at, display_name)` — `password_hash NOT NULL` is the OIDC pinch point.
  - `groups` / `group_members` / `vault_group_acl` (0004) with ready-made fns: `create_group`, `add_group_member(email)`, `remove_group_member`, `ensure_builtin_groups`.
  - `api_tokens(user_id, name, token_hash UNIQUE sha256, created_at, last_used_at)` (0007) + `user_for_api_token(plain) -> Option<User>` — the exact auth primitive the REST API needs.
  - `accessible_vaults(email)` (store.rs:730) and `access_for(email, slug) -> Access` (store.rs:672, `can_read()/can_write()`) — the ACL pair both MCP and WebDAV already use.
  - `vault_changes` (0010) fed fire-and-forget per WebDAV PUT (`log_vault_change`, server.rs:908–922).
- **WebDAV** (`webdav_entry`, server.rs:796–930 + `src/webdav.rs` helpers): GET = `tokio::fs::read` with **no ETag/Last-Modified on file responses**; PUT = write + `log_vault_change` + `rebuild_book`. Helpers `source_dir_of`, `safe_join`, `rel_from_uri`, `parse_basic` are `pub` in `webdav.rs`. **No `If-Match`/`If-None-Match` handling exists anywhere today** (grep confirms zero hits).
- **MCP** (`src/mcp.rs`): `MoriMcp { store }` tools `list_vaults` / `list_notes` / `read_note` / `search_notes` — each is thin: `accessible_vaults` → `source_dir_of` → `safe_join` → `list_md_files` / `search_md` / `fs::read_to_string`. `list_md_files` and `search_md` are `pub(crate)` — **already reusable from `server.rs` without changes**.
- **Dependencies** (Cargo.toml): **no HTTP client exists** (no reqwest; lettre uses rustls, git2 vendored-openssl). OIDC will be the first outbound HTTP caller → a client dep is unavoidable.

## 2. Integration points

### 2.1 OIDC SSO

- **Config**: new `[oidc]` section → `OidcSection { issuer, client_id, client_secret, allowed_domains: Vec<String>, groups_claim: Option<String>, group_map: HashMap<String,String> (claim value → MoriNet group name), admin_group: Option<String> }` on `FileConfig` + `Option<OidcSection>` on `RuntimeConfig` (None = feature off, SSO button hidden). Env override `MORINET_OIDC_CLIENT_SECRET` wins over file (copy the `locked_accounts` env-wins + blank-is-unset pattern).
- **New file `src/oidc.rs`**: discovery doc fetch (issuer `/.well-known/openid-configuration`), JWKS cache, auth-URL builder, token exchange, ID-token validation (signature/iss/aud/exp/nonce). This is the only component with real new logic; everything downstream is existing machinery.
- **Routes (both on the `public` router — they run before any session exists)**:
  - `GET /auth/oidc/start` → build auth URL with `state` + `nonce`, 302 to IdP.
  - `GET /auth/oidc/callback` → validate `state`, exchange code, validate ID token, **auto-provision/link user by email**, sync groups, then do *exactly what `login_submit` does*: `state.codec.sign(email, …)` → `Set-Cookie` → `Redirect::to("/")`.
  - The callback fits the session layer **unchanged**: the cookie codec only knows email+expiry, so an SSO login and a password login are indistinguishable downstream. Zero changes to `require_session`/`require_admin`/`require_dav_auth`.
- **CSRF `state`**: no server-side session store exists; use a short-lived HMAC-signed state cookie (same `SessionCodec` construction, different payload prefix + 5-min TTL) or a small `oidc_states` table. Signed-cookie is preferred: stateless, no cleanup job, matches AD-4.
- **Auto-provisioning rules** (in order): email matches existing user → link & sign in; else email domain ∈ `allowed_domains` → create user (`provider='oidc'`); else reject with a friendly page.
- **Group sync**: on every SSO login, read `groups_claim` from ID token; for each mapped claim value `create_group(name)` (idempotent — `name UNIQUE`) + `add_group_member(group, email)`. `admin_group` claim → `role='admin'`. Removal-on-login-absence is a policy decision — recommend **additive-only** in v1.2 (removal needs full-sync semantics and can lock people out on partial claims).
- **New deps**: `openidconnect` (or lighter `oauth2` crate) + `reqwest` with **rustls** (not native-tls — keeps the static-binary / `cargo install` constraint; precedent: lettre already chose rustls).

### 2.2 Read-only REST API

- **New router branch** `api` in `build_router_with_email`: `/api/v1/vaults`, `/api/v1/vaults/{slug}/notes`, `/api/v1/vaults/{slug}/notes/{*path}`, `/api/v1/search` — with its own `require_api_token` middleware cloned from the `mcp_entry` pattern: `Authorization: Bearer` → `store.user_for_api_token` → insert `Claims` into extensions (identical extension contract, so handlers read identity the same way). **Do not mount on the cookie `protected` router** — tokens must not depend on browser cookies, and the cookie middleware would 302-redirect JSON clients to `/login`.
- **Reuse from MCP — this is the big win**: the four tool bodies in `mcp.rs` *are* the REST business logic. Extract the pure parts into shared helpers (they already are `pub(crate)`: `list_md_files`, `search_md`, plus `store.accessible_vaults`, `webdav::source_dir_of`, `webdav::safe_join`) and have both `MoriMcp` tools and REST handlers call them. Recommended shape: new `src/api.rs` with plain async fns returning `serde_json::Value`; `mcp.rs` tools become thin wrappers (or stay as-is and `api.rs` duplicates the 5-line call sequence — extraction is cleaner and keeps ACL logic single-sourced).
- **Response shape**: JSON (`{"slug","name","scope","role"}` per vault — same fields MCP `list_vaults` emits; search hits = same `Hit` struct, already `Serialize`). Add `Cache-Control: no-store` and ETag on note reads (see 2.3).

### 2.3 ETag conflict detection

- **Where computation lives**: file metadata at the WebDAV layer — `webdav.rs` gains `etag_of(&std::fs::Metadata) -> String` (weak-safe strong tag: `"<mtime_unix>-<size>"` quoted, or `sha256(content)` for exactness; markdown notes are small, content hash is affordable and survives touch-only changes). One helper, used by three call sites.
- **WebDAV GET** (server.rs:862): add `ETag` header to the file response; optionally `getetag` in PROPFIND `prop_entry` (webdav.rs:136) so clients like Obsidian WebDAV plugins can pre-check.
- **WebDAV PUT** (server.rs:876, before `tokio::fs::write` at :901): if `If-Match` present → compute current etag, mismatch ⇒ `412 Precondition Failed` (no write, no rebuild, no `vault_changes` row). If `If-None-Match: *` and file exists ⇒ `412` (client thinks it's new, server says it exists). Missing headers ⇒ today's last-write-wins behavior (backward compatible, no client breakage).
- **Upload-time check** (`/vaults/upload`, local re-upload path): same helper — hash the incoming file set vs on-disk before overwrite, refuse with a conflict message when a file changed since the client's snapshot. Keep it at the same call site as the existing quota/lock gates.
- **REST API reads** get the same `ETag` header for free from the shared helper (client round-trip: REST read → WebDAV PUT with `If-Match`).
- **No new table needed**: ETags are derived from the filesystem at request time; `vault_changes` already records what changed. Do **not** add a per-file hash column — it would need invalidation on git-refresh, convert pipeline, and PUT (three writers), and mtime+size/content-hash is already consistent because all writers go through these paths.

## 3. Data-flow changes

1. **Login flow**: `GET /login` (unchanged) + new "Sign in with SSO" button → `/auth/oidc/start` → IdP → `/auth/oidc/callback` → provision/link + group sync → **same** `Set-Cookie` + `Redirect("/")` as password login. All downstream request flows (web, admin, WebDAV, MCP, REST) are untouched.
2. **REST read flow**: client `Bearer` token → `require_api_token` → `Claims` extension → handler → `accessible_vaults`/`access_for` → fs read via `source_dir_of`+`safe_join` → JSON (+ETag). No rebuild, no writes.
3. **Conflict flow**: client `GET /webdav/…/note.md` → gets `ETag: "abc"` → edits → `PUT If-Match: "abc"` → server re-computes → equal ⇒ write+rebuild+log (today's path); different ⇒ `412`, client re-fetches and merges manually (no locks, no CRDT — matches Out-of-Scope).

## 4. New vs modified (explicit)

**New:**
- `src/oidc.rs` — discovery, JWKS, auth URL, code exchange, ID-token validation, state/nonce codec.
- `src/api.rs` — REST handlers + shared vault-read logic (extracted from `mcp.rs` bodies).
- `migrations/0011_oidc.sql` — `ALTER TABLE users ADD COLUMN provider TEXT NOT NULL DEFAULT 'password' CHECK (provider IN ('password','oidc'))`. Keep `password_hash NOT NULL`: SSO users get a random bcrypt placeholder (unguessable ⇒ `verify_login` can never succeed for them) — avoids the SQLite table-rebuild needed to make the column nullable.
- `require_api_token` middleware (in `server.rs` or `api.rs`).
- `store.rs` new fns: `create_sso_user(email, role)`, `set_user_role` if absent, `link_or_create_oidc_user` (thin SQL, same file as `create_user` at :349).
- Cargo deps: `openidconnect` (or `oauth2`) + `reqwest` (rustls).

**Modified:**
- `src/config.rs` — `OidcSection` + `RuntimeConfig.oidc: Option<…>` + env-wins for client secret.
- `src/server.rs` — `build_router_with_email`: add `/auth/oidc/start` + `/auth/oidc/callback` to `public`, new `api` branch merged; `login_form`/`LOGIN_HTML` gains SSO button (only when `cfg.oidc.is_some()`); `webdav_entry` GET adds `ETag`, PUT adds `If-Match`/`If-None-Match` pre-check before `tokio::fs::write`; upload handler gains hash check.
- `src/webdav.rs` — `etag_of()` helper, optional `getetag` in `prop_entry`.
- `src/mcp.rs` — tool bodies delegate to extracted helpers (behavior identical).
- `src/store.rs` — SSO user fns (additive only; no schema touch beyond 0011).

**Untouched (by design):** `session.rs` (codec is provider-agnostic), `require_session`/`require_admin`/`require_dav_auth`, `api_tokens` schema, `groups`/`group_members` schema, `vault_changes`, vault pipeline/rebuild.

## 5. Suggested build order

1. **Phase A — ETag / conflict detection** (smallest, zero auth coupling, no new deps): `etag_of` helper → WebDAV GET/PROPFIND/PUT + upload check + tests. Independently shippable.
2. **Phase B — Read-only REST API** (no new deps; reuses `user_for_api_token` + MCP helpers): extract shared read logic from `mcp.rs` → `api.rs` handlers + `require_api_token` → mount `/api/v1`. Ships ETag headers on reads, so A feeds B.
3. **Phase C — OIDC SSO** (riskiest, only new deps, only migration): config `[oidc]` + `src/oidc.rs` + migration 0011 → start/callback routes + login button + auto-provision → group claim sync last (needs admin UI sanity for mapped groups).
   Rationale: A and B are independent of C and of each other except ETag reuse; C is the only change that can break login for existing users, so it lands last on a stable base. Within C: password-login regression tests first (coexistence is the acceptance bar), then provisioning, then group sync (additive-only).

**Watch-outs:** (1) `password_hash NOT NULL` — do the placeholder-hash trick, not a nullable rebuild. (2) Blank-env OIDC secret must fall back to file value (repeat the Phase 12 fail-open fix). (3) `search_md` cap of 50 hits is shared — expose a `limit` param at the REST layer, keep the MCP default. (4) SSO users with a locked account (`locked_accounts`) must still be refused mutations — the guard keys on email, so it works automatically; verify in tests. (5) `cargo install` static-binary constraint ⇒ rustls everywhere, no native-tls.
