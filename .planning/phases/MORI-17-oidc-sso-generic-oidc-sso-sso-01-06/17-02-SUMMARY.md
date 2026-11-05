# 17-02 SUMMARY — oidc-core-flow-mock-idp-and-login-button

**Status:** COMPLETE — all 4 tasks shipped, all final gates green.
**Branch:** main (not pushed, per orchestrator rule)
**Date:** 2026-11-06
**Requirements:** SSO-01..06 (core flow), D-01, D-02, D-14, D-15 groundwork

## Commits

- `0950443` feat(deps): add openidconnect (rustls-only) + rsa dev-dep for SSO core flow (D-01, SSO-01)
- `c08613b` test(oidc): in-test mock IdP harness (discovery/JWKS/token, PKCE gate) + oidc module (SSO-01)
- `a2c8c83` feat(oidc): authorization-code + PKCE S256 core flow with fail-closed callback (D-01, D-02, D-15, SSO-02..05)
- `643353b` feat(oidc): conditional SSO button on login page when [oidc] configured (SSO-06)

## What shipped

### Task 1 — dependencies (SSO-01) — `Cargo.toml` / `Cargo.lock`
- `openidconnect = { version = "4.0.1", default-features = false, features = ["reqwest", "rustls-tls"] }`
  (resolves to 4.1.0 via `^4.0.1`; PATTERNS §9 feature string honored exactly).
- `rsa = "0.9"` **dev-dependency only** (mock IdP RS256 signing).
- `tokio` gains the `net` feature (mock IdP `TcpListener`).
- Dep audit (PATTERNS §9): `grep -c 'name = "rustls"' Cargo.lock` = **1** (single 0.23.46),
  `native-tls` = **0**, `wiremock|mockito|httpmock` = **0** (no HTTP mocking crates added).

### Task 2 — in-test mock IdP (SSO-01) — `src/oidc.rs` (new module) + `src/lib.rs`
- `pub mod oidc;` added after `pub mod obsidian;` in `src/lib.rs`.
- `MockIdp`: plain-HTTP `tokio::net::TcpListener` serving
  `/.well-known/openid-configuration` (issuer derived from `Host` header),
  `/jwks` (fixed kid `test-key-1`), `POST /token` (Basic client auth + S256 PKCE
  verifier gate; ID token signed RS256 with `rsa` dev-dep).
- Committed test fixtures `src/testdata/sso_test_key.pem` / `sso_wrong_key.pem`
  (neutral fake keys, not real secrets); foreign-key rejection test uses the wrong key.
- Constants: `TEST_CLIENT_ID = "test-client"`, fixed test client secret (fixture value only).
- No mocking crates — raw listener + `http_get` helper.

### Task 3 — core flow handlers (SSO-02..05, D-01/D-02/D-15) — `src/oidc.rs`, `src/server.rs`, `src/store.rs`
- `sso_login`: discovery → `authorize_url` with `state` + `nonce` + PKCE S256;
  state trio (state, nonce, verifier) rides ONE short-lived signed cookie
  `morinet_sso_state` minted with the existing `SessionCodec`, TTL 600 s —
  no server-side state table. 302 Found with `Location` + `Set-Cookie`.
- `sso_callback`: constant-time `state` check (`subtle::ConstantTimeEq`), code
  exchange with verifier, ID-token verification via `openidconnect`
  (signature/JWKS, `iss`, `aud`, `exp`, `nonce`). **Access token is never
  consulted for identity** — only the signature-validated ID token feeds
  identity resolution (D-15 groundwork).
- Unknown `(iss, sub)` fails **closed**: `deny`/`not_linked` audit event via
  `log_auth_event`, 403 via `error_page` + `*r.status_mut()` override
  (error_page hardcodes 400). Linking/provisioning deferred to wave 3.
- Success path mints a session **byte-identical to `login_submit`** (D-02 —
  same HMAC cookie + attributes; verified by test against the same codec) and
  clears the state cookie. Two `SET_COOKIE` headers appended manually because
  axum array `IntoResponseParts` uses `insert` (second cookie would overwrite).
- Routes `/sso/login` + `/sso/callback` registered **unconditionally** after
  `/login`; handlers answer 404 (via `error_page` + `status_mut`) when `[oidc]`
  is absent, so existing gates are untouched.
- `src/store.rs`: `count_auth_events(kind, detail)` audit read surface for
  tests (no tokens/codes are ever stored).
- `src/server.rs`: `error_page`, `parse_cookies` widened to `pub(crate)`.
- `random_token()` returns 64 hex chars (two UUID simple strings) — valid PKCE
  verifier length 43..=128.

### Task 4 — login page SSO button (SSO-06, D-14 style) — `src/server.rs`, `src/oidc.rs`
- `login_html(&state)`: with no `[oidc]` returns `LOGIN_HTML` **byte-identical**
  (test asserts equality); with `[oidc]`, inserts
  `<p><a class="button" href="/sso/login">Se connecter via {escaped}</a></p><p>ou</p>`
  immediately before the password form; `sso_label` HTML-escaped (never trust
  config strings raw).
- `login_form` now `State<SharedState>` → `Html<String>`; unauthorized
  `login_submit` response renders via `login_html` too. `LOGIN_HTML` made
  `pub(crate)` for the byte-identity test.

## TDD evidence

- Task 2 RED: `cargo test --lib mock_idp` → `E0433: cannot find type MockIdp`.
  GREEN: `1 passed`.
- Task 3 RED: `cargo test --lib sso_` → compile failure (handlers/helpers absent).
  GREEN: `cargo test --lib sso_` → 17 oidc sso_* tests pass (plus the 4 wave-1
  `sso_reg_*` baseline tests matched by the same filter).
- Task 4 RED: `cargo test --lib login_page` → 3 tests fail (no SSO UI logic).
  GREEN: `3 passed`; `login_submit_failure` regression still green.

## Final gates (all green)

- `cargo test --lib`: **440 passed, 0 failed, 4 ignored** (sso_reg_* baseline UNEDITED).
- `cargo clippy --lib --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- Dep audit: rustls = 1, native-tls = 0, mocking crates = 0.

## Key decisions / deviations

- `openidconnect` version request `4.0.1` resolves to 4.1.0 (`^` semantics) —
  intended, feature string exactly as pinned by the plan.
- `rsa` appears in the normal dependency graph via `openidconnect` itself
  (intrinsic to its RS256 verification); **our own declaration is dev-only**,
  satisfying the "rsa dev-dep only" audit intent.
- Manual `StatusCode::FOUND` response for `sso_login` and manual `append` of the
  two `SET_COOKIE` headers on callback success — axum `Redirect` + array parts
  could not carry both cookies (array parts use `insert`).
- `exchange_and_verify` returns only the authenticated `sub` (not email) —
  local email resolution stays a store/caller concern (wave 3 linking input).
- No deviations from acceptance criteria; no pushes; `--no-verify` never used;
  `.planning/STATE.md` / `.planning/ROADMAP.md` untouched.

## Integration notes (wave 3)

- Identity linking/provisioning for unknown `(iss, sub)` plugs into the
  fail-closed branch in `sso_callback` (currently `deny`/`not_linked`).
- Groups claim / `group_map` mapping (D-09/D-10) consumes the same validated
  ID-token claims path; `OidcRuntime.groups_claim` / `group_map` already resolved
  by wave 1 config.
- `morinet_sso_state` cookie trio is stateless by design — no cleanup job needed.
