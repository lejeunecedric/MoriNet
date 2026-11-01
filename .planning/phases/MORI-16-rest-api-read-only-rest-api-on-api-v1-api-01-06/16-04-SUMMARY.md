---
phase: 16
plan: 16-04
name: per-token-rate-limiter
status: complete
one_liner: Per-token fixed-window rate limiter — 120 req/60s keyed by SHA-256 of the bearer token, enforced in require_api_bearer with 429 problem+json + Retry-After.
requirements_completed: [API-06]
commits: [2d5af67, c202162]
completed: 2026-11-01
---

# 16-04: per-token-rate-limiter — SUMMARY

## What was built

- **`src/server.rs`** — `AppState` gained
  `pub api_rate: std::sync::Mutex<std::collections::HashMap<String, (u32, u64)>>`
  (`(count, window_start_unix_secs)`, keyed by the lowercase hex SHA-256 of the
  plaintext token — never the plaintext, B2), initialized empty at **all four**
  `AppState { … }` construction sites. Same `vault_locks` contract: the std
  Mutex guards only the map and is never held across an `await`.
  `unix_now()` (`:126`) made `pub(crate)` so `api.rs` can stamp windows.
- **`src/api.rs`** —
  - `const API_RATE_LIMIT: u32 = 120`, `const API_RATE_WINDOW_SECS: u64 = 60`
    (D-13).
  - `token_key(plain: &str) -> String` — `sha2::Sha256` + `hex::encode`, same
    idiom as `server::token_hash`; the plaintext is consumed into the hash and
    `drop`ped by the caller, never stored or logged (B2).
  - `api_rate_check(map: &Mutex<HashMap<String,(u32,u64)>>, key: &str, now: u64)
    -> Result<(), u64>` — fully synchronous with injected `now` (testable
    clock): prunes expired windows (`retain(|_, (_, start)| *start + 60 >= now)`),
    get-or-insert, resets the window when `now - start >= 60`, increments;
    `Ok(())` at ≤120, `Err(retry_after)` over, where
    `retry_after = 60 - (now - start)` ∈ 1..=60.
  - Enforcement inside `require_api_bearer` **after** successful token
    resolution and **before** the handler — unauthenticated floods cost one
    indexed query and never touch the map; every endpoint counts equally
    against the same per-token budget (decision 6a). Over-limit answers
    `429 TOO_MANY_REQUESTS` via the existing `problem()` helper with
    `code: "rate_limited"` (`application/problem+json`, D-03) and
    `Retry-After: <secs>` mutated onto the response, mirroring how the 401
    mutates `WWW-Authenticate`. The 401 path was factored into an
    `api_unauthorized()` helper (behavior unchanged).

## Tests (TDD)

Unit (no HTTP, injected clock): `rate_limiter_allows_120_blocks_121`,
`rate_limiter_window_resets_after_60s`, `rate_limiter_retry_after_counts_down`,
`rate_limiter_tokens_independent`, `rate_limiter_prunes_expired_windows`,
`rate_limiter_key_is_token_hash_not_plaintext`.
HTTP (replicated `api_app`/`mint_token`/`get` harness):
`api_rate_limit_returns_429_with_retry_after` (120×200 then 429,
problem+json, `Retry-After` ∈ 1..=60, `code == "rate_limited"`),
`api_rate_limit_is_per_token` (exhausted token A stays 429 while fresh
token B answers 200), `api_rate_limit_counts_all_endpoints` (121 requests
spread across vaults/notes/search/index/catch-all → last is 429).

- **RED**: Task 1 — tests first failed to compile (`token_key` /
  `api_rate_check` did not exist yet). Task 2 — the 3 HTTP tests failed before
  the middleware enforcement existed (the 121st request answered 200 OK where
  the tests assert 429).
- **GREEN**: `cargo test --lib rate_limiter` → 6 passed;
  `cargo test --lib api_rate_limit` → 3 passed;
  `cargo test --lib api_` → 28 passed (no existing test exceeds 120 requests
  on one token; fresh token per test = fresh bucket).
- **Full suite**: `cargo test --lib` → **395 passed / 0 failed / 4 ignored**
  (baseline 386 + 9 new). `cargo clippy --all-targets` clean,
  `cargo fmt --check` clean.

## Deviations

- None from the plan contract. One mechanical necessity: `server::unix_now`
  was private; made `pub(crate)` (the plan anchors to it at `:126`).

## Decisions

- Enforcement lives **inside `require_api_bearer`** (not a second
  `.route_layer`): the plan locks keying by SHA-256 of the *token*, and the
  middleware is the one place that already holds the plaintext before hashing
  — a second layer would re-parse the header for nothing. The plan explicitly
  allowed either shape.
- Prune-on-every-check (O(map) per request) instead of a background sweeper:
  map size is bounded by distinct active tokens per minute; no new deps (D-05),
  no task spawning.
- `retry_after = window_end - now` (never 0): a client told `Retry-After: 0`
  would hammer; the fixed window guarantees ≥1s remains whenever a request is
  rejected mid-window… at exactly `now == window_end` the window resets first,
  so the value is always in 1..=60.

## Lessons (for the wave-5 docs-page executor)

- **Public docs page registration point**: anything added inside
  `api::router()` (`src/api.rs:530`) sits **inside** the single
  `.route_layer(require_api_bearer)` — it would require a Bearer token *and*
  count against the 120/min budget. The uncounted public docs page must be
  registered on the **top-level router in `server.rs`** where
  `crate::api::router(&state)` is merged (`src/server.rs:281`), as its own
  public route outside that group.
- **Catch-all shadowing**: `/api/v1/{*rest}` (`any(api_not_found)`) is
  registered before `route_layer` so unknown API paths get 401-then-404
  problem+json. In matchit, static/param routes win over `{*rest}` regardless
  of registration order, but keep new API routes registered above the
  catch-all for consistency with waves 2–4. **Never** use
  `Router::fallback` — a merged router's fallback becomes the whole-app
  fallback and would change site-wide non-API 404s.
- Rate limiter is per **token**, not per user: two tokens of the same user
  have independent buckets. A docs page that mints or probes tokens in tests
  gets a fresh bucket per token — existing tests are unaffected.
