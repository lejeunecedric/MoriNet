# 14-01 Summary — local vault upload

**Status:** Complete (both tasks). Commits: `618c770` (backend), `53afad4` (home form).
**Full suite:** 332 passed / 0 failed. Clippy `-D warnings` + `cargo fmt --check` clean.

## What was built

### Task 1 — backend (618c770)
- `axum` gains the `multipart` feature (Cargo.toml:26) — sole dependency change.
- `POST /vaults/upload` inside the protected router, route-scoped
  `DefaultBodyLimit::max(256MB)` (other routes keep the 2 MB default — proven).
- Gate order mirrors `load_personal_vault`: locked refusal → name/slug
  (shared `vault_slug_from_name` helper now used by BOTH handlers) → quota →
  **slug-collision pre-check (`vault_name` + `try_exists`) before any disk
  touch** (checker MAJOR fix — a colliding upload can never wipe a live
  vault's tree).
- `sanitize_upload_name`: rejects backslash / drive-colon / NUL / absolute /
  `.` / `..` / empty segments; strips the top-level picked dir; silently
  ignores dotfiles, `.git`, non-markdown; canonicalized-parent prefix check
  before every write; 5 MB/file + 2000-file caps; ≥1-markdown rule.
- FILE-part-before-`name` aborts (no disk touched).
- Tree lands in `data_dir/uploads/<slug>` → `LocalSource` sync+build (AD-14
  dispatch) → `create_personal_vault`; `set_vault_source` NEVER called, so
  Refresh keeps answering "no upstream to refresh".
- `fail_upload` removes the dir on every post-write failure;
  `delete_my_vault` and `admin_delete_vault` best-effort-remove
  `uploads/<slug>` (repos/ behavior untouched — verified DB-only).

### Task 2 — home page (53afad4)
- "Local vault" form in the Load-a-vault panel: multipart form to
  `/vaults/upload` with required name + `webkitdirectory` picker + `#up-count`.
- `LOCKED_ACCOUNT_JS` `init()` extended with two lines to `block()` the new
  form (same MSG, no duplication).
- New `LOCAL_UPLOAD_COUNT_JS` raw-string const: "N files selected" on change.

## Tests added (12)
`sanitize_upload_name_rules`, `upload_registers_personal_vault`,
`upload_rejects_traversal_path`, `upload_rejects_oversized_file`,
`upload_requires_markdown`, `upload_locked_account_refused`,
`upload_quota_enforced_before_disk`, `upload_body_limit_scoped`,
`upload_unique_slug_conflict` (asserts the first vault still serves 200),
`delete_my_vault_removes_uploads_dir`, `admin_delete_vault_removes_uploads_dir`,
`index_has_local_upload_form` (+ extended `home_page_shows_locked_popup_script`).

## Deviations
- None from the plan's locked decisions. The plan's note that delete flows
  "remove repo dirs" was verified FALSE against the live tree (DB-only
  deletes) — uploads cleanup was added as a new best-effort step only.

## Decisions
- Route `/vaults/upload`; stream parts via `field.chunk()` loop (never
  buffer); ignored files silently skipped; name required and must arrive
  first.

## Lessons
- An uploaded vault's served book lives INSIDE `uploads/<slug>` — any future
  "clean before write" logic must keep the slug pre-check ahead of every
  `remove_dir_all`.
- Splitting a 2-task plan into two executor dispatches (backend, then
  frontend) is what kept both under the 262k subagent context cap on this
  repo; a single 2-task dispatch died there twice in Phase 13/14.
