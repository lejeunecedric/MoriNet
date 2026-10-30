---
phase: 15
plan: 04
subsystem: server
tags: [upload, conflict-gate, force-overwrite, staging, vault-changes, audit]

requires:
  - phase: 14 (local vault upload)
    provides: upload_local_vault gate order, sanitization, caps, exclusive create
  - phase: 15 plan 01
    provides: webdav::etag_of (quoted sha256) for the D-02 compare
  - phase: 15 plan 02
    provides: log_vault_change 6-arg (actor, forced), vault_changes_for_path
  - phase: 15 plan 03
    provides: webdav::write_source_atomic + vault_lock per-vault write lock
provides:
  - "upload update path: same-slug re-upload of an OWNED uploaded vault updates it (D-01/D-04) with per-file hash-compare conflict gate (D-02)"
  - "409 conflict JSON {status,vault,conflicts[path,server_etag,changes]} consumed by the 15-05 two-phase fetch JS"
  - "force=1 multipart text field overwrites + records modified/added rows with forced + real actor, one event_id (D-05/D-06)"
affects: [15-05]

tech-stack:
  added: []
  patterns:
    - "Staging dir uploads/.staging/<slug>-<uuid>: stream+sanitize there, compare, then apply to the live tree only via write_source_atomic under vault_lock"
    - "work_dir = staging (update) or fresh dir (new vault): fail_upload's tree-wipe can only ever hit the tree the request owns"
    - "Staging dir slug-prefixed so a test can prove ITS OWN leftovers are gone despite the shared parallel-test data_dir"

key-files:
  created: []
  modified:
    - src/server.rs

key-decisions:
  - "update_mode eligibility exactly per plan: vault_name Some + vault_source None + slug in list_vaults_for(claims.email) — no new store method (planner decision, followed)"
  - "quota block and exclusive-create block wrapped in `if !update_mode` — quota gates creation only; an at-limit user can still update"
  - "conflict `changes` rows expose {ts, kind, actor} only (event_id/forced dropped) — owner-only endpoint, minimal disclosure"
  - "existing upload_unique_slug_conflict test premise superseded by D-01: same-owner identical re-upload is now an idempotent update (redirect), not the 'rename the vault' refusal"

requirements_completed: [CONF-04, CONF-05]
requirements-completed: [CONF-04, CONF-05]

one_liner: "Same-slug re-upload of an owned uploaded vault becomes the update path: staging + per-file sha256 conflict gate (409 rich JSON), force=1 overwrite via write_source_atomic under the per-vault lock with forced/actor audit rows, plain new-vault uploads byte-for-byte unchanged"

duration: ~45min
completed: 2026-10-30
---

# Phase 15 Plan 04: upload-update-path-conflict-gate Summary

**Upload conflict detection + force-overwrite (CONF-04) on the update path, with the plain new-vault upload regression (CONF-05) proven unchanged.**

## Performance

- **Duration:** ~45 min
- **Tasks:** 2 (implemented test-first: RED observed before the handler change)
- **Files modified:** 1 (`src/server.rs`)

## Accomplishments

- **Update-path eligibility (D-01/D-04)** — right after slug derivation, before quota: when the slug is registered, `vault_source` is None (uploaded vault — git vaults keep Refresh semantics) and the slug is in `list_vaults_for(claims.email)` (ownership, same source as the ownership gates), `update_mode = true`. Quota check, "already taken" pre-check and exclusive-create block are wrapped in `if !update_mode` — quota gates creation only, so an at-limit user can still update. Everything else keeps the verbatim "already taken" error_page (covers git-sourced and other-users' vaults).
- **Staging (threat model: caps + cleanup)** — update streams parts to `uploads/.staging/{slug}-{uuid4}/` reusing the identical sanitization (rel-path rejection + `parent.canonicalize().starts_with(staging_root)`) and caps (2000 files, 5 MB/file, markdown-required). `work_dir` is the tree the request owns (staging in update mode, fresh `uploads/<slug>` otherwise), so `fail_upload`'s tree-wipe is never reachable on a live tree; staging is removed on every exit path (success, 409, every error via `abort_staging`).
- **Conflict gate (D-02)** — per staged file, `webdav::etag_of` compare vs the live file when present; conflict = both sides, different sha256. Conflicts + no force → 409 `application/json`: `{"status":"conflict","vault":…,"conflicts":[{"path","server_etag","changes":[{ts,kind,actor}]}]}` with `changes` from `vault_changes_for_path(slug, path, 5)` (D-07 data). Nothing written on 409 (test asserts live bytes unchanged).
- **Force + apply (D-05/D-06, criterion 7)** — `force` text field picked up in the non-file-part branch (`force=1`, may arrive after file parts). Apply phase runs under the per-vault `vault_lock` (same lock as WebDAV PUT — second consumer of the shared helper): `create_dir_all(parent)` + `webdav::write_source_atomic` per file; server-only files untouched (force never deletes). Audit: one `event_id` per operation, kind `modified` for overwritten / `added` for local-only, `forced = force_flag`, `actor = claims.email`, best-effort spawn + eprintln swallow. Lock released before the rebuild; rebuild via the same `LocalSource` dispatch; success = `Redirect::to("/vault/{slug}/")` (same as new-vault path).
- **New-vault path unchanged (CONF-05)** — `update_mode == false` flow is behaviorally byte-for-byte the Phase 14 pipeline (only `dir` → `work_dir` rename, which is the same dir there).
- 7 new tests + 1 updated: `upload_update_conflict_returns_409_json` (409 body shape + server ETag hex + no partial write + no staging leftovers), `upload_update_force_overwrites_and_records` (redirect, v2 bytes, modified row forced=1 actor=admin@example.com, added row for local-only file, one shared event_id), `upload_update_adds_local_only_keeps_server_only`, `upload_update_refused_for_other_users_vault`, `upload_update_refused_for_git_vault`, `upload_plain_new_vault_regression` (CONF-05), `upload_update_rejects_traversal_path` (threat model: traversal refused on the update path before any live write).

## Task Commits

1. **Tasks 1+2: update-path conflict gate + e2e tests** — `6a8749f` (feat, tests + handler in one commit; tests written first, RED observed before the handler patch)
2. **Formatting** — `style(15-04)` rustfmt commit

**Plan metadata:** this commit (docs(15-04): complete plan)

## TDD Evidence

- **RED:** with the new tests written first, `cargo test --lib upload_` → 13 passed / 5 failed — the five new-behavior tests got the old "already taken" 400 refusal (conflict test showed the error page verbatim); the refusal/regression tests already passed against existing behavior.
- **GREEN:** after the handler implementation, `cargo test --lib upload_` → 18 passed / 0 failed, stable across 3 consecutive runs (audit-row polling uses the 15-03 race-free poll pattern).
- One compile fix during GREEN: `work_dir.as_path()` hit unstable `str_as_str` inference — used `unwrap_or(work_dir)` directly.

## Acceptance Criteria (all PASS)

- `grep -n '"status": "conflict"\|StatusCode::CONFLICT' src/server.rs` → serde_json-built body at :3394, `StatusCode::CONFLICT` at :3400 ✓
- Source assertion: quota block and exclusive `create_dir` block are inside `if !update_mode { … }` ✓
- Source assertion: `fail_upload(&dir, …)` remains only in the new-vault path (3 sites, all unreachable in update_mode); `finish_upload_update` only ever `remove_dir_all`s the staging dir ✓
- `cargo check` ✓; `cargo clippy --all-targets` no warnings ✓; `cargo fmt --check` clean ✓
- `cargo test --lib upload_` → 18 passed (7 new + all pre-existing upload_* tests; `upload_unique_slug_conflict` updated per D-01, see Deviations) ✓
- 409 test asserts live file bytes unchanged (no partial write) ✓
- Force test asserts `forced = 1`, `actor = admin@example.com`, kind `modified`, one shared event_id ✓
- Full `cargo test`: **357 passed, 0 failed** (baseline 350 + 7 new) ✓
- `./scripts/check.sh` (fmt + clippy -D warnings + tests + release build): **OK** ✓

## Files Created/Modified

- `src/server.rs` — `upload_local_vault`: update_mode eligibility check, quota/pre-check/exclusive-create gated on `!update_mode`, staging dir + `work_dir` plumbing, `force` field pickup, handoff to new `finish_upload_update`; new helpers `collect_rel_files`, `abort_staging`, `finish_upload_update`; 7 new tests + `staging_leftovers`/`poll_vault_changes`/`seed_upload` test helpers; `upload_unique_slug_conflict` updated

## Decisions Made

- Staging dir named `{slug}-{uuid4}` instead of bare `{uuid4}` (minor deviation, see below) — makes the threat-model "no .staging leftovers" assertion deterministic per test despite the shared parallel-test `data_dir`.
- Conflict `changes` entries expose `{ts, kind, actor}` only; `event_id`/`forced` are dropped from the wire shape (the D-07 page needs who/when; the endpoint is owner-only anyway).
- `force` field is read in both modes (harmless no-op for new vaults) so the curl fallback works uniformly; only the update path consumes it.

## Deviations from Plan

1. **`upload_unique_slug_conflict` updated (not "unchanged")** — the plan's acceptance said pre-existing upload tests stay unchanged, but this test's premise (same-owner same-slug re-upload → "rename the vault") is exactly what D-01 supersedes. Updated to assert the new semantics: identical re-upload by the owner is an idempotent update (redirect, tree intact, still serves). The "already taken" wording is still covered unchanged by `upload_update_refused_for_other_users_vault` and `upload_update_refused_for_git_vault`.
2. **Staging dir slug-prefixed** (`{slug}-{uuid4}` vs `{uuid4}`) — same location and lifecycle, chosen so each test can prove its own leftovers are gone without flaking on concurrent tests' in-flight staging.

## Issues Encountered

- None blocking. The linter noise on every `patch` call (Rust 2015 edition false positives on the whole file) is pre-existing tooling noise, not introduced here.

## Next Phase Readiness

- **15-05 (two-phase fetch JS):** the server contract is fixed — POST same slug → 409 JSON `{status:"conflict",vault,conflicts:[{path,server_etag,changes:[{ts,kind,actor}]}]}`; re-POST with `force=1` text field → 303 redirect; any non-409 is success. Plain form POST without `force` still works from curl (non-JS fallback).
- No blockers.

## Self-Check: PASSED

---
*Phase: MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05*
*Completed: 2026-10-30*
