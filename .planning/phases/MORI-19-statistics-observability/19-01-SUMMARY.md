---
phase: MORI-19-statistics-observability
plan: 19-01
one_liner: "Migration 0014 rebuilds access_log with CHECK (source IN ('web','mcp','api')) preserving all rows and both indexes, adds mcp_activity + search_queries tables, and ships the seven store accessors plus 90-day boot prunes every later Phase-19 plan builds on."
requirements_completed: [STAT-01, STAT-02, STAT-03, STAT-04, STAT-05]
subsystem: database
tags: sqlite, migration, table-rebuild, access-log, mcp-activity, search-queries, prune

# Dependency graph
requires:
  - phase: 10.1 (shares + stats)
    provides: access_log table + CHECK (source IN ('web','mcp')) + stats queries + prune convention
  - phase: 13 (wiki changes)
    provides: vault_changes + prune_vault_changes 90-day boot-prune convention
provides:
  - migration 0014 — access_log.source CHECK widened to web|mcp|api via table rebuild (history + indexes preserved)
  - mcp_activity + search_queries tables with CHECK vocabularies and slug_ts indexes
  - Store accessors: log_mcp_activity / recent_mcp_activity / log_search_query / top_search_queries / recent_search_queries
  - prune_mcp_activity + prune_search_queries wired at boot (eprintln-guarded)
  - days: i64 window parameter on stats_per_day / stats_top_pages / stats_own_count / recent_vault_changes
affects: [19-02-mcp-instrumentation, 19-03-rest-api-logging, 19-04-statistics-page]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "First SQLite table rebuild (create-new/copy/drop/rename) inside a sqlx migration — no BEGIN/COMMIT, sqlx wraps the file"
    - "Date-window filters bind the datetime() modifier as a parameter (format!(\"-{days} days\")), never string-concatenated"
    - "Boot prunes follow the prune_auth_events eprintln-guard shape — a failed prune never aborts boot"

key-files:
  created: [migrations/0014_stats_observability.sql]
  modified: [src/store.rs, src/server.rs, src/main.rs]

key-decisions:
  - "recent_mcp_activity returns excerpt as String (plan's tuple type listed a third i64 — a typo; excerpt is TEXT in the schema)"
  - "vault_changes_roundtrip_and_grouping passes days=36500 because its fixture uses fixed Jan-2026 timestamps outside any realistic window"
  - "#[allow(clippy::too_many_arguments)] on log_mcp_activity (9 args pinned by plan; precedent src/oidc.rs:870)"

patterns-established:
  - "Table rebuild migration: explicit column-list INSERT...SELECT including id, both indexes recreated after rename"
  - "Detailed-activity inserts are best-effort: callers spawn and ignore errors (mirrors log_access)"

requirements-completed: [STAT-01, STAT-02, STAT-03, STAT-04, STAT-05]

duration: ~35min
completed: 2026-11-18
---

# Phase 19 / Plan 19-01: storage foundation — migration 0014, store accessors, boot prune Summary

**Migration 0014 rebuilds access_log with CHECK (source IN ('web','mcp','api')) preserving every row and both indexes, adds mcp_activity + search_queries tables, and ships the seven store accessors plus 90-day boot prunes every later Phase-19 plan builds on.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-11-18T16:30Z (approx)
- **Completed:** 2026-11-18T17:05Z
- **Tasks:** 2 (both TDD)
- **Files modified:** 4

## Accomplishments
- `migrations/0014_stats_observability.sql`: access_log table rebuild (create-new/copy/drop/rename, explicit column-list copy incl. id, both indexes recreated) + `mcp_activity` (tool CHECK list_notes|read_note|search_notes, status CHECK ok|error, capped excerpt — no column can hold a full body) + `search_queries` (source CHECK mcp|api), each with a `(vault_slug, ts)` index. No BEGIN/COMMIT (sqlx wraps the file).
- Store accessors (src/store.rs, section "Detailed MCP activity + search queries", after `prune_access_log`): `log_mcp_activity`, `recent_mcp_activity`, `log_search_query`, `top_search_queries`, `recent_search_queries`, `prune_mcp_activity`, `prune_search_queries`.
- D-06 groundwork: `days: i64` added to `stats_per_day`, `stats_top_pages`, `stats_own_count`, `recent_vault_changes` (bound `datetime('now', ?)` modifier; `recent_vault_changes` filters BOTH the outer WHERE and the inner event_id subquery). All server.rs call sites pass `90` — behavior identical to before (90 = retention ceiling).
- STAT-05 boot prune: `prune_mcp_activity` + `prune_search_queries` called in main.rs after `prune_auth_events` in the eprintln-guard shape.
- Pin test `access_log_rejects_bad_source` moved to web|mcp|api in the same commit as the rebuild (D-07).

## Task Commits

1. **Task 1: migration-0014-rebuild-and-new-tables** — `d856897` (feat + tests, same commit per D-07)
2. **Task 2: store-accessors-date-range-and-boot-prune** — `a7d7167` (feat + tests)
3. **fmt/clippy follow-up** — `85a681a` (style)

## TDD Evidence

**Task 1 RED** (before the migration file existed):
- `access_log_accepts_all_three_sources` — FAILED: `CHECK constraint failed: source IN ('web', 'mcp')`
- `access_log_rejects_bad_source` (updated pin test) — FAILED: same CHECK rejection of 'api'
- `mcp_activity_rejects_bad_tool_and_status` — FAILED: `no such table: mcp_activity`
- `search_queries_rejects_bad_source` — FAILED: `no such table: search_queries`
- `access_log_indexes_survive_rebuild` — passed on write: honest **coverage lock** (indexes already existed from 0008; the test pins their survival through the rebuild).

**Task 1 GREEN:** `cargo test --lib -- access_log mcp_activity search_queries` → 7 passed, 0 failed.

**Task 2 RED:** compile failure — 22 errors: E0599 ×7 (each new method absent) + E0061 (stats_top_pages arity).

**Task 2 GREEN:** `cargo test --lib store::` → 102 passed; targeted server filters (refresh/wikidav/stats) → 20 passed.

**Full suite:** `cargo test --lib` → **564 passed / 1 failed / 4 ignored** (554 baseline + 11 new). The single failure was `server::tests::export_zip_streams_bundle` ("okf-export: demo: Permission denied (os error 13)") under parallel load — both known-flaky export tests pass in isolation (`cargo test --lib -- export_waits_for_semaphore_permit export_zip_streams_bundle` → 2 passed). Recorded as the pre-existing flake per baseline instructions; not chased.

## New tests (11)

`store::access_log_tests`: `access_log_accepts_all_three_sources`, `access_log_indexes_survive_rebuild`, `mcp_activity_rejects_bad_tool_and_status`, `search_queries_rejects_bad_source`, `mcp_activity_roundtrip_newest_first`, `recent_mcp_activity_respects_days`, `top_search_queries_aggregates_and_orders`, `recent_search_queries_newest_first`, `prune_mcp_activity_removes_only_old_rows`, `prune_search_queries_removes_only_old_rows`, `stats_top_pages_respects_days`.

## Deviations from plan

1. **`recent_mcp_activity` tuple type**: the plan's signature listed `Vec<(String, String, String, String, String, i64, i64, i64)>` for `(ts, tool, email, args, status, latency_ms, resp_size, excerpt)` — the 8th element (excerpt) is TEXT in the schema, so the last tuple element is `String`, not `i64`. The plan's type list was a typo; implemented as `(String, String, String, String, String, i64, i64, String)`.
2. **Extra call sites in store.rs**: the plan listed only server.rs call sites for the signature changes, but `store::vault_changes_tests::vault_changes_roundtrip_and_grouping` also calls `recent_vault_changes` with a fixture of fixed Jan-2026 timestamps (older than any realistic window). It now passes `days=36500` (100 years) so its grouping/capping assertions stay pinned; the window itself is pinned by the new Phase-19 days tests.
3. **One extra test**: `recent_search_queries_newest_first` — the plan's test list omitted coverage for `recent_search_queries`; added a small newest-first assertion.
4. **`#[allow(clippy::too_many_arguments)]` on `log_mcp_activity`** (9 args pinned by the plan for 19-02 callers; same allow already exists at src/oidc.rs:870).
5. **Migration header avoids the literal word "BEGIN"** (says "no explicit transaction statements") so the acceptance grep `grep -c "BEGIN"` reports 0.
6. **Pre-existing flake**: `server::tests::export_zip_streams_bundle` failed once in the full parallel run, passes isolated — recorded per baseline, not debugged.
7. Minor hygiene: `log_access` doc comment updated to the web|mcp|api vocabulary (D-07).

## Decisions

- Two new tables (not columns on access_log) per 19-CONTEXT "Claude's Discretion" — access_log rows stay count-only aggregates.
- `recent_vault_changes` binds the same `-{days} days` modifier twice (outer + subquery) via one `modifier` variable.
- New prunes placed after the `prune_auth_events` block in main.rs, keeping the `?`-propagating prunes untouched.

## Lessons

- sqlx's SQLite migrator wraps each file in a transaction — a rebuild migration must contain no transaction statements, and the acceptance grep for "BEGIN" also counts comments, so phrase headers around it.
- When adding a date window to an existing query, existing fixtures with hard-coded old timestamps silently fall outside it — grep test fixtures for literal dates before pinning windows.
- `sqlx::query_as::<_, Vec<String>>` does not compile for a single column; fetch `Vec<(String,)>` and map.

## Notes for next executors (verbatim contracts)

Store (src/store.rs, section `─── Detailed MCP activity + search queries (Phase 19, D-01/D-04) ───`, after `prune_access_log`):

```rust
#[allow(clippy::too_many_arguments)]
pub async fn log_mcp_activity(&self, email: &str, slug: &str, tool: &str, args: &str,
    status: &str, latency_ms: i64, resp_size: i64, excerpt: &str) -> anyhow::Result<()>
// tool CHECK: 'list_notes'|'read_note'|'search_notes'; status CHECK: 'ok'|'error'.
// 19-02 MUST cap args/excerpt at 200 chars UTF-8-boundary-safe at the write site.

pub async fn recent_mcp_activity(&self, slug: &str, days: i64, limit: i64)
    -> anyhow::Result<Vec<(String, String, String, String, String, i64, i64, String)>>
// (ts, tool, email, args, status, latency_ms, resp_size, excerpt), ORDER BY ts DESC, id DESC

pub async fn log_search_query(&self, email: &str, slug: &str, query: &str, source: &str)
    -> anyhow::Result<()>   // source CHECK: 'mcp'|'api' — 19-03 passes 'api'

pub async fn top_search_queries(&self, slug: &str, days: i64, limit: i64)
    -> anyhow::Result<Vec<(String, i64)>>   // GROUP BY query ORDER BY c DESC, query ASC

pub async fn recent_search_queries(&self, slug: &str, days: i64, limit: i64)
    -> anyhow::Result<Vec<String>>          // raw strings newest-first (19-04 tokenizer input)

pub async fn prune_mcp_activity(&self) -> anyhow::Result<u64>
pub async fn prune_search_queries(&self) -> anyhow::Result<u64>   // 90-day DELETE, wired at boot main.rs:157-163
```

Changed signatures (server.rs currently passes `90` at :2847, :2914, :2929, :2948 — 19-04 replaces 90 with the chip value):

```rust
pub async fn stats_per_day(&self, slug: &str, days: i64) -> anyhow::Result<Vec<(String, String, i64)>>
pub async fn stats_top_pages(&self, slug: &str, days: i64) -> anyhow::Result<Vec<(String, i64)>>
pub async fn stats_own_count(&self, slug: &str, email: &str, days: i64) -> anyhow::Result<i64>
pub async fn recent_vault_changes(&self, slug: &str, max_events: i64, max_rows: i64, days: i64)
    -> anyhow::Result<Vec<(String, String, String, String)>>
```

- access_log CHECK now accepts `source='api'` (migration 0014) — 19-03 can log API reads/searches directly.
- Test fixtures: store tests use `Store::connect_memory()` via `store()` (mod tests) or `st(_name)` (mod access_log_tests, src/store.rs ~:3357); new Phase-19 store tests live at the end of `mod access_log_tests`. No tempfile anywhere.
- Migration style: header comment `-- Phase 19 (…): …`, no transaction statements, `sqlx::migrate!` auto-picks new files (src/store.rs:117/:127).
