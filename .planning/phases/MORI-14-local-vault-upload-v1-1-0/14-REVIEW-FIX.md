---
status: all_fixed
---

# Phase 14 Review Fix Report — local vault upload (14-REVIEW.md)

**Fixed:** 2026-10-29. TDD: both regression tests written first and confirmed red
(orphan upload returned 400; `..` delete wiped the shared `uploads/` root), then
implemented, then green. Full suite: **334 passed, 0 failed, 4 ignored**
(baseline 332 + 2 new). `cargo clippy --all-targets -- -D warnings` clean,
`cargo fmt --check` clean.

## Findings addressed

### HIGH-1 — concurrent same-slug uploads: loser's `fail_upload` deleted the winner's live tree
**Commit:** `0e853a8` — `fix(14): HIGH-1 exclusive upload dir creation closes slug TOCTOU`

`upload_local_vault` (`src/server.rs`) no longer does `remove_dir_all` +
`create_dir_all` on a TOCTOU-racy pre-check. The dir is now created
**exclusively** with `tokio::fs::create_dir(&dir)`:

- Pre-check keeps only the vault-row check (`store.vault_name(&slug)`) for the
  sequential UX ("the slug '…' is already taken — rename the vault"); the
  `try_exists` arm was removed because exclusive creation now owns that race.
- `AlreadyExists` ⇒ no vault row owns the slug (pre-check), so the dir is an
  orphan from a crashed upload or a concurrent in-flight request. One-shot
  recovery: `remove_dir_all` once, then retry `create_dir` once. If the retry
  also fails, another request is actively recreating the dir → error page
  "an upload for this name is already in progress — rename the vault".
- Because every code path that reaches writing won `create_dir` exclusively,
  it provably owns the dir — `fail_upload`'s `remove_dir_all` (including the
  UNIQUE-conflict arm) can no longer destroy another request's tree.
- Side effect: LOW-1 is fixed too — an orphan dir no longer blocks re-upload
  of the same name (it is recovered on the next upload).

### LOW-2 — early-error arms created the dir without cleanup
**Commit:** `0e853a8` (folded into HIGH-1, as the review suggested)

The `create_dir` non-`AlreadyExists` error arm and the `canonicalize` error arm
now clean up via `fail_upload(&dir, …)` instead of a bare `error_page`.

### MEDIUM-1 — `admin_delete_vault` fed the raw path slug to `remove_dir_all`
**Commit:** `ae49290` — `fix(14): MEDIUM-1 slug-charset guard on uploads cleanup in delete flows`

New helper `slug_is_upload_safe(slug)`: non-empty and every char
`is_ascii_lowercase() || is_ascii_digit() || '-'` — the exact charset
`vault_slug_from_name` emits. The uploads `remove_dir_all` in
`admin_delete_vault` runs only when the slug passes the guard (`POST
/admin/vaults/../delete` with slug `..` can no longer reach the filesystem);
`delete_my_vault` gets the same guard defensively.

## Regression tests (both red before, green after)

- `server::tests::upload_recovers_orphan_dir` — creates `uploads/orphan-book`
  with stale content and no vault row; the upload recovers the dir, returns
  303, the vault serves 200, and the orphan content is gone.
- `server::tests::admin_delete_dotdot_slug_does_not_wipe_data` — as admin,
  `POST /admin/vaults/../delete` (matchit matches slug `..`); asserts
  `data_dir` and its `uploads/` subdir survive.

## Commits

- `0e853a8` fix(14): HIGH-1 exclusive upload dir creation closes slug TOCTOU
- `ae49290` fix(14): MEDIUM-1 slug-charset guard on uploads cleanup in delete flows

Both touch only `src/server.rs`. Not pushed.

## Not addressed (out of scope, per review)

- LOW-1 as a standalone item: resolved implicitly by the HIGH-1 orphan
  recovery (see above).
- INFO items: verified-correct, no action requested.
