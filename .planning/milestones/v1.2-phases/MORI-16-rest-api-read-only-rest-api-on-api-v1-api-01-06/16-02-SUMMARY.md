---
phase: 16
plan: 16-02
name: router-bearer-problem-json
status: complete
one_liner: /api/v1 standalone Bearer-gated router merged as sibling of mcp, RFC 9457 problem+json primitive, JSON index + GET /vaults.
requirements_completed: [API-01, API-02, API-05]
commits: [4ceb6af, 861b439]
completed: 2026-11-01
---

# 16-02: router-bearer-problem-json — SUMMARY

## What was built

- **`src/api.rs`** — the `/api/v1` surface, on top of the 16-01 helpers:
  - `pub(crate) fn problem(status, code, detail) -> Response` — the ONE RFC 9457
    error constructor for the whole surface: `CONTENT_TYPE: application/problem+json`,
    body `{type:"about:blank", title, status, detail, code}`. `title` is derived
    from the code slug by `problem_title` (`not_found` → "Not Found"). Closed
    vocabulary enforced by convention: `unauthorized|not_found|invalid_request|precondition_failed|rate_limited|internal_error` (D-03). Fixed detail strings, zero logging (B2).
  - `async fn require_api_bearer(State<SharedState>, Request, Next) -> Response` —
    copies the `mcp_entry` bearer-strip chain verbatim (AUTHORIZATION →
    `strip_prefix("Bearer ")` → trim), resolves via `store.user_for_api_token`,
    on None returns `problem(UNAUTHORIZED, "unauthorized", …)` **plus**
    `WWW-Authenticate: Bearer`; on Some inserts the `User` clone into request
    extensions (the `require_dav_auth` Claims-insert pattern). Only AUTHORIZATION
    is read — cookies are structurally never consulted (D-01 / Avoids B4).
  - `pub fn router(state: &SharedState) -> Router` — routes
    `GET /api/v1` (`api_index`), `GET /api/v1/vaults` (`api_vaults`), then the
    catch-all `.route("/api/v1/{*rest}", any(api_not_found))` — all registered
    BEFORE the single `.route_layer(from_fn_with_state(state, require_api_bearer))`
    so the catch-all is Bearer-gated (401 without token, 404 problem+json with).
    NO `Router::fallback` anywhere (would leak into the whole-app 404 path).
  - `api_index` — static JSON index (D-14): all five endpoint paths with
    one-line descriptions + `"auth"` hint naming Bearer + /account; docs entry
    notes it is public (16-05).
  - `api_vaults` — `accessible_vault_entries` → `vault_json` → **bare**
    `Json(Vec<Value>)`, exactly slug/name/scope/role, no envelope (API-02, D-08);
    `[]` for a user with no grants (200, never 403/404).
  - `api_not_found` — `problem(NOT_FOUND, "not_found", "no such API endpoint")`.
- **`src/server.rs`** — in `build_router_with_email` (~:273): `let api =
  crate::api::router(&state);` and `.merge(api)` appended to the merge chain
  (:280) as a **sibling of `.merge(mcp)`**, never inside `protected`
  (D-01/B4). `test_router` delegates to `build_router_with_email`, so all
  router tests reach the api group with no extra wiring.

## Tests added (src/api.rs `mod tests`)

Task 1: `api_requires_bearer`, `api_rejects_forged_and_revoked_tokens`,
`api_ignores_session_cookie`, `api_unknown_path_is_problem_not_found`.
Task 2: `api_index_lists_endpoints`, `api_vaults_matches_mcp_fields`,
`api_vaults_empty_for_stranger`.
Harness: `router_cfg`, `api_app`, `share_app` (mcp.rs share_fixture pattern),
`mint_token`, `get`, `body_json`.

## RED/GREEN evidence

- **Task 1 RED** (tests first, router absent): `cargo test --lib api_` →
  `FAILED. 6 passed; 4 failed` — e.g. `assertion left == right failed: no Bearer → 401, left: 404, right: 401`
  (bare axum 404, no problem+json).
- **Task 1 GREEN**: `cargo test --lib api_` → `ok. 10 passed; 0 failed`
  (4 new + 6 pre-existing `api_token_*` green throughout).
- **Task 2 RED**: `cargo test --lib api_index` → `0 passed; 1 failed`;
  `cargo test --lib api_vaults` → `0 passed; 2 failed` (catch-all 404).
- **Task 2 GREEN**: `cargo test --lib api_` → `ok. 13 passed; 0 failed`.
- **Full suite**: `cargo test --lib` → **374 passed / 0 failed** (baseline
  367 + 7 new; the 4 ignored mdbook-corpus oracles unchanged).
- `cargo clippy --all-targets`: zero warnings. `cargo fmt --check`: clean.
- Acceptance greps: `application/problem+json` in api.rs ✓; `merge(api)` at
  server.rs:280 ✓; api group is a sibling merge, not under `require_session`'s
  route_layer ✓; `grep -rn "tracing\|log::" src/api.rs` → 0 (B2) ✓; no
  `Router::fallback` call anywhere (only comment mentions) ✓;
  `git diff Cargo.toml Cargo.lock` empty (D-05) ✓.

## Deviations

- The plan said tests should use "the server.rs test_router + req +
  create_api_token patterns" — those helpers are private to server.rs's test
  module, so the patterns were **replicated** in api.rs's `mod tests`
  (`api_app`/`get`/`mint_token`) against the public `build_router_with_email`.
  Same oneshot path as curl, per the plan's verification note.
- None otherwise; all must_have truths and acceptance criteria met as written.

## Decisions

- Identity hand-off: middleware inserts the resolved `store::User` clone into
  extensions; `api_vaults` extracts `Extension<User>` + `State<SharedState>` —
  no token re-look-up in handlers.
- `problem_title` derives the RFC 9457 title from the code slug instead of a
  second table, so the closed vocabulary has one source.
- Task 1's "live token → 200" assertion was written against the catch-all
  (404 = gate passed) and tightened to 200 in task 2 when the endpoint landed —
  final state matches the plan.
- The index marks `/api/v1/docs` as public (16-05 serves it outside the group).

## Lessons (for wave-3/4 executors)

- **Where routes get appended:** inside `pub fn router(state: &SharedState)` in
  `src/api.rs` — add `.route(...)` lines AFTER `api_vaults` and BEFORE the
  `.route("/api/v1/{*rest}", any(api_not_found))` catch-all; everything must
  stay above the single `.route_layer(axum::middleware::from_fn_with_state(
  state.clone(), require_api_bearer))` (currently api.rs ~:272-286). Static and
  param routes win over `{*rest}` in matchit; the router already carries
  `.with_state(state.clone())` so handlers can extract `State<SharedState>`.
- Handlers get the caller via `Extension(user): Extension<User>` (inserted by
  `require_api_bearer`) — never re-read the AUTHORIZATION header.
- Every error path must call `crate::api::problem(status, code, detail)` with
  the closed code set. Wave-3 mapping: `ApiReadError::NotFound` → 404
  `not_found`; `InvalidPath` → 400 `invalid_request`; `NoSuchFolder`/
  `NoSuchNote` → 404 `not_found`-shaped per plan; malformed If-None-Match →
  400 `precondition_failed`. Wave-4 limiter → 429 `rate_limited` +
  `Retry-After` (slot in as a second layer inside `router()` or extend
  `require_api_bearer`).
- server.rs wiring is done once: `build_router_with_email` merges the api group
  and `test_router` delegates to it — wave-3/4 tests just use the api.rs test
  harness (`api_app`, `share_app`, `mint_token`, `get`, `body_json`).
- Do NOT add `Router::fallback` anywhere in the app (site-wide 404 shape).
