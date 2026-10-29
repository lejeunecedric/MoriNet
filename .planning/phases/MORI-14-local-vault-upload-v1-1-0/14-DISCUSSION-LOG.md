# Phase 14 Discussion Log

**Mode:** `--auto` (all gray areas auto-selected, recommended option chosen per question — single pass)
**Date:** 2026-10-29

## Areas discussed & auto-selections

### Upload transport
- Q: multipart vs zip vs per-file API? → **Selected:** single multipart POST with axum `multipart` feature (native, no new crate) (recommended default).

### Size limits
- Q: how to handle axum's 2 MB default body limit? → **Selected:** route-scoped `DefaultBodyLimit` 256 MB + in-code caps 5 MB/file, 2000 files, abort-with-error and store nothing on violation (recommended default).

### Storage & pipeline
- Q: where does the tree live and how is it registered? → **Selected:** `data_dir/uploads/<slug>` + shared `LocalSource` build dispatch + `create_personal_vault`, no vault_source so Refresh keeps its existing no-upstream answer (recommended default).

### Security & gates
- Q: untrusted client paths? → **Selected:** strict sanitization (reject absolute/`..`/backslash/drive-prefix, canonicalized prefix check, skip `.git`/dotfiles/non-markdown), same locked + quota gates as GitHub load, cleanup on build failure (recommended default).

## Claude's discretion
route name/form layout/labels, streaming vs buffering, ignored-file reporting, JS details.

## Deferred
Re-upload updates; zip upload.
