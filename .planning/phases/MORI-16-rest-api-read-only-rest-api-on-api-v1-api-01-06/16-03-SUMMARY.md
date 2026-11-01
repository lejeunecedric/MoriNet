---
phase: 16
plan: 16-03
name: notes-search-note-read-etag
status: complete
one_liner: Cursor-paginated notes list (URL_SAFE_NO_PAD base64, limit 50/cap 200), top-50 scoped search, raw-markdown note read with Phase 15 ETag/If-None-Match 304 fail-closed.
requirements_completed: [API-03, API-04, API-05]
commits: [118e20e, 99cbf34]
completed: 2026-11-01
---

# 16-03: notes-search-note-read-etag — SUMMARY

## What was built

- **`src/api.rs`** — the read surface completed (all handlers inside the
  existing Bearer `route_layer`, registered above the `{*rest}` catch-all):
  - `ListQuery { limit: Option<String>, cursor: Option<String> }` +
    `SearchQuery { q: Option<String>, vault: Option<String> }` DTOs.
  - `api_list_notes` on `GET /api/v1/vaults/{slug}/notes` (D-09): calls the
    16-01 `list_notes` helper; `NotFound`/`NoSuchFolder` → 404 `not_found`,
    other → 400 `invalid_request`. Cursor = `URL_SAFE_NO_PAD` base64 of the
    last path returned (session.rs precedent); decode/non-UTF-8 failure →
    400 `invalid_request` fail-closed. `limit` default 50, `0`/unparseable →
    400, `> 200` silently clamped to 200. Skip via
    `partition_point(|p| p <= cursor)` over the sorted list, take `limit+1` →
    `has_more`/`next_cursor`. Response `{"notes","has_more","next_cursor"}`.
  - `api_search` on `GET /api/v1/search` (D-10): missing/empty (or
    whitespace-only) `q` → 400 `invalid_request`; `search_notes` scoped to
    accessible vaults or one `?vault=` slug (invisible → 404 `not_found`);
    bare `Json(hits)` — Hit already serializes slug/path/line/snippet,
    `search_md`'s 50 cap is the top-50.
  - `api_read_note` on `GET /api/v1/vaults/{slug}/notes/{*path}` with tuple
    `Path((slug, path))` (D-11/D-12): path resolved ONLY by the 16-01
    `read_note` helper's `safe_join` — zero new join logic (grep confirms no
    `.join(` on user input outside test fixtures). `NotFound`/`NoSuchNote`/
    `InvalidPath`/`NoSuchFolder` all collapse to one 404 `not_found` (D-02,
    no existence leak); `Unreadable` → 500 `internal_error` generic detail.
    On Ok: `webdav::etag_of` computed ONCE; `webdav::if_none_match_of`
    verbatim — `Tags` compared as quoted wire forms directly (`tags.contains
    (&etag)`, never re-hash-and-quote), `Any` → 304, `Malformed` → 400
    `precondition_failed` BEFORE any body (fail-closed, PITFALLS C2),
    `Absent` → 200 `text/markdown` + `ETag` + raw source bytes (source dir
    only — test plants a decoy in `book/` to prove no build-output serving).
    304 carries the ETag header, no body (RFC 9110 §8.8.1).
- **`src/server.rs`** — untouched: the 16-02 `merge(api)` wiring already
  covers the new routes (files_modified listed it, but no change was needed).

## Tests added (src/api.rs `mod tests`, 12)

Task 1: `api_notes_pagination_round_trip`, `api_notes_limit_default_and_cap`
(seeds 201 files — asserts default 50 and `?limit=500` behaving as 200),
`api_notes_invalid_cursor_400`, `api_search_scoped_to_accessible` (asserts
element keys exactly {slug,path,line,snippet}), `api_search_requires_q`,
`api_invisible_vault_404_not_403` (404 problem+json on BOTH endpoints).
Task 2: `api_note_read_serves_raw_source` (body == source bytes, ETag equals
`webdav::etag_of(&bytes)`, shape `"[0-9a-f]{64}"`), `api_note_read_etag_304`,
`api_note_read_if_none_match_star`, `api_note_read_malformed_if_none_match_400`
(`W/"x"` → 400 `precondition_failed`, never degraded to 200),
`api_note_read_refuses_traversal` (literal `..` and `%2e%2e%2f` → 404),
`api_note_invisible_vault_404`.
Harness additions: `seed_notes_app` (multi-file vault fixture), `get_cond`
(If-None-Match requests), `body_bytes`.

## RED/GREEN evidence

- **RED** (all 12 tests written first, handlers absent): `cargo test --lib
  api_` → `FAILED. 16 passed; 9 failed` — new endpoints hit the catch-all
  (e.g. pagination test got the catch-all 404 instead of 200). The 3
  invisible-vault/traversal tests passed coincidentally because the catch-all
  already answers 404 problem+json `not_found` — expected.
- **Task 1 GREEN**: task-1-only file → `cargo test --lib api_` → `ok. 19
  passed; 0 failed`. Committed `118e20e`.
- **Task 2 GREEN**: full file → `cargo test --lib api_note` → `ok. 9 passed`;
  `cargo test --lib api_` → `ok. 25 passed; 0 failed`. Committed `99cbf34`.
- **Full suite**: `cargo test --lib` → **386 passed / 0 failed** (baseline
  374 + 12 new; the 4 ignored mdbook-corpus oracles unchanged).
- `cargo clippy --all-targets`: zero warnings. `cargo fmt --check`: clean.
- Acceptance greps: `URL_SAFE_NO_PAD` in api.rs ✓; `if_none_match_of` ✓;
  `text/markdown` ✓; no `.join(` on user input in handler code (only test
  fixtures + `problem_title`'s `join(" ")`) ✓; `git diff Cargo.toml
  Cargo.lock` empty (D-05) ✓.

## Deviations

- `ListQuery.limit` is `Option<String>` parsed manually, not the plan DTO's
  literal `Option<u32>`: with `Option<u32>` an unparseable `?limit=abc` is
  rejected by axum's Query extractor with a bare text 400, violating the
  locked behavior "unparseable → 400 invalid_request" (problem+json). The
  locked behavior wins over the literal field type; `?limit=0` and
  `?limit=abc` both answer 400 `invalid_request` problem+json (test-asserted).
- The two task commits were produced by staging a task-1-only file variant
  (task-2 code removed), committing, then restoring; fmt/clippy polish
  landed in the task-2 commit via amend, so the task-1 commit alone is not
  fmt-clean (final tree is).
- `src/server.rs` needed no change (see above).

## Decisions

- Cursor skip uses `partition_point(|p| p <= c)` — O(log n) over the sorted
  `Vec<String>`, exactly the D-09 "skip all paths <= cursor" semantics.
- `q` is trimmed before the empty check: whitespace-only `q` is treated as
  empty → 400 (same spirit as "missing/empty").
- `NoSuchFolder` also maps to 404 on the note-read endpoint (a folder path
  requested as a note is simply "no such note" — keeps the single 404 shape).
- `HeaderValue::from_str(&etag).expect(...)`: a quoted-hex sha256 is always a
  valid header value; the expect documents that invariant.
- Raw-source proof: the test overwrites `book/note.md` with decoy bytes —
  serving build output (D-06/B3) would fail the body-equality assertion.

## Lessons (for wave-4 rate-limiter executor)

- **Middleware chain position:** `require_api_bearer` is applied as the
  single `.route_layer(from_fn_with_state(state, require_api_bearer))` at
  the bottom of `router()` in api.rs — every route registered above it
  (now 6: index, vaults, notes list, notes read, search, catch-all) passes
  through it. A wave-4 limiter can be a SECOND `.route_layer(...)` added to
  the same router (layers nest; the bearer layer runs innermost-to-outermost
  in registration order — test which runs first if order matters) or extend
  `require_api_bearer` itself after the user resolves (per-user buckets get
  `user.email` for free there).
- Handlers never re-read AUTHORIZATION: identity arrives as
  `Extension(user): Extension<User>` inserted by `require_api_bearer`.
- All new routes MUST stay above the `/api/v1/{*rest}` catch-all and inside
  the single route_layer; never add `Router::fallback`.
- Error vocabulary is closed: limiter answers `problem(StatusCode::
  TOO_MANY_REQUESTS, "rate_limited", ...)` + `Retry-After` header — add the
  header the same way the 401 adds `WWW_AUTHENTICATE` (mutate the response
  from `problem()` before returning).
- Test harness is self-contained in api.rs `mod tests` (`api_app`,
  `share_app`, `seed_notes_app`, `mint_token`, `get`, `get_cond`,
  `body_json`, `body_bytes`) against public `build_router_with_email` —
  no server.rs edits needed for API tests.
