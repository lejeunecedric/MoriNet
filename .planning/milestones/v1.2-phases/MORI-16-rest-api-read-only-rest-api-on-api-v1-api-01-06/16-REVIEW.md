---
phase: 16
title: "REST API — Read-only REST API on /api/v1 — Code Review"
reviewed_range: 2eff9b1..HEAD (source only, .planning excluded)
reviewed_files: [src/api.rs, src/mcp.rs, src/server.rs, src/lib.rs]
date: 2026-11-01
status: clean-with-findings
counts:
  critical: 0
  high: 0
  medium: 2
  low: 4
  info: 5
verification:
  - "cargo test --lib api:: → 41 passed, 0 failed"
  - "cargo test --lib mcp:: → 12 passed, 0 failed (wrapper refactor preserves ACL matrix + error strings)"
  - "matchit 0.8.4 / axum 0.8 method-mismatch behavior reproduced in an isolated probe crate (see M-02)"
---

# Phase 16 Code Review

Scope: `src/api.rs` (new, 1755 lines), `src/mcp.rs` (wrapper refactor), `src/server.rs`
(wiring only: `api_rate` field, `app_page`/`unix_now` visibility, docs route, router merge),
`src/lib.rs` (`mod api`). Checked against D-01..D-15 and ROADMAP Avoids B1–B4.

**Bottom line: no CRITICAL or HIGH findings.** The four threat axes hold up under
inspection and the test suite:

- **B1 (ACL bypass):** every read path funnels through `accessible_vault_entries` →
  `source_dir` → `safe_join` (`api.rs:57,75,96,116`); invisible vaults yield
  `NotFound` → 404 problem+json on all four endpoints including `?vault=` scoped
  search (`api.rs:131-135,447-449`); traversal (raw and `%2e%2e%2f`) collapses to the
  same 404 (`api.rs:474-479`). The full MCP matrix is ported and green.
- **B2 (token leakage):** plaintext token is consumed into SHA-256 and dropped
  (`api.rs:262-263`); the limiter map key is the hash (`api.rs:198-203`); no
  `println!`/`tracing` anywhere in `api.rs`; problem+json details are fixed strings.
- **B3 (build output):** reads resolve via `webdav::source_dir_of` (source tree only)
  and the decoy test at `api.rs:1229-1253` proves build output is never served.
- **B4 (cookie/Bearer mixing):** the api router is a sibling of `protected` with its
  own `route_layer` (`api.rs:581-598`, `server.rs:284-294`); middleware reads only
  `AUTHORIZATION` (`api.rs:244-249`); cookie-only request → 401 test green.
- **Rate limiter:** std Mutex acquired and released entirely inside synchronous
  `api_rate_check` (`api.rs:211-231`) — never across an await; prune bounds the map to
  tokens active in the last window; Retry-After math verified by tests.
- **ETag/304:** quoted-form compare via `webdav::if_none_match_of`, Malformed → 400
  fail-closed before any body (`api.rs:491-504`).
- **Cursor:** base64url decode failure → 400, never silently treated as no-cursor
  (`api.rs:388-399`).
- **Docs page:** fully static, zero interpolation, public, no vault data
  (`api.rs:528-572`).

## MEDIUM

### M-01 — JSON index advertises the wrong search parameter (`slug` vs `vault`)
**Evidence:** `src/api.rs:305` — the `GET /api/v1` index says
`"Search note contents across your vaults (?q=, optional &slug=)"`, but the
implementation's query DTO accepts only `q` and `vault` (`src/api.rs:347-350`), and the
docs page correctly documents `&vault=slug` (`src/api.rs:540`).
**Impact:** a client that follows the machine-readable index sends `?slug=x`; serde
ignores unknown fields, so the request silently becomes an **unscoped global search**
instead of the requested vault-scoped one — wrong results with no error. This is the
one discoverability surface where the two sources of truth disagree.
**Fix:** change the index string at `api.rs:305` to `&vault=` (one-word fix), or accept
`slug` as an alias in `SearchQuery`. Add a test asserting the index text and
`SearchQuery` field agree.

### M-02 — Method-mismatch on exact API paths returns bare 405, not problem+json (D-03 gap)
**Evidence:** all API routes are registered `get(...)` (`src/api.rs:583-587`) while the
catch-all is `any(...)` (`api.rs:592`). In matchit 0.8.4 a static path match with a
method mismatch yields axum's built-in `405 Method Not Allowed` (bare status +
`Allow: GET,HEAD` header) instead of falling through to the `any` catch-all. Reproduced
empirically with the same axum 0.8 / matchit 0.8.4 versions:
`POST /api/v1/vaults → 405 (Allow: GET,HEAD)`, while `POST /api/v1/nope → 404
problem+json`. This contradicts D-03 ("every error is RFC 9457 problem+json") and the
closed-vocabulary claim in `problem()`'s doc comment (`api.rs:150-157`).
**Not a security issue:** the probe confirms the Bearer `route_layer` middleware still
runs for the 405 (no auth bypass; the request also counts against the rate limiter).
**Fix:** register the four GET endpoints with `any(...)` and answer non-GET with
`problem(StatusCode::METHOD_NOT_ALLOWED, "invalid_request", ...)` inside the handlers
(or a tiny shared method-guard), keeping the surface single-shaped. Alternatively,
document 405 as an accepted exception to the closed vocabulary.

## LOW

### L-01 — Rate limiter underflows on wall-clock rewind
**Evidence:** `src/api.rs:219` (`now - *start`) and `api.rs:227`
(`API_RATE_WINDOW_SECS - (now - *start)`) use plain subtraction on wall-clock seconds
(`crate::server::unix_now()`). If the system clock steps backward (NTP correction),
`now < *start` → debug-build panic (500/dropped connection) or release-mode wraparound
(huge value ≥ 60 → window silently resets, self-healing).
**Fix:** `now.saturating_sub(*start)` at both sites (and in the `retain` predicate if
start can be in the future). One-line hardening.

### L-02 — Authenticated content responses carry no `Cache-Control`/`Vary`
**Evidence:** note reads respond with only `Content-Type` + `ETag`
(`src/api.rs:509-519`); `/vaults`, notes list, and search responses set no caching
headers at all. RFC 9110 §7.6.1 forbids shared-cache reuse for
`Authorization`-bearing requests absent explicit directives, so protocol-correct
intermediaries are safe — but a misconfigured reverse proxy in a self-hosted deploy
could heuristically cache per-user content keyed only by URL.
**Fix:** add `Cache-Control: private, no-store` (or at minimum `Vary: Authorization`) to
the four data endpoints' success responses.

### L-03 — MCP `search_notes` wrapper issues a duplicate store query for logging
**Evidence:** `src/mcp.rs:223-246` — the global-search path calls
`crate::api::accessible_vault_entries` a second time (the helper already resolved the
same list inside `search_notes`, `api.rs:136-143`) purely to emit one `log_access` row
per vault. Semantics are preserved (same set, same rows — tests green), but every
global MCP search now pays the ACL query twice and can log a set that drifted from the
one actually searched.
**Fix:** have `api::search_notes` return the searched `(slug, dir)` pairs (or an
optional log-sink callback) so the wrapper logs from the same resolution.

### L-04 — `Bearer` scheme prefix match is case-sensitive
**Evidence:** `src/api.rs:248` (`strip_prefix("Bearer ")`) — `bearer <token>` (valid
per RFC 7235's case-insensitive auth-scheme) is rejected 401. Mirrors the existing
`/mcp` middleware (`src/server.rs:306`), so this is consistency with the codebase
rather than a regression — but both deviate from the RFC and will surprise clients.
**Fix (optional, cross-phase):** case-insensitive prefix check in both gates, or
document the strict form on the docs page.

## INFO

### I-01 — Unreachable error arms in `api_list_notes` / `api_search`
`src/api.rs:367-373` maps `Err(_)` → 400, but with `sub: None` the helper can only
produce `NotFound`/`NoSuchFolder` (both handled at `api.rs:364`); `api.rs:450-454`
similarly arms for a variant `search_notes` never returns. Dead code — harmless, but
`unreachable!()` or dropping the arms would keep the mapping honest.

### I-02 — Fixed-window boundary burst is inherent to D-13
A client can spend 120 at t=59 and another 120 at t=60 (window resets on first request
of the new window, `api.rs:219-222`) — up to 240 requests across a 2-second boundary.
This is exactly what D-13 chose ("fixed-window"); noting only so it isn't mistaken for
a sliding window later.

### I-03 — Unauthenticated flood is deliberately unthrottled
The limiter runs only after successful token resolution (`api.rs:256-258`); anonymous
floods cost one indexed `SELECT` on `api_tokens.token_hash` each (`store.rs:1595-1606`).
Documented as intentional in-code; fine for the threat model, worth revisiting if the
instance is ever public-facing.

### I-04 — Store failures degrade to `200 []` on `/vaults`
`accessible_vault_entries` swallows store errors via `unwrap_or_default()`
(`api.rs:57-59`), so a DB fault answers an empty vault list rather than 500. Inherited
verbatim from the MCP legacy bodies (D-04 parity requirement); flagging only because
REST clients get no signal that anything is wrong.

### I-05 — REST access logging intentionally absent
`api.rs:1501-1511` documents why the three MCP logging tests were not ported
(`access_log.source` CHECK-constrained to `('web','mcp')` in migration 0008). Gap is
deliberate and recorded — no action; remove this note when REST logging is ever scoped.

## Notes on the MCP wrapper refactor (D-07 / focus area h)

Error strings are byte-preserved via `ApiReadError::mcp_message()`
(`api.rs:44-52`) and the verbatim-string tests (`mcp_hidden_share_invisible`,
`mcp_revoked_share_denied`) pass. `read_note`'s switch from `read_to_string` to
`fs::read` + `String::from_utf8` (`mcp.rs:203-219`) maps non-UTF-8 to the same legacy
"could not read the note". `list_notes`/`search_notes` log ordering matches the
pre-refactor behavior (log only on success, one row per searched vault). Only the
duplicate-query nit (L-03) was found.

## Suggested fix order

1. M-01 (one-word string fix, prevents silently-wrong client behavior)
2. M-02 (method-guard or documented exception)
3. L-01 (saturating_sub), L-02 (Cache-Control)
4. L-03, L-04 at next touch of those files

---
*Reviewer: GSD code review, phase 16. Verified against 2eff9b1..HEAD; all cited line
numbers are post-phase HEAD.*
