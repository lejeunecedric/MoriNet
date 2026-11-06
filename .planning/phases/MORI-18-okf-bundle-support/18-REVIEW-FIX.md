---
phase: 18-okf-bundle-support
date: 2026-11-06
commit: cde7995
fixed: [REV-18 M-01, REV-18 M-02, REV-18 M-03, REV-18 M-04]
accepted: [REV-18 LOW dup-entries (fixed via symlink skip), REV-18 M-03 rate-limit half, LOW api_vaults N+1, LOW MCP filter cost, LOW finish_upload_update param doc]
status: complete
---

# Phase 18 Review Fix Report

## Fixed (commit `cde7995`)

- **M-01** `src/server.rs` `okf_export` — export failures no longer render
  `path.display()` into the user-facing page. Missing source dir now returns
  the documented 404 collapse (T-18-09); any builder error logs the detail
  server-side (`eprintln!("okf-export: {slug}: …")`) and answers a bare 500.
- **M-02** `src/okf.rs` `build_bundle_zip` — dot-**files** are now skipped in
  addition to dot-dirs: a committed `.env`/`.netrc` can no longer reach
  readers. New test `bundle_zip_skips_dotfiles_and_dotdirs`.
- **M-03** `src/server.rs` — the synchronous walk + deflate now runs via
  `tokio::task::spawn_blocking`, off the async worker.
- **M-04** `src/okf.rs` — symlinks are never followed at all (lstat via
  `DirEntry::file_type()`): an escaping or dangling symlink is skipped +
  logged instead of bailing the whole bundle. This also kills the LOW
  duplicate-entries finding (internal symlinks no longer re-export their
  target) and non-regular files (fifo/socket/device) are excluded.
  Canonicalize+`starts_with` kept as defense in depth. Test renamed
  `bundle_zip_skips_symlink_escape` and strengthened (bundle still builds,
  secret absent).

## Accepted (documented debt)

- **M-03 rate-limit half**: no per-user export rate limit. Export is
  read-ACL-gated and caps-bounded (20k files / 256 MiB pre-buffer); repeated
  exports cost CPU but cannot exceed caps per call. Revisit if abuse seen.
- **LOW `api_vaults` N+1 + arbitrary-JSON embed**: `okf` field only embeds
  parsed `OkfStats`; N+1 bounded by per-user vault quota.
- **LOW MCP filter O(vault) reads**: bounded by page cap, same cost class as
  `?meta=1`.
- **LOW `finish_upload_update` param doc**: passes source dir where doc says
  `book_dir`; works via `source_dir_of` passthrough. Cosmetic.

## Verification

- `cargo test --lib`: **549 passed / 0 failed / 4 ignored**.
- `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- Export route tests (5) + bundle tests (6) green, incl. new dot-file and
  symlink-tolerance assertions.
