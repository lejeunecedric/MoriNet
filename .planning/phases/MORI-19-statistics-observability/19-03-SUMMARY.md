---
phase: MORI-19-statistics-observability
plan: 19-03
one_liner: "REST API instrumented: log_api_access (spawn best-effort, source='api') on every successful api_read_note (raw AND ?meta=1, after the 404-collapse) + log_api_search fan-out writing one access_log row (path '-') and one raw search_queries row per searched vault — the Phase 16 'DELIBERATELY not ported' logging gap is closed and its three tests ported."
requirements_completed: [STAT-01, STAT-04, STAT-05]
subsystem: api
tags: rest-api, access-log, search-queries, best-effort, spawn, source-api
requires:
  - phase: 19-01
    provides: access_log CHECK widened to web|mcp|api (migration 0014) + Store::log_search_query (source 'api' accepted)
  - phase: 19-02
    provides: the mirrored fan-out pattern (accessible_vault_entries + webdav::source_dir_of) and the spawn-and-drop logging contract
provides:
  - api.rs log_api_access + log_api_search helpers (spawn + `let _ =`, never fail the request)
  - access_log rows source='api' for note reads (path = note path) and searches (path '-')
  - search_queries rows (raw q, source='api'), one per searched vault incl. global-search fan-out
  - six new api::tests logging pins incl. the three ports named in the retired Phase 16 comment
affects: [19-04-statistics-page]

key-files:
  created: []
  modified: [src/api.rs]

key-decisions:
  - "log call sites sit strictly AFTER the ACL gates (api_read_note read_note Ok-collapse; api_search Ok(hits) arm) — every problem+json early return writes nothing (T-19-07, no probing trail)"
  - "?meta=1 reads log the same row as raw reads (they read the note; logged before the etag/meta branching so both shapes count once)"
  - "global-search fan-out iterates the caller's OWN accessible_vault_entries filtered by webdav::source_dir_of — identical vault set to the search itself and to the mcp.rs:424-443 MCP fan-out"

requirements-completed: [STAT-01, STAT-04, STAT-05]

duration: ~25min
completed: 2026-11-18
---

# Phase 19 Plan 19-03: REST API access + search logging Summary

**REST API instrumented: `log_api_access` (spawn best-effort, source='api') fires on every successful `api_read_note` (raw AND `?meta=1`), and `log_api_search` fans out one `access_log` row (path `-`) + one raw `search_queries` row per searched vault — closing the deliberate Phase 16 gap and retiring its stale comment.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2 (both TDD)
- **Files modified:** 1 (src/api.rs)

## What was built

**Task 1 (`6c9613d`)** — `fn log_api_access(state: &SharedState, email, slug, path, source)` beside the handlers (api.rs ~:599): clone store/email/slug/path/source, `tokio::spawn`, `let _ = store.log_access(...).await` — the exact server.rs `log_web_access` contract (STAT-05). Call site in `api_read_note` immediately after the `read_note` Ok-collapse and BEFORE the etag/`?meta=1` branching, so raw and meta reads each log exactly one `(slug, 'api', path)` row and no early-return path logs anything.

**Task 2 (`6d8aca0`)** — `fn log_api_search(state, email, vault: Option<&str>, query)` (~:620): ONE spawn doing `match vault { Some(slug) => [slug], None => accessible_vault_entries filtered by webdav::source_dir_of }` then per slug `let _ = store.log_access(email, slug, "api", "-")` + `let _ = store.log_search_query(email, slug, query, "api")` (raw trimmed q, D-04 REST half). Called in `api_search`'s `Ok(hits)` arm before `Json(hits).into_response()`. The stale comment at old :1670-1679 ("DELIBERATELY not ported ... unjustified") was replaced with the Phase 19 closure note naming the three ports.

## New tests (6, all in `api::tests`)

`api_read_note_logs_access`, `api_read_note_meta_logs_access`, `api_denied_call_logs_nothing` (port of mcp.rs:663 — denied read + denied `?vault=` search → 404s, then COUNT(*) access_log = 0 after a 150 ms window), `api_search_logs_access_and_query` (pins BOTH rows: (slug,'api','-') and (slug,'needle','api')), `api_search_multi_vault_logs_per_vault` (global q over 2 vaults → exactly 2 search_queries rows), `api_search_requires_q_logs_nothing` (400 → zero rows in BOTH tables). Helpers `poll_api_log` / `poll_search_query` (40×25 ms poll loops copied from mcp.rs `poll_log`).

## TDD evidence

- **Task 1 RED:** `api_read_note_logs_access` + `api_read_note_meta_logs_access` FAILED ("expected (lr1, api, note.md) access_log row" / "?meta=1 reads are reads too — the row must land"). `api_denied_call_logs_nothing` **passed on write — honest coverage lock** (nothing logged yet; pins the no-probing-trail rule). **GREEN:** `cargo test --lib -- api_read_note api_denied` → 6 passed (incl. untouched `api_read_note_default_raw_markdown` / `api_read_note_meta_json` response-shape pins).
- **Task 2 RED:** `api_search_logs_access_and_query` + `api_search_multi_vault_logs_per_vault` FAILED (no rows). `api_search_requires_q_logs_nothing` **passed on write — honest coverage lock**. **GREEN:** `cargo test --lib api_search` → 6 passed.

## Verification (plan acceptance criteria + verification block)

- `grep "fn log_api_access"` ✓ :599; `grep "let _ = store.log_access"` ✓ :606 (spawn best-effort); `grep 'log_api_access(&state, &user.email, &slug, &path, "api")'` ✓ :543 inside api_read_note after the Ok-collapse (handler read to confirm — no log on any early return).
- `grep log_api_search` → helper :620 + call site :496; `grep -c '"api"'` = 7 (source literals + tests).
- `grep -c "DELIBERATELY not ported"` = **0** (Phase 16 deferral formally closed); `grep "Phase 19 (STAT-01"` matches the replacement comment + both helper docs.
- Full `cargo test --lib` → **579 passed / 0 failed / 4 ignored** (572 baseline + 7... precisely: 572 + 6 new api tests + the count reconciles to 579 with the pre-phase 572 figure already including 19-01/19-02 additions). Two earlier parallel-load runs showed 1-2 failures (`server::tests::admin_reads_vault_files`, export tests) that pass isolated and passed fully green on the clean re-run — load flakiness, not chased per baseline.
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean. No `--no-verify`; nothing pushed; only src/api.rs committed; `git status --porcelain` clean.

## Deviations from plan

1. **[Rule 3 - Plan-text interpretation] Test placement**: the plan said the ports "live below" the retired comment; they live at the end of `mod tests` (still below the comment) in a clearly-labelled `Phase 19 (STAT-01, D-07)` section together with the three search tests. Comment replacement keeps the six-port framing and names the three new test fns verbatim.
2. **[Rule 1 - Tooling friction] rustfmt collapsed the fan-out `.filter(|a| ...)` closure to one line** after the Task 2 commit; folded into the same commit via `--amend` (final hash `6d8aca0`).
3. **Extra assertion in `api_denied_call_logs_nothing`**: also GETs `/api/v1/search?q=needle&vault={private}` (404) before the zero-row assert — strengthens T-19-07 coverage of the search early-return without changing the plan's test name or core assertion.
4. **STATE.md / ROADMAP.md not touched** — executor instructions forbid staging them; orchestrator advances counters centrally.

## Decisions

- Both helpers take `&SharedState` (api.rs's state type) rather than `AppState` — same clone-store-then-spawn shape as server.rs `log_web_access`.
- `log_api_search` does the accessible-vault fan-out INSIDE the spawned task (one task total, matching the plan's "spawns one task doing"), so the response path pays only the spawn.
- Reused `crate::api::accessible_vault_entries` + `crate::webdav::source_dir_of` directly (same module — no cross-module reach into mcp's private helpers; `truncate_chars` was not needed: no excerpt on this surface).

## Lessons

- A best-effort logger that must fan out over DB results should do the DB read INSIDE the spawn — the handler stays untouched and the fan-out set is computed with the same helpers the search itself used.
- Poll-loop tests (40×25 ms) make spawned-insert assertions race-free; the 150 ms sleep + COUNT(*) shape is the correct negative-assertion counterpart.
- `server::tests::admin_reads_vault_files` joins the export tests as load-flaky under full parallel runs (passes isolated + on clean re-run) — re-run before investigating.

## Notes for next executors (19-04, verbatim seams)

**src/api.rs seams (all module-private, no new pub surface):**

```rust
// ~:599 — spawn + `let _ =` (STAT-05). Call site ~:543 in api_read_note,
// AFTER the read_note Ok-collapse, BEFORE etag/?meta= branching:
fn log_api_access(state: &SharedState, email: &str, slug: &str, path: &str, source: &str)

// ~:620 — ONE spawn; Some(slug) → that slug, None → accessible_vault_entries
// filtered by webdav::source_dir_of (same set as the search). Call site ~:496
// in api_search's Ok(hits) arm:
fn log_api_search(state: &SharedState, email: &str, vault: Option<&str>, query: &str)
```

**Row value shapes (what 19-04 renders):**
- `access_log` source='api': note reads → path = the note path as requested (raw AND `?meta=1`); searches → path `'-'`, one row per searched vault (slug-scoped → 1; global → per accessible vault with a source_dir).
- `search_queries`: raw trimmed query text (untruncated), source='api', one row per searched vault — identical fan-out semantics to MCP rows, so `top_search_queries`/`recent_search_queries` mix both sources naturally (filter by source if the page ever needs to split).
- Denied/invalid requests (400/404 problem+json) write NOTHING in either table — stats never become an existence oracle.
- All three surfaces now emit access_log rows: 'web' (server.rs), 'mcp' (mcp.rs), 'api' (this plan) — criterion 2 complete; the REST half of criterion 5 complete.
