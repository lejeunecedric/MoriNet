---
phase: 15
plan: 03
subsystem: webdav
tags: [etag, conditional-put, if-match, if-none-match, atomic-write, per-vault-lock, race-safety]

requires:
  - phase: 15 plan 01
    provides: webdav::etag_of, Precondition enum, if_match_of / if_none_match_of fail-closed parsers
  - phase: 15 plan 02
    provides: Store::log_vault_change 6-arg (actor, forced), vault_changes_for_path
  - phase: 6 (WebDAV surface)
    provides: PUT handler, safe_join, can_write gate
provides:
  - "webdav::write_source_atomic(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<String> — the ONE shared 'write source file' helper (temp file + atomic rename, returns quoted ETag of the written bytes); 15-04's upload path reuses it"
  - "AppState::vault_locks + vault_lock(state, src_dir) -> Arc<tokio::sync::Mutex<()>> — per-vault in-process write lock keyed by source dir; 15-04 reuses the same pattern"
  - "Conditional PUT enforcement: 400 malformed / 412 mismatch (body names current ETag) / 200 conditional update / 201 unconditional — all under the per-vault lock"
affects: [15-04, phase-16-rest-api]

tech-stack:
  added: ["tokio sync feature (Mutex for per-vault locks)"]
  patterns:
    - "std::sync::Mutex guards the lock map only to get-or-insert + clone the Arc; never held across await"
    - "Bind the Arc from vault_lock() to a local before .lock().await (temporary-Arc lifetime)"
    - "check→write entirely inside the tokio Mutex guard; lock released before rebuild_book"
    - "Sibling temp file `.{name}.tmp-{uuid}` in the same dir → rename is atomic on the same filesystem; temp removed on any error"

key-files:
  created: []
  modified:
    - Cargo.toml
    - src/webdav.rs
    - src/server.rs

key-decisions:
  - "412 body names the current ETag (or 'resource does not exist') — only reachable after can_write, so no info leak"
  - "Malformed precondition → 400 evaluated before the body is read (fail-closed, PITFALLS C2)"
  - "If-Match evaluated before If-None-Match per RFC 9110 §13.1/§13.2; either rejecting → 412"
  - "Conditional update → 200; unconditional keeps historical 201 byte-for-byte (CONF-05)"
  - "D-06: vault_changes actor = claims.email (Basic-auth user), forced = false for WebDAV PUT"

requirements_completed: [CONF-02, CONF-03, CONF-05]
requirements-completed: [CONF-02, CONF-03, CONF-05]

one_liner: "Conditional PUT (If-Match/If-None-Match → 400/412/200 with new ETag, unconditional 201 unchanged) enforced race-safely via the ONE shared write_source_atomic temp-file+rename helper under a per-vault tokio write lock (ROADMAP criterion 7), with D-06 actor on audit rows"

duration: ~50min
completed: 2026-10-30
---

# Phase 15 Plan 03: etag-conflict-enforcement Summary

**Conditional PUT preconditions (CONF-02/CONF-03) enforced race-safely (ROADMAP criterion 7) via one shared atomic-write helper + per-vault write lock, with the unconditional PUT regression (CONF-05) and D-06 actor wiring proven by tests.**

## Performance

- **Duration:** ~50 min
- **Started:** 2026-10-30T12:47Z (approx)
- **Completed:** 2026-10-30T13:40Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- **Task 1 — the ONE shared "write source file" helper (criterion 7, PITFALLS C3/C5):**
  - `pub async fn write_source_atomic(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<String>` in `src/webdav.rs` (line 47): writes to sibling temp file `.{name}.tmp-{uuid}` in the SAME directory (same filesystem under `data_dir` → rename is atomic), fsyncs, renames onto the target, removes the temp on any error, and returns the quoted ETag recomputed from the write payload via `etag_of` (never a pre-write hash). No truncated or interleaved targets, ever.
  - `AppState::vault_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>` (server.rs:41) + `fn vault_lock(state, src_dir) -> Arc<tokio::sync::Mutex<()>>` (server.rs:47) — per-vault in-process write lock keyed by source dir; the std mutex is taken only to get-or-insert + clone the Arc, never across await. tokio `sync` feature added to Cargo.toml.
  - WebDAV PUT now writes through the lock + helper; `tokio::fs::write(&full` count in server.rs is 0.
- **Task 2 — conditional PUT enforcement (CONF-02/CONF-03):**
  - PUT branch evaluates `webdav::if_match_of` / `if_none_match_of` (server.rs:931) after `access.can_write()` + `safe_join` and **before the body is read**. Malformed → 400 fail-closed (PITFALLS C2 — never treated as Absent).
  - Inside the per-vault lock: read current bytes → `current_etag = etag_of(...)` → evaluate If-Match (`*` requires existence; Tags must contain the current quoted ETag) then If-None-Match (`*` → 412 if exists = create-only; Tags matching current → 412). Reject → `precondition_failed` (server.rs:1027): 412 whose body names the current ETag — nothing written, no vault_changes row, no rebuild.
  - Success carries the new `ETag` header (CONF-03). Conditional update → 200; unconditional update keeps the historical 201 byte-for-byte (CONF-05).
  - D-06: the fire-and-forget `log_vault_change` now passes `claims.email` as actor and `forced = false` (15-02 placeholder `""` replaced).
- 8 new tests: `write_source_atomic_roundtrip`, `put_if_match_mismatch_412_names_current_etag`, `put_if_match_star_absent_file_412`, `put_if_match_match_update_returns_200_with_new_etag`, `put_if_none_match_star_exists_412`, `put_if_none_match_star_absent_creates_201`, `put_malformed_if_match_weak_etag_400`, `put_unconditional_update_still_201_no_etag_required` (also asserts modified row: actor=admin@example.com, forced=0).

## Task Commits

1. **Task 1: atomic-write-helper-and-per-vault-lock** — `26c5acf` (feat, includes test + helper + lock)
2. **Task 2: put-preconditions-etag-success-actor-tests** — `b2cf4b0` (feat, tests + enforcement)
3. **Hardening** — `eb11ea7` (fix: race-free audit-row poll in CONF-05 test + rustfmt)

**Plan metadata:** this commit (docs(15-03): complete plan)

## TDD Evidence

- **RED 1:** `cargo test --lib write_source_atomic` → compile failure, `write_source_atomic` not found. **GREEN 1:** `write_source_atomic_roundtrip` + existing PUT regression tests (`admin_put_writes_the_source_file`, `webdav_put_records_change`, `webdav_writer_sharee_can_put_reader_cannot`) pass.
- **RED 2:** `cargo test --lib put_` → 7 failed / 9 passed: all six conditional tests got 201 (no enforcement yet) and the CONF-05 actor assertion failed (actor was `""` pre-D-06). **GREEN 2:** `cargo test --lib put_` → 16 passed / 0 failed, stable across 3 consecutive runs.
- **Flake caught & fixed:** first full-suite run showed 349/1 — the CONF-05 test read `rows[0]` and the fire-and-forget added/modified spawns can land out of order under load; fixed by polling for the modified row itself (`eb11ea7`). Subsequent full runs clean.

## Acceptance Criteria (all PASS)

- `grep -n "pub async fn write_source_atomic" src/webdav.rs` → line 47 ✓; `tokio::fs::write(&full` in server.rs → 0 hits ✓
- 412 body names current ETag — asserted in `put_if_match_mismatch_412_names_current_etag` ✓
- Malformed (`W/"x"`) → 400, nothing written — `put_malformed_if_match_weak_etag_400` ✓
- `If-None-Match: *` create-only (412 exists / 201 absent) — both tests ✓
- Successful conditional PUT returns new ETag header equal to `etag_of(new bytes)` — `put_if_match_match_update_returns_200_with_new_etag` ✓
- Unconditional PUT unchanged: 201, file written, modified row with actor + forced=0 — `put_unconditional_update_still_201_no_etag_required` ✓
- Full `cargo test`: **350 passed, 0 failed** (baseline 342 + 8 new) ✓
- `./scripts/check.sh` (fmt + clippy -D warnings + tests + release build): **OK** ✓

## Files Created/Modified

- `Cargo.toml` — tokio `sync` feature
- `src/webdav.rs` — `write_source_atomic` helper (line 47)
- `src/server.rs` — `AppState::vault_locks` + `vault_lock` helper + 4 construction sites; PUT branch rewritten (preconditions → lock → check → body read → atomic write → audit with actor → rebuild → ETag response); `precondition_failed` helper; 8 new tests + `dav_put` test helper

## Decisions Made

- Followed 15-01's verbatim contract: compare quoted forms directly against `etag_of` output; Malformed → 400, never Absent; 412 names the current ETag.
- Kept the Review LOW-02 behavior (metadata error ⇒ assume existed) when classifying added vs modified.
- Lock scope is check→write only; `rebuild_book` runs after the guard drops so a slow mdbook build never blocks other PUTs to the vault.

## Deviations from Plan

None — both tasks executed as written. One test-side race (audit-row ordering under parallel load) was found by the full suite and hardened in `eb11ea7`.

## Issues Encountered

- Temporary-Arc lifetime error when calling `vault_lock(...).lock().await` inline — bind the returned `Arc` to a local first (noted for 15-04).
- `cargo fmt --check` in `scripts/check.sh` reformatted the new code; applied and committed separately.

## Next Phase Readiness

- **15-04 (upload conflict path):** reuse `webdav::write_source_atomic(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<String>` (src/webdav.rs:47) and the `vault_lock(&state, &src_dir)` pattern (src/server.rs:47) — bind the Arc to a local before `.lock().await`. Compare incoming vs current bytes with `etag_of` (D-02); forced uploads pass `forced = true` + real actor to `log_vault_change`.
- **Phase 16 REST API:** `Precondition` parsers + `etag_of` + the atomic helper are all public and subsystem-agnostic.
- No blockers.

## Self-Check: PASSED

---
*Phase: MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05*
*Completed: 2026-10-30*
