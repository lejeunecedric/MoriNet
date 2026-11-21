---
status: passed
phase: 15
score: 7/7
date: 2026-10-30
---

# Phase 15 Verification: etag-conflict

**Verification method:** each ROADMAP success criterion checked against live code
(`src/webdav.rs`, `src/server.rs`, `src/store.rs`, `migrations/0011*.sql`) and
proven by running the targeted `cargo test --lib` filters. Full suite at HEAD
(`ab052cd`): **359 passed / 0 failed**.

## Per-criterion verdicts

### 1. WebDAV GET carries strong ETag = sha256 of raw SOURCE bytes — PASS (CONF-01)
- `webdav::etag_of` (`src/webdav.rs:31`) — sha256 + hex, returns the QUOTED wire
  form `"<hex>"`; strong only, never `W/`, never mtime (C1/C2). Single hash site
  for GET, PROPFIND, and precondition compares ("one hash, three consumers").
- GET branch hashes the raw bytes read from the **source** dir
  (`source_dir_of` unwraps `.morinet-build` so WebDAV never touches build
  output): `src/server.rs:892-907` (`etag_of(&bytes)` → `ETAG` header).
- Tests (run, green): `webdav::tests::etag_of_is_strong_sha256` (quoted, 64
  lowercase hex, no `W/`, equals sha256 of known bytes),
  `server::tests::get_returns_strong_etag` (asserts header matches
  `^"[0-9a-f]{64}"$` AND equals `etag_of` of the raw source file bytes).

### 2. PROPFIND includes getetag per file — PASS (CONF-01)
- `prop_entry` emits `<D:getetag>` only when `Some` (files yes, collections no):
  `src/webdav.rs:200-213`; `list_dir` hashes each file child; single-file
  PROPFIND passes the hash too (`src/server.rs:864-877`).
- Tests (run, green): `server::tests::propfind_includes_getetag_for_files_only`
  (file entry's `<D:getetag>` equals sha256 of the file bytes; collection
  entries carry none), `webdav::tests::multistatus_marks_dirs_and_sorts_them_first`.

### 3. PUT non-matching If-Match → 412, nothing written / no rebuild / no audit row; malformed → 400 — PASS (CONF-02)
- Malformed precondition fails closed to 400 **before the body is read**, after
  authz: `src/server.rs:928-934`; parser `precondition_of`
  (`src/webdav.rs:418-448`) never degrades garbage/weak tags to `Absent`.
- Mismatch → `precondition_failed` (`src/server.rs:964-967`, body names the
  current ETag at `:1024-1038`); the return precedes the write (`:978`), the
  audit spawn (`:988-1003`) and `rebuild_book` (`:1006`) — structurally nothing
  can be written, logged, or rebuilt on 412.
- Tests (run, green): `put_if_match_mismatch_412_names_current_etag` (412 + body
  names current ETag + file unchanged + polled `vault_changes_for_path` shows no
  new row), `put_if_match_star_absent_file_412`,
  `put_malformed_if_match_weak_etag_400` (`W/"x"` → 400, nothing written),
  `webdav::tests::if_match_parses_per_rfc9110` / `if_none_match_parses_per_rfc9110`.

### 4. If-None-Match: * create-only; successful conditional PUT returns new ETag — PASS (CONF-03)
- `If-None-Match: *` rejects when the file exists (`src/server.rs:959-963`);
  every success returns the new ETag header (`:1015`), conditional update → 200,
  create → 201 (`:1010-1014`).
- Tests (run, green): `put_if_none_match_star_exists_412` (nothing written),
  `put_if_none_match_star_absent_creates_201` (201 + ETag == `etag_of(body)`),
  `put_if_match_match_update_returns_200_with_new_etag` (200 + new ETag + file
  updated).

### 5. Stale upload tree → per-file conflict list; force-overwrite recorded forced=1 — PASS (CONF-04)
- Migration `migrations/0011_vault_changes_forced_actor.sql`: additive
  `forced INTEGER NOT NULL DEFAULT 0` + `actor TEXT NOT NULL DEFAULT ''`;
  `Store::log_vault_change` 6-arg (`src/store.rs:1441-1459`),
  `vault_changes_for_path` newest-first (`src/store.rs:1468`).
- `finish_upload_update` (`src/server.rs:3461-3588`): per-file sha256 compare
  via `etag_of` (`:3486-3491`); conflicts + no force → 409 JSON
  `{status,vault,conflicts[path,server_etag,changes[ts,kind,actor]]}` with
  who/when from `vault_changes_for_path` (`:3493-3527`); force path writes via
  the shared helper and logs `modified`/`added` rows with `forced = force_flag`
  and `actor = claims.email`, one `event_id` per operation (`:3562-3577`).
- Tests (run, green): `upload_update_conflict_returns_409_json` (409, path +
  server ETag in JSON, no partial write, staging cleaned),
  `upload_update_force_overwrites_and_records` (force → 303, file overwritten,
  `modified` row with `forced=1` + actor, shared event_id),
  `upload_update_adds_local_only_keeps_server_only`,
  `store::vault_changes_tests::vault_changes_forced_actor_defaults` /
  `_roundtrip` / `vault_changes_for_path_newest_first`.
- UX half (15-05): `home_page_embeds_two_phase_upload_js` green.

### 6. Unconditional writes behave exactly as before — PASS (CONF-05)
- No precondition headers → `Precondition::Absent` path: unconditional update
  keeps the historical **201** (`src/server.rs:1010-1014`); plain new-vault
  upload path untouched by the gate (`update_mode` only for owned uploaded
  vaults; quota/exclusive-create blocks skipped only in update mode).
- Tests (run, green): `put_unconditional_update_still_201_no_etag_required`,
  `upload_plain_new_vault_regression`, plus the whole Phase-14 upload regression
  set (`upload_registers_personal_vault`, `upload_quota_enforced_before_disk`,
  etc.) green in the full suite.

### 7. Check→write race-safe: per-vault lock + ONE shared atomic-write helper — PASS (C3/C5)
- `webdav::write_source_atomic` (`src/webdav.rs:47-69`): sibling temp file
  `.{name}.tmp-{uuid}` in the same dir → `tokio::fs::rename` (atomic, same
  filesystem); temp removed on any error; returns ETag recomputed from the
  written bytes. **Used by both** WebDAV PUT (`src/server.rs:978`) and the
  upload update apply loop (`src/server.rs:3552`) — one helper, no second write
  path.
- `AppState::vault_locks` + `vault_lock` (`src/server.rs:41-56`): per-vault
  `tokio::sync::Mutex` keyed by source dir; std Mutex guards only the map,
  never held across await. Whole check→write section runs under the guard in
  both paths (`:940-982`, `:3533-3557`); lock released before rebuild.
- Test (run, green): `write_source_atomic_roundtrip` (content + returned ETag,
  overwrite roundtrip, no temp leftovers).

## Requirement traceability

| Req | Plan | Code | Tests (all run green) |
|---|---|---|---|
| CONF-01 | 15-01 | webdav.rs:31 `etag_of`, :200 getetag; server.rs:896 GET, :867 single-file PROPFIND | `etag_of_is_strong_sha256`, `get_returns_strong_etag`, `propfind_includes_getetag_for_files_only` |
| CONF-02 | 15-03 | server.rs:928-934 (400), :955-967 (412), :1024 body; webdav.rs:418 fail-closed parser | `put_if_match_mismatch_412_names_current_etag`, `put_if_match_star_absent_file_412`, `put_malformed_if_match_weak_etag_400`, `if_match_parses_per_rfc9110` |
| CONF-03 | 15-03 | server.rs:959-963, :1010-1015 | `put_if_none_match_star_exists_412`, `put_if_none_match_star_absent_creates_201`, `put_if_match_match_update_returns_200_with_new_etag` |
| CONF-04 | 15-02, 15-04, 15-05 | migrations/0011; store.rs:1441, :1468; server.rs:3461-3588; LOCAL_UPLOAD_JS | `vault_changes_forced_actor_defaults/_roundtrip`, `vault_changes_for_path_newest_first`, `upload_update_conflict_returns_409_json`, `upload_update_force_overwrites_and_records`, `upload_update_adds_local_only_keeps_server_only`, `home_page_embeds_two_phase_upload_js` |
| CONF-05 | 15-03, 15-04 | server.rs:1010-1014 (Absent → 201), update_mode gating leaves new-vault path untouched | `put_unconditional_update_still_201_no_etag_required`, `upload_plain_new_vault_regression`, Phase-14 upload regression set |

## Suite result

- Targeted filters above: all green.
- Full suite: `cargo test --lib` → **359 passed / 0 failed**.

## Human verification

1. **Real-client smoke test (CONF-05, ROADMAP criterion 6 "rclone/Obsidian
   regression green")**: automated tests prove the server-side contract
   (unconditional PUT → 201, plain upload unchanged), but no automated test
   drives a real rclone or Obsidian WebDAV client. Manual smoke: `rclone copy`
   a folder to a `/webdav/<slug>/` mount and edit+save a note from Obsidian via
   a WebDAV bridge — both must behave exactly as pre-Phase-15 (no 412s, no
   client errors). This is the only open item; all automated criteria pass.
   **RESOLVED 2026-10-30: user approved phase completion; the rclone/Obsidian
   smoke test is deferred to the operator and tracked in 15-HUMAN-UAT.md.**
