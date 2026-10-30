# STACK Research — v1.2 Team Access

Research date: 2026-10-30. Versions verified live against crates.io on this date.
Scope: stack additions/changes for OIDC SSO, read-only REST API, and ETag conflict
detection. Everything already shipped (axum 0.8 server, sqlx/SQLite, cookie
session, API tokens, MCP, WebDAV, config.toml+env pattern) is assumed — not re-researched.

## Verdict

Three features, **two new dependency trees, one of them real**:

| Feature | New crates | Notes |
|---|---|---|
| OIDC SSO | `openidconnect` 4.0.1 (features `reqwest`, `rustls-tls`) | pulls `oauth2` 5.0.0 + `reqwest` 0.12 transitively — the only meaningful addition |
| Read-only REST API | none | axum `Json` + serde already in repo |
| ETag conflict detection | none | `sha2` + `hex` already in repo; header parsing is hand-rolled like the rest of `webdav.rs` |

No breaking upgrades required. `axum = "0.8"` (0.8.9 is current) stays. Do **not**
bump `tower-http` 0.6 → 0.7.1 in this milestone — breaking minor, no benefit here.

## 1. OIDC client library choice

### Recommended: `openidconnect = "4.0.1"` with `features = ["reqwest", "rustls-tls"]`

Verified 2026-10-30: `openidconnect` max stable **4.0.1** (2025-07-06); depends on
`oauth2 ^5.0.0` (current stable, unchanged since 2025-01-21 — the ecosystem settled).
Feature `rustls-tls` forwards to oauth2's reqwest with rustls — consistent with the
repo's existing rustls choice (`lettre` uses `tokio1-rustls-tls`) and keeps the
`cargo install` story clean (git2 already vendors OpenSSL; the OIDC tree stays pure-Rust
crypto: `rsa`, `p256`, `p384`, `ed25519-dalek` for ID-token signature verification).

Why the full library over the alternatives:

- **`oauth2` 5.0.0 alone** — handles discovery, code flow, PKCE, token exchange, but
  does **not** validate ID tokens (no JWKS fetch/cache, no nonce check). You would add
  `jsonwebtoken` 11.1.0 + hand-rolled JWKS caching and claim validation anyway — that is
  re-implementing the hard 30% of `openidconnect` with more security risk.
- **Manual reqwest+serde code flow** — tempting for dep weight, but discovery document
  parsing, JWKS rotation/caching, `state`/`nonce` binding, and claim verification are
  exactly where home-grown OIDC implementations leak vulnerabilities. Not worth it for
  a security boundary.
- `openidconnect` 4.x supports everything the target IdPs (Keycloak / Authentik /
  Authelia) emit: OIDC discovery, PKCE, RS/ES/EdDSA-signed ID tokens, custom claims
  (needed for the groups claim).

Caveat to record: `oauth2` 5.0.0 pins `reqwest ^0.12` while reqwest 0.13.5 is current —
so the pinned transitive reqwest is 0.12.x, not 0.13. Harmless (nothing else in the
tree needs reqwest), just don't add a direct `reqwest = "0.13"` dep or you'll compile
two copies.

### Session integration (no new crates)

The existing session is a **stateless signed cookie** (`session.rs::SessionCodec`:
`b64url(email).expiry.hmac_sha256`), verified by middleware in `server.rs` (~line 418+).
OIDC fits without touching it:

1. `GET /auth/oidc/login` → build auth URL via `openidconnect`, put `state`+`nonce`+PKCE
   verifier in a short-lived HttpOnly cookie (SessionCodec can sign it; no server store needed).
2. `GET /auth/oidc/callback` → exchange code, validate ID token, extract
   `email`/`email_verified`/groups claim → auto-provision or link user in `store.rs`
   (email match or allowed-domain rule) → **issue the same session cookie the password
   login already sets** (same `Set-Cookie` code path, `HttpOnly + HMAC`, `cfg.cookie_name`).
3. All existing middleware (`require_session`, admin gate) works unchanged — OIDC users
   are ordinary rows in the users table with a new nullable `oidc_subject` column.

Group claim → MoriNet groups: a `store.rs` upsert at each SSO login mapping the
configured claim (e.g. `groups`) to the existing `groups` tables (v0.1.9 group system).

### Config additions (existing pattern, no new crate)

New `[oidc]` section in `config.toml` + env-var overrides (env wins), same resolution
code shape as `config.rs::RuntimeConfig`: `issuer`, `client_id`, `client_secret`,
`redirect_uri`, `allowed_domains`, `groups_claim`, group/role mapping, `enabled` flag.
Disabled by default → zero behavior change for existing deployments.

## 2. Read-only REST API — no new crates

- axum 0.8 `Json` + `serde` derive are already in the repo. Current JSON usage is
  ad-hoc `serde_json::json!` macros (server.rs:275, 1678) and `#[derive(Deserialize)]`
  for forms. For a **public** API, define proper `#[derive(Serialize)]` DTO structs in
  a new `src/api.rs` (stable field names, `#[serde(rename_all = "camelCase")]` or snake_case —
  pick one and document it) instead of `json!` macros.
- Auth: reuse the exact Bearer pattern from the MCP endpoint (`server.rs` ~257:
  strip `Bearer `, `store.user_for_api_token`, 401 + `WWW-Authenticate: Bearer`).
  Same tokens created at `/account` — no new auth surface.
- ACL scoping: reuse the vault-access logic `mcp.rs` already implements (list/read/search
  bound to the token's user); `mcp.rs::search_md` is `pub(crate)` and directly reusable
  for `/api/v1/search`.
- Mount under `/api/v1/` in `server.rs` router: `GET /api/v1/vaults`,
  `GET /api/v1/vaults/{slug}/notes`, `GET /api/v1/vaults/{slug}/notes/{path}`,
  `GET /api/v1/search`. Read-only per milestone scope (management stays WebDAV + admin UI).
- `axum-extra` (0.12.6) is **not** needed: its useful extractors (TypedHeader etc.)
  don't earn a new crate here.

## 3. ETag / conflict detection — no new crates

- Hash utility: `sha2` 0.10 + `hex` 0.4 are already direct deps (used for API-token and
  session hashing). A shared helper `fn etag_of(bytes) -> "\"<sha256hex>\""` (strong ETag)
  — put it next to the existing sha256 helpers in `server.rs`/`webdav.rs`.
  For large files an `mtime+size` weak ETag is cheaper, but vault markdown is small —
  content hash is simpler and correct across Refresh/rebuild.
- WebDAV (`webdav.rs`): headers are already parsed manually from `HeaderMap` (PROPFIND is
  a custom method the file handles by hand — same approach for `If-Match` / `If-None-Match`;
  no crate parses these for you). Changes:
  - `GET`/`PROPFIND` responses gain `ETag:` (+ keep `getlastmodified`).
  - `PUT` with `If-Match: <etag>` → re-hash target file, mismatch → **412 Precondition Failed**.
  - `PUT` create with `If-None-Match: *` → **412** if file exists.
  - No `If-Match` sent → current behavior (last-write-wins) so existing clients don't break.
- Upload-time check (local vault upload, Phase 14 pipeline): multipart handler already
  hashes content for the changes log; compare a client-supplied base hash and return
  **409 Conflict** on mismatch. Same helper, no new crate.
- `httpdate` 1.0.3 exists but `webdav.rs::http_date` is already hand-rolled — leave it.
- axum-extra 0.12 has **no** etag middleware (feature list verified) — nothing to adopt there.

## Dependency diff (Cargo.toml)

```toml
# ADD
openidconnect = { version = "4.0.1", default-features = false, features = ["reqwest", "rustls-tls"] }

# UNCHANGED (verified current or compatible)
axum = "0.8"          # 0.8.9 current
tower-http = "0.6"    # 0.7.1 exists — do NOT bump this milestone
sha2 = "0.10"         # reused for ETags
serde / serde_json    # reused for REST DTOs
```

Rejected/deferred: `oauth2` alone (no ID-token validation), manual reqwest OIDC
(security risk), `jsonwebtoken` 11.1.0 (only needed if going manual), `axum-extra`
0.12.6 (no etag feature, extractors not needed), `tower-http` 0.7 upgrade (breaking,
no benefit), `httpdate` (already hand-rolled).

## Pitfalls to carry into planning

1. `openidconnect` 4.0.1 was released 2025-07 — mature but slow-moving; the `oauth2`
   5.0.0 pin means reqwest stays 0.12.x. Fine today; revisit if a CVE lands.
2. Auto-provisioning must require `email_verified = true` from the IdP, or an attacker
   with an unverified email at an allowed domain takes over an account.
3. Group-claim mapping must never *demote* local admins implicitly — decide sync
   semantics (additive vs authoritative) in the phase plan.
4. ETag must be computed on the **source** markdown (WebDAV operates on the vault
   source, not the mdBook build output) or If-Match breaks after every Refresh.
5. The REST API reads served/built content vs source — pick one deliberately
   (MCP reads source dirs today; mirror that for consistency).
