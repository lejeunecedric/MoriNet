---
phase: 16
name: rest-api
status: passed
score: 7/7
verified: 2026-11-01
verifier: gsd-phase-verifier
commit_under_test: 374a987
---

# Phase 16 Verification — Read-only REST API on `/api/v1`

**Status: PASSED (7/7)**

All seven ROADMAP success criteria verified against the actual implementation
(`src/api.rs`, `src/mcp.rs`, `src/server.rs`, `src/webdav.rs`) by targeted test
runs and code reads. Orchestrator's full-suite baseline at HEAD: 403 passed /
0 failed / 4 ignored, `check.sh` OK. Targeted re-runs below all green.

## Test commands run

| Command | Result |
| --- | --- |
| `cargo test --lib api_` | 36 passed / 0 failed (all `api::tests` + related store/server token tests) |
| `cargo test --lib tools_are_acl_scoped` | 2 passed (MCP original + REST port) |
| `cargo test --lib mcp_` | 12 passed / 0 failed (MCP ACL suite incl. hidden/revoked share) |
| `cargo test --lib etag_of` | 1 passed (`webdav::etag_of_is_strong_sha256`) |
| `grep tracing/info!/warn!/println src/api.rs` | zero hits — B2 no-token-logging gate clean |

## Criterion 1 — Bearer-only via `user_for_api_token`; session cookies rejected 401 — API-01 ✅

- **Code**: `require_api_bearer` (`src/api.rs:239-276`) reads ONLY the
  `AUTHORIZATION` header, strips `Bearer `, resolves via
  `state.store.user_for_api_token` (`src/store.rs:1595`). Cookies are
  structurally never consulted. The `/api/v1` group is a standalone router
  with its own `route_layer` (`src/api.rs:581-598`), merged as a SIBLING of
  `protected` in `build_router_with_email` (`src/server.rs:287`) — never
  inside the `require_session` group (D-01/B4).
- **Tests**: `api_requires_bearer` (401 + `WWW-Authenticate: Bearer` +
  problem+json), `api_ignores_session_cookie` (cookie alone → 401; Bearer +
  unrelated cookie → 200 on the Bearer alone), `api_rejects_forged_and_revoked_tokens`
  (forged → 401; live → 200; revoked → 401).

## Criterion 2 — `GET /api/v1/vaults` = exactly the accessible vaults — API-02 ✅

- **Code**: `api_vaults` (`src/api.rs:316-326`) maps
  `accessible_vault_entries` (personal + shares, same store query as MCP)
  through `vault_json` — bare array, exact MCP field set slug/name/scope/role.
- **Tests**: `api_vaults_matches_mcp_fields` (bare array, keys exactly
  `["name","role","scope","slug"]`, role verbatim), `api_vaults_empty_for_stranger`
  (200 + `[]`, never 403), `api_sharee_sees_shared_vault` (reader→`read`,
  writer→`write`), `helper_vault_json_field_names`.

## Criterion 3 — notes cursor pagination + raw markdown source + Phase 15 ETag + path canonicalization — API-03 ✅

- **Cursor pagination**: `api_list_notes` (`src/api.rs:356-427`) — sorted
  list, URL_SAFE_NO_PAD base64 cursor, `has_more` via limit+1 lookahead,
  default 50 / silent cap 200, invalid cursor fails closed 400. Tests:
  `api_notes_pagination_round_trip` (union of pages == full list, no dupes),
  `api_notes_limit_default_and_cap`, `api_notes_invalid_cursor_400`.
- **Raw source, not build output**: `read_note` resolves via
  `webdav::source_dir_of` (`src/webdav.rs:73` — maps `…/.morinet-build/book`
  back to the source tree). Test `api_note_read_serves_raw_source` writes a
  source-only token plus a `book/note.md` decoy reading "BUILD OUTPUT LEAK"
  and asserts the body equals the raw source bytes (B3 gate).
- **Phase 15 ETag**: `api_read_note` (`src/api.rs:464-520`) sets
  `ETAG = webdav::etag_of(&bytes)` (strong quoted sha256 —
  `etag_of_is_strong_sha256` green); If-None-Match parsed only by
  `webdav::if_none_match_of`, Malformed fails closed 400 before any body.
  Tests: `api_note_read_etag_304` (304 carries ETag, no body),
  `api_note_read_if_none_match_star`, `api_note_read_malformed_if_none_match_400`.
- **Path canonicalization**: exclusively `mcp::safe_join` (`src/mcp.rs:20-34`)
  — absolute paths and `Component::ParentDir` refused, never trimmed. Tests:
  `api_note_read_refuses_traversal` and
  `api_read_note_refuses_traversal_even_with_access` (raw `../` and
  percent-encoded `%2e%2e%2f` both → 404 against a real outside file).

## Criterion 4 — search scoped to accessible vaults — API-04 ✅

- **Code**: `search_notes` (`src/api.rs:125-146`) iterates ONLY
  `accessible_vault_entries` (or one slug gated through `source_dir`),
  reuses MCP `search_md` with the same 50-hit cap.
- **Tests**: `api_search_scoped_to_accessible` (sharee hits, stranger gets
  `[]`, invisible `?vault=` → 404), `api_search_finds_notes_in_shared_vault`,
  `api_search_requires_q` (missing/empty `q` → 400 problem+json),
  `helper_search_scoped_to_accessible`.

## Criterion 5 — 404-not-403 everywhere + RFC 9457 problem+json — API-05 ✅

- **Code**: `ApiReadError::NotFound` deliberately covers unknown slug AND
  no-access (D-02); every handler maps it to 404 `not_found`. The single
  `problem()` constructor (`src/api.rs:158-171`) emits
  `application/problem+json` with a CLOSED code vocabulary
  (`unauthorized`/`not_found`/`invalid_request`/`precondition_failed`/
  `rate_limited`/`internal_error`); the `ANY /api/v1/{*rest}` catch-all sits
  INSIDE the Bearer group so unknown paths are 401 without token, 404
  problem+json with one.
- **Tests**: `api_invisible_vault_404_not_403` (notes list + scoped search),
  `api_note_invisible_vault_404`, `api_hidden_share_invisible`,
  `api_revoked_share_denied` (all three read endpoints 404 after revoke),
  `api_unknown_path_is_problem_not_found`, `api_tools_are_acl_scoped`.
  Every failing-path test asserts `CONTENT_TYPE == application/problem+json`.

## Criterion 6 — per-token 429 + `Retry-After` — API-06 ✅

- **Code**: fixed-window limiter in `require_api_bearer` (`src/api.rs:187-231`,
  120 req / 60 s per token), keyed by SHA-256 hash of the token — plaintext
  consumed into the hash and dropped (B2); enforced after token resolution,
  before the handler, so all endpoints count equally; `Retry-After` set to
  remaining window seconds.
- **Tests**: `api_rate_limit_returns_429_with_retry_after` (121st → 429,
  problem+json `rate_limited`, Retry-After ∈ 1..=60),
  `api_rate_limit_is_per_token`, `api_rate_limit_counts_all_endpoints`, plus
  unit tests `rate_limiter_allows_120_blocks_121`,
  `rate_limiter_window_resets_after_60s`, `rate_limiter_retry_after_counts_down`,
  `rate_limiter_tokens_independent`, `rate_limiter_prunes_expired_windows`,
  `rate_limiter_key_is_token_hash_not_plaintext`.

## Criterion 7 — MCP ACL matrix ported, green on all four endpoints — API-05 ✅

- **Code**: ACL single-sourced in `src/api.rs` free helpers; `mcp.rs` tools
  are thin wrappers calling `crate::api::{accessible_vault_entries,
  list_notes, read_note, search_notes}` (`src/mcp.rs:172-236`), so the two
  transports cannot drift.
- **Tests**: six 1:1 ports in `api::tests` — `api_tools_are_acl_scoped`
  (port of `tools_are_acl_scoped`), `api_read_note_refuses_traversal_even_with_access`,
  `api_sharee_sees_shared_vault`, `api_hidden_share_invisible`,
  `api_revoked_share_denied`, `api_search_finds_notes_in_shared_vault` —
  all green alongside the six MCP originals (`cargo test --lib mcp_`: 12 passed).
  The three MCP LOGGING tests are deliberately NOT ported (documented at
  `src/api.rs:1501-1511`: `access_log.source` CHECK-constrains to
  `('web','mcp')`; REST access logging intentionally out of scope for v1).

## Notes / minor observations (non-blocking)

- Wrong-method requests (e.g. `POST /api/v1/vaults`) answer axum's bare
  405 Method Not Allowed rather than problem+json. The closed code vocabulary
  in the plan never includes 405 and no criterion or test covers it; the API
  is GET-only by design. Cosmetic at most; not a criterion violation.
- `GET /api/v1/docs` is intentionally public static HTML (D-15) — zero vault
  data, byte-identical responses (`api_docs_page_no_vault_data`).

## Human verification

None required — every criterion is covered by automated tests and code reads.
