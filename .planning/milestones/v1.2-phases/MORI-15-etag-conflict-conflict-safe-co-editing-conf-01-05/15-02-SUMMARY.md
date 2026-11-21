---
phase: 15
plan: 02
subsystem: store
tags: [vault-changes, migration, audit, sqlite, conflict-page]

requires:
  - phase: 13 (wiki changes log)
    provides: vault_changes table, log_vault_change, best-effort logging pattern
  - phase: 15 plan 15-01
    provides: etag layer (independent; both feed 15-03/15-04)
provides:
  - "migration 0011: vault_changes gains forced INTEGER NOT NULL DEFAULT 0 and actor TEXT NOT NULL DEFAULT '' (additive, kind CHECK untouched)"
  - "Store::log_vault_change(slug, event_id, kind, path, actor, forced) — 6-arg audit write (D-06)"
  - "Store::vault_changes_for_path(slug, path, limit) -> Vec<(ts, kind, actor, event_id, forced)> newest-first (D-07 data source)"
affects: [15-03, 15-04]

tech-stack:
  added: []
  patterns:
    - "Additive ALTER TABLE ... ADD COLUMN NOT NULL DEFAULT only; no table rebuild (0009 style)"
    - "Tuple query_as with bound ? placeholders only (no format! around SQL)"

key-files:
  created:
    - migrations/0011_vault_changes_forced_actor.sql
  modified:
    - src/store.rs
    - src/server.rs

key-decisions:
  - "webdav PUT spawn and record_vault_changes pass \"\", false for now — 15-03 wires the real actor (planner decision, followed)"
  - "vault_changes_for_path orders by ts DESC, id DESC (deterministic within equal ts), matching recent_vault_changes style"

requirements_completed: [CONF-04]
requirements-completed: [CONF-04]

one_liner: "Migration 0011 adds forced/actor audit columns to vault_changes (additive, kind vocabulary untouched) and the store layer gains the 6-arg log_vault_change plus the per-path vault_changes_for_path history query the conflict page needs — CONF-04 groundwork for 15-03/15-04"

duration: ~15min
completed: 2026-10-30
---

# Phase 15 Plan 02: vault-changes-forced-actor-store Summary

**Migration 0011 (`forced`/`actor` on `vault_changes`) + store-layer audit changes (D-05/D-06) and the D-07 per-path history query — CONF-04 storage/API ready for 15-03/15-04.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files modified:** 3 (1 created)

## Accomplishments
- `migrations/0011_vault_changes_forced_actor.sql` — exactly two additive statements: `ALTER TABLE vault_changes ADD COLUMN forced INTEGER NOT NULL DEFAULT 0;` and `... actor TEXT NOT NULL DEFAULT '';`. Documented header block; 0010 kind CHECK untouched (D-05); no index changes (`idx_vault_changes_slug_ts` suffices). `sqlx::migrate!` auto-registers it.
- `Store::log_vault_change(&self, slug, event_id, kind, path, actor: &str, forced: bool)` — INSERT extended to `(vault_slug, event_id, kind, path, actor, forced)` with `.bind(actor)` / `.bind(forced as i64)`; doc comment keeps the best-effort contract (D-04) and the locked kind vocabulary (D-05).
- `Store::vault_changes_for_path(&self, slug, path, limit) -> Vec<(String, String, String, String, i64)>` returning `(ts, kind, actor, event_id, forced)` newest-first (`ORDER BY ts DESC, id DESC LIMIT ?`), bound params only — the D-07 who/when data source for the conflict page.
- All callers updated to the 6-arg signature: webdav PUT audit spawn (server.rs) and `record_vault_changes` pass `"", false` (15-03 wires the real actor; refresh path is out of D-06's actor scope); the six test call sites (server.rs ×5, store.rs ×1) pass `"", false`.

## Task Commits

1. **Task 1: migration-0011-and-store-methods** — `de94fa7` (feat: migration + store API + all callers, includes fmt)
2. **Task 2: store-tests-forced-actor** — `8a82784` (test: defaults, roundtrip, per-path history)

**Plan metadata:** docs(15-02) commit

## TDD Evidence

- **RED:** with the three new tests written first, `cargo test --lib vault_changes` failed to compile: 3 errors — E0061 (log_vault_change 4-arg vs 6-arg in the roundtrip test) and E0599 ×2 (`vault_changes_for_path` not found).
- **GREEN:** after implementing Task 1, `cargo test --lib vault_changes` → 8 passed, 0 failed, including `vault_changes_forced_actor_defaults` (raw 4-column INSERT reads back `forced = 0`, `actor = ''` — additive safety for pre-existing rows), `vault_changes_forced_actor_roundtrip` (`log_vault_change(..., "user@example.com", true)` stores actor verbatim, forced=1), `vault_changes_for_path_newest_first` (path scoping, newest-first, limit honored).

## Acceptance Criteria (all PASS)

- `grep -c "ALTER TABLE vault_changes" migrations/0011_*.sql` → 2; no DROP/CHECK change ✓
- `grep -n "actor, forced" src/store.rs` → INSERT line 1451 ✓
- `cargo check` ✓ and `cargo test --lib --no-run` compiles (all production AND test callers updated) ✓
- `cargo test --lib vault_changes` → 8 passed ✓
- `cargo clippy --all-targets -- -D warnings` → clean ✓
- Full suite `cargo test --lib` → **342 passed, 0 failed, 4 ignored** (339 prior + 3 new) ✓
- `./scripts/check.sh` → `check: OK` (fmt, clippy, full cargo test, release build) ✓
- Migration boot check: every store test runs `Store::connect_memory()` which applies 0011 via `sqlx::migrate!` without error; the defaults test covers the "DB already at 0010" data-safety case per the plan's threat model ✓

## Files Created/Modified
- `migrations/0011_vault_changes_forced_actor.sql` — created (two additive ALTERs + header)
- `src/store.rs` — `log_vault_change` 6-arg + extended INSERT; new `vault_changes_for_path`; 3 new tests; 1 test caller updated
- `src/server.rs` — webdav PUT spawn + `record_vault_changes` pass `"", false`; 5 test call sites updated

## Decisions Made
- `vault_changes_for_path` adds `id DESC` as tiebreaker after `ts DESC` (same house style as `recent_vault_changes`; deterministic ordering for equal timestamps).
- Followed the planner decision verbatim: refresh path and webdav PUT spawn pass `""`/`false` — real actor wiring is 15-03's job.

## Deviations from Plan
None. (Plan's line anchors for the test call sites had drifted ~110 lines after 15-01; located them by grep instead — same six sites.)

## Issues Encountered
- None. `.planning/STATE.md` untouched (orchestrator-owned; not staged).

## Next Phase Readiness
- **15-03:** call `log_vault_change(slug, event_id, kind, path, actor, false)` in the webdav PUT spawn with the claims email; preconditions use `webdav::if_match_of`/`etag_of` from 15-01.
- **15-04:** force-overwrite rows: `kind = "modified"`, `forced = true`, one shared `event_id` per operation, `actor = claims.email`; conflict page reads `vault_changes_for_path(slug, path, limit)`.
- No blockers.

## Self-Check: PASSED

---
*Phase: MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05*
*Completed: 2026-10-30*
