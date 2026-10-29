---
phase: MORI-14-local-vault-upload-v1-1-0
verified: 2026-10-29
verifier: independent phase verification (against source + live test runs, not SUMMARYs)
commits: 618c770 (endpoint), 53afad4 (home form)
status: passed
score: 10/10 criteria verified (1 sub-item pending live-browser human check)
---

# Phase 14 Verification — Local Vault Upload (REQ-LOC-001 / REQ-LOC-002)

Verification performed independently against the actual implementation in
`src/server.rs` and by re-running the phase's tests. SUMMARY claims were
treated as unverified until reproduced.

## Test runs (executed fresh, this session)

| Command | Result |
|---|---|
| `cargo test --lib upload_` | **10 passed / 0 failed** (registers_personal_vault, rejects_traversal_path, rejects_oversized_file, requires_markdown, locked_account_refused, quota_enforced_before_disk, body_limit_scoped, unique_slug_conflict, index_has_local_upload_form, +1) |
| `cargo test --lib sanitize_upload_name_rules` | **1 passed / 0 failed** |
| `cargo test --lib home_page` | **2 passed / 0 failed** (shows_locked_popup_script, clean_for_unlocked_user) |
| `cargo test --lib index_has` | **3 passed / 0 failed** (index_has_local_upload_form, index_has_three_sections, +tree test) |
| `cargo test --lib removes_uploads_dir` | **2 passed / 0 failed** (delete_my_vault_removes_uploads_dir, admin_delete_vault_removes_uploads_dir) |

## Criterion 1 — REQ-LOC-001: "Local vault" picker uploads the markdown tree

**Verdict: VERIFIED (code + tests); live picker behavior → human check pending.**

Code evidence (`src/server.rs`):
- Home page "Load a vault" panel carries a second form beside the GitHub form
  (~L587–591): `<form method="post" action="/vaults/upload" enctype="multipart/form-data">`
  with a required `name` text input and `<input type="file" name="files" webkitdirectory multiple required>`
  plus a `<span id="up-count">` counter — exactly the ROADMAP's
  `<input type=file>` + `webkitdirectory` option. Submit label is
  "Upload vault" (form itself is the plan's "Local vault" form, 14-01-PLAN L69/202).
- `LOCAL_UPLOAD_COUNT_JS` (L1981–1995): live "N files selected" count on the
  picker's `change` event before submit (D-06).
- `LOCKED_ACCOUNT_JS` (L1998–2032) extended to intercept
  `form[action="/vaults/upload"]` submit with the same locked popup/alert as
  the GitHub form; rendered only for locked accounts (L554–558).
- Route registered on the **protected** router (L184–187) — auth required.

Test evidence: `index_has_local_upload_form` (asserts the form markup and
`form[action="/vaults/upload"]` selector), `home_page_shows_locked_popup_script`,
`home_page_clean_for_unlocked_user` — all pass.

**Human verification pending:** real-browser `webkitdirectory` directory-picker
behavior (OS folder dialog, browser populating relative `file_name`s) cannot be
exercised headlessly. The server side of the contract (multipart parts with
`MyWiki/index.md`-style relative names) IS covered by `multipart_body` tests,
and the markup is asserted. A human should click the picker once on the live
demo to confirm the end-to-end UX.

## Criterion 2 — REQ-LOC-002: stored under owner's space, normal convert/build/register pipeline, quota + ACL like GitHub load

**Verdict: VERIFIED.**

Code evidence (`upload_local_vault`, L2927–3090):
- Tree streamed to `data_dir/uploads/<slug>/` (L2980), never buffered in RAM.
- Gate order mirrors `load_personal_vault` (D-05): `is_locked` refusal FIRST
  (L2934), then name/slug validation, then quota check via
  `effective_personal_vault_limit` + `count_personal_vaults` **before any disk
  write** (L2963–2979), then slug-collision pre-check before disk touch
  (L2984–2992).
- Build goes through the SHARED dispatch: `LocalSource { path: dir }.sync()` +
  `.build()` (L3065–3070) — same convert/build pipeline as the GitHub load
  (AD-14).
- Registration via `create_personal_vault(&claims.email, …)` (L3073–3076) —
  identical owner/quota/ACL semantics; `vault_source` is NEVER set (D-04), so
  Refresh keeps answering "no upstream" for uploaded vaults.
- Every post-write failure calls `fail_upload` (L2917–2920) which removes the
  uploads tree — abort stores nothing.
- Delete flows clean up: `delete_my_vault` (L2068) and `admin_delete_vault`
  (L3365) both `remove_dir_all(uploads/<slug>)` after a successful DB delete.

Test evidence: `upload_registers_personal_vault` (303 redirect to
`/vault/my-local/`, vault listed as personal for the owner, `vault_source`
asserted `None`, files on disk, built book served 200),
`upload_quota_enforced_before_disk`, `upload_locked_account_refused`,
`upload_unique_slug_conflict`, `delete_my_vault_removes_uploads_dir`,
`admin_delete_vault_removes_uploads_dir` — all pass.

## Criterion 3 — Size limits enforced; non-markdown junk ignored/rejected with clear error

**Verdict: VERIFIED.**

Code evidence:
- `UPLOAD_BODY_LIMIT = 256 MB` applied as a **route-scoped**
  `DefaultBodyLimit::max` layer (L186, L2847) — other routes keep axum's 2 MB
  default.
- `MAX_FILE_BYTES = 5 MB` enforced while streaming chunks (L3045–3047,
  "file too large: max 5 MB per file"); `MAX_FILES = 2000` (L3022–3023,
  "too many files: max 2000 per upload").
- `sanitize_upload_name` (L2875–2912): traversal (`..`, `.`, empty segments),
  absolute paths, backslashes, drive prefixes, NUL → whole upload rejected;
  dotfiles/`.git`/non-markdown → silently skipped; top-level picked dir
  stripped. Defense-in-depth: canonicalized parent must be inside the
  canonicalized uploads root **before any byte is written** (L3032–3036).
- "no markdown files found" error when nothing valid arrives (L3061).

Test evidence: `upload_body_limit_scoped` (3 MB upload succeeds on
`/vaults/upload`, beating the 2 MB default), `upload_rejects_oversized_file`,
`upload_requires_markdown`, `upload_rejects_traversal_path` (dir wiped),
`sanitize_upload_name_rules` (Write/Ignored/Rejected matrix) — all pass.

## Deviations / notes

- Button label is "Upload vault" (inside the "Load a vault" panel) rather than
  a literal "Local vault" button; this matches the locked plan decision
  (14-01-PLAN L69/202) and satisfies the criterion's intent (directory picker
  beside the GitHub load form). Not a gap.
- ROADMAP Phase 14 checkbox in ROADMAP.md is already ticked; REQUIREMENTS.md
  REQ-LOC-001/002 boxes remain unchecked (ship/closing task, not this
  verification's scope).

## Overall

**status: passed — 10/10.** All three ROADMAP criteria have concrete code and
passing-test evidence reproduced in this session. One human check remains:
exercise the real `webkitdirectory` picker in a browser on the live demo
(markup + endpoint contract are fully covered headlessly).
