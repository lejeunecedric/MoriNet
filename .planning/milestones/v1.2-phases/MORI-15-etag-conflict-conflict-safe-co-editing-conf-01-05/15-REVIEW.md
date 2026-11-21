---
status: issues_found
phase: 15
date: 2026-10-30
counts:
  critical: 0
  high: 1
  medium: 1
  low: 5
  info: 3
---

# Phase 15 Code Review — ETag / Conflict-Safe Co-Editing (CONF-01..05)

Scope: `git diff 3ad2852..HEAD -- src/ Cargo.toml migrations/` (commits da6093d..4ba3cc7, plans 15-01..15-05).
Reviewed read-only; no source modified. Files: `src/server.rs`, `src/store.rs`, `src/webdav.rs`, `migrations/0011_vault_changes_forced_actor.sql`, `Cargo.toml`.

## Findings

### REV-15-01 — HIGH — Upload-update path is admin-only; non-admin owners can never update their own vault

- **File:** `src/server.rs:3202-3220` (update_mode gate), `src/store.rs:211-260` (`list_vaults_for`)
- **Description:** The update-path eligibility check uses `list_vaults_for(&claims.email).any(|v| v.slug == slug)` and the comment claims it is the "same source as the ownership gates". It is not. `list_vaults_for`'s non-admin branch selects vaults **only** via a join on the legacy `vault_acl` table (kept "for one release as a fallback" per `migrations/0004_groups.sql`). Nothing in production code ever inserts `vault_acl` rows — `grant_acl` is called only from tests — and `create_personal_vault` (`src/store.rs`) sets `owner_id` without touching `vault_acl`. Result: for any non-admin user, `list_vaults_for` returns an empty list, `owned` is always `false`, `update_mode` is always `false`, and re-uploading their own vault falls through to the "the slug '…' is already taken — rename the vault" refusal. The entire CONF-04 update path (staging, conflict gate, force, audit) is reachable only by admins. Every 15-04 test passes because it logs in as `admin@example.com`. The correct source of truth is the ownership model used by the other gates (`owner_id` / `accessible_vaults`, cf. the owner-gate comment at `src/server.rs:2301-2304`).
- **Suggested fix:** Gate on ownership, not the legacy ACL list — e.g. a store method `has_personal_vault(email, slug)` (the same query the Edit-page owner gate uses), or check `accessible_vaults`/`owner_email == claims.email`. Add a regression test with a non-admin owner performing a re-upload.

### REV-15-02 — MEDIUM — Conflict scan runs outside the per-vault lock (TOCTOU vs concurrent WebDAV PUT)

- **File:** `src/server.rs:3478-3492` (conflict scan), `src/server.rs:3533-3534` (lock acquired only for apply)
- **Description:** The phase contract (ROADMAP criterion 7 / C3) is that check→write happens under `vault_lock`. In `finish_upload_update` the conflict comparison (staged vs live sha256) happens **before** `vault_lock(state, dir)` is acquired, and the apply loop under the lock never re-verifies content — it only re-reads `existed` for the audit kind. A WebDAV PUT that lands between the scan and lock acquisition is silently overwritten even without `force=1`, defeating the conflict gate for exactly the co-editing race the phase exists to prevent. (Contrast the WebDAV PUT handler at `src/server.rs:940-983`, which correctly holds the lock across its whole check→write.)
- **Suggested fix:** Move the conflict scan inside the same `vault_lock` section as the apply (scan + gate + apply under one guard), or re-hash each conflicting target under the lock before writing and re-emit 409 if a new conflict appeared.

### REV-15-03 — LOW — Per-vault write lock held across the full request-body read (slow-client lock starvation)

- **File:** `src/server.rs:940-941, 969-972`
- **Description:** In the WebDAV PUT branch, `axum::body::to_bytes(body, 8MB)` runs while `_dav_guard` is held. A slow or stalled client (slow-loris style) holds the per-vault write lock for the entire body transfer — there is no read timeout — blocking every other PUT and every upload-update apply for that vault. Precondition evaluation itself only needs the on-disk state; the body could be read before taking the lock (malformed-precondition 400 already precedes it).
- **Suggested fix:** Read the body before acquiring the lock (payload cap already enforces the size bound), or wrap the body read in a timeout; keep only check→write under the guard.

### REV-15-04 — LOW — `If-None-Match` (tag list) against an existing non-matching file returns 201, not 200

- **File:** `src/server.rs:1012-1016`
- **Description:** The status rule is `if existed && if_match != Absent { 200 } else { 201 }`. A conditional request that carries only `If-None-Match: "<etag>"` (not `*`) against an existing file whose ETag differs passes the gate and overwrites the file, yet answers 201 CREATED because `if_match` is `Absent`. CONF-05's "unconditional keeps 201, conditional answers 200" intent is arguably violated for this conditional variant.
- **Suggested fix:** Treat any present precondition as conditional: `if existed && (if_match != Absent || if_none_match != Absent) { 200 }`.

### REV-15-05 — LOW — `forced=1` recorded on audit rows even when no conflict was overwritten

- **File:** `src/server.rs:3560-3575` (audit spawn uses `force_flag` unconditionally)
- **Description:** Migration 0011 and D-05 define `forced=1` as "overwrote a conflicting file after explicit confirmation". The audit spawn stamps **every** row of the operation with `force_flag`, including pure additions and identical-content re-uploads where `conflicts` was empty. A `force=1` re-upload with zero conflicts records `forced=1` rows, polluting the force-overwrite audit trail the column exists to provide.
- **Suggested fix:** Pass `force_flag && conflicted_paths.contains(path)` per row (the conflict set is already computed at `src/server.rs:3478-3492`).

### REV-15-06 — LOW — Partial application on mid-apply failure: live tree half-updated with no rollback or report

- **File:** `src/server.rs:3535-3556` (apply loop; `abort_staging` on first error)
- **Description:** Each file is atomic (`write_source_atomic`), but the operation is not: if file *k* fails (disk full, permission), files 1..k-1 are already live, the staging dir is deleted, and the user sees a generic error page — the vault now serves a mix of old and new content with no indication of which files landed, and the audit rows for the applied files are never written (audit spawn sits after the loop).
- **Suggested fix:** At minimum, log/return which paths were applied before the failure; consider deferring audit for applied files (write audit rows incrementally) so the history matches the disk state.

### REV-15-07 — LOW — Staging dir leaks when the client disconnects mid-upload

- **File:** `src/server.rs:3270-3280` (staging creation), cleanup only in `abort_staging`/`fail_upload`/success paths
- **Description:** All staging cleanup happens on explicit return paths. If the client aborts the multipart request, axum drops the handler future between `.await` points and none of the cleanup code runs — the `<slug>-<uuid>` dir under `uploads/.staging/` persists forever. There is no startup or periodic sweep of `.staging`. Repeated aborted updates accumulate disk usage.
- **Suggested fix:** Sweep `uploads/.staging/*` older than a threshold at startup (or on each update that creates a staging dir).

### REV-15-08 — INFO — PROPFIND hash-on-read reads every file fully into memory with no size cap

- **File:** `src/webdav.rs:285-296` (`list_dir` per-child `tokio::fs::read`), `src/server.rs:862-868` (single-file PROPFIND)
- **Description:** Accepted cost per 15-CONTEXT ("hash-on-read per file is accepted"), but worth recording: a PROPFIND on a large directory reads every file into RAM (no cap; PUT caps at 8 MB but on-disk files are unbounded), so one Depth:1 request can spike memory proportional to the whole tree. Fine for markdown vaults; revisit if vaults ever hold binaries.

### REV-15-09 — INFO — `precondition_of` accepts the empty quoted string `""` as a valid tag

- **File:** `src/webdav.rs:418-448`
- **Description:** `If-Match: ""` parses as `Tags(["\"\""])` (len 2, quoted, no inner quote). It is a legal RFC quoted-string and harmless (never matches a sha256 tag → always 412), but it is a degenerate validator; rejecting empty tag bodies would be stricter. Fail-closed contract (Malformed → 400, never Absent) is otherwise correctly implemented, including non-UTF-8 header bytes.

### REV-15-10 — INFO — Conflict JSON `server_etag` unused by the JS renderer; D-07 link is the generic stats page

- **File:** `src/server.rs:3503-3510` (payload), `src/server.rs:2140-2160` (`renderConflict`)
- **Description:** The 409 contract is internally consistent (`status`/`vault`/`conflicts[].path|server_etag|changes[].ts|kind|actor` all match between server and JS), but the JS never reads `server_etag` (reserved for a future If-Match retry), and the per-file link points to `/vault/{slug}/stats` rather than a per-path anchor, so "who/when" context requires manual scanning on the stats page.

## Checked and clean

- **Fail-closed preconditions (CONF-02, PITFALLS C2):** `precondition_of` returns `Malformed` for weak `W/`, bare tokens, empty values, and non-UTF-8 bytes; the PUT handler 400s before reading the body and after the `can_write` gate (`src/server.rs:926-933`); tests cover weak-etag 400 with nothing written.
- **CONF-05 regressions:** unconditional PUT still 201 (test `put_unconditional_update_still_201_no_etag_required`); plain new-vault upload path byte-identical behavior (exclusive `create_dir`, `fail_upload` semantics preserved — `work_dir` aliases `dir` on the new-vault path); git-sourced and foreign vaults keep the "already taken" refusal.
- **Lock usage (WebDAV PUT):** check→write fully under `vault_lock`; guard dropped before `rebuild_book` and before the audit spawn; std Mutex guards only the map, never held across await.
- **Atomic writes:** `write_source_atomic` sibling-temp + rename, temp removed on every error path, ETag recomputed from the write payload.
- **SQL:** all new queries (`log_vault_change`, `vault_changes_for_path`) use bound parameters only; migration 0011 is additive with defaults, CHECK on `kind` untouched.
- **Path traversal:** update path reuses `sanitize_upload_name` + canonicalized `starts_with(root)` defense against the staging root; traversal test proves refusal before any live write; apply loop only walks real paths under staging.
- **XSS in new home-page JS:** every dynamic value (paths, actors, error text) rendered via `textContent`/`createElement`; the only `href` is built from the server-slugified slug with a fixed `/vault/` prefix; no `innerHTML`.
- **409 contract:** server JSON and JS renderer field-for-field consistent; staging cleaned on the 409 path (test-verified).
- **Locked accounts:** server-side `is_locked` gate at the upload entry point (`src/server.rs:3170`) precedes the JS popup — no security regression from removing the form.
- **Actor/forced plumbing:** WebDAV PUT records the Basic-auth email with `forced=false`; refresh path records `""`/`false`; one `event_id` per operation (test-verified).
