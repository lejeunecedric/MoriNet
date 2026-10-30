---
status: all_fixed
date: 2026-10-30
fixed:
  - id: REV-15-01
    commit: 25290c9
  - id: REV-15-02
    commit: 2be4bd9
test_summary: 362 passed / 0 failed (359 baseline + 3 new regression tests); scripts/check.sh OK
---

# Phase 15 Review Fixes

## REV-15-01 (HIGH) — upload-update gate used the wrong ownership source

**Fix (25290c9):** `upload_local_vault`'s `update_mode` gate now calls
`can_edit_vault(&state, &claims.email, &slug)` — the same owner gate the Edit
page uses (admin OR `owner_id` via `accessible_vaults`) — instead of
`list_vaults_for`, whose non-admin branch joins only the legacy `vault_acl`
table that production never populates. Non-admin owners now reach the CONF-04
update path; non-owners still get the unchanged "already taken" refusal.

**Regression tests (in `src/server.rs` tests):**
- `upload_update_allowed_for_nonadmin_owner` — non-admin owner re-upload:
  409 conflict JSON on diverged content, 303 on clean update (was 400
  "already taken" pre-fix; observed RED then GREEN).
- `upload_update_refused_for_other_nonadmin_same_slug` — a different non-admin
  re-uploading the same slug is still refused 400 with the owner's files
  untouched (guard test; passed pre-fix by construction).
- All pre-existing admin-path upload-update tests kept passing.

## REV-15-02 (MEDIUM) — conflict scan ran outside the vault_lock

**Fix (2be4bd9):** in `finish_upload_update`, the per-file conflict scan, the
force gate, and the apply loop now run under ONE `vault_lock` guard (same
check→write-under-lock contract as the WebDAV PUT handler, ROADMAP criterion
7). The 409 JSON build (DB reads via `vault_changes_for_path`) happens after
the guard drops; the rebuild still runs after the lock is released (15-03
pattern).

**Race test approach (`upload_update_conflict_scan_holds_vault_lock`):**
deterministic simulation, no timing gamble on the scan window itself. The
test (same module, so it can use `vault_lock` and `finish_upload_update`
directly):
1. Seeds a vault whose live file equals the staged file (`"same\n"`), so the
   initial scan sees NO conflict.
2. Acquires the same per-vault `vault_lock` the WebDAV PUT handler uses, then
   spawns `finish_upload_update` (force=false) — it blocks on the lock.
3. While the update is blocked, mutates the live file to `"webdav put\n"` —
   exactly the window a concurrent PUT lands in pre-fix (between scan and
   lock).
4. Releases the lock and asserts 409 + live file still `"webdav put\n"`.

Pre-fix: scan ran before the lock, saw no conflict, apply silently
overwrote the mutation → 303 (observed RED). Post-fix: the under-lock scan
sees the mutation → 409, no write (GREEN).

## Verification

- Baseline before fixes: 359 passed / 0 failed.
- After both fixes: `cargo test` aggregate **362 passed / 0 failed**
  (359 + 3 new regression tests).
- `./scripts/check.sh`: OK (fmt, clippy, tests, release build).
- Commits on `main` (not pushed, no branches): `25290c9`, `2be4bd9`.
