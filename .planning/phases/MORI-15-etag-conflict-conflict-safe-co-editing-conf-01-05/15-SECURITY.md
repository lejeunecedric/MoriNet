---
phase: 15
slug: etag-conflict-conflict-safe-co-editing-conf-01-05
status: verified
threats_open: 0
asvs_level: 1
created: 2026-10-30
---

# Phase 15 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| WebDAV client → server | HTTP Basic + ACL on GET/PROPFIND/PUT of vault source markdown | user content, ETags, precondition headers |
| Browser (home page JS) → /vaults/upload | Cookie-authed multipart upload + two-phase fetch conflict flow | file trees, conflict JSON (paths, ETags, actor emails) |
| Authz gate → content emission | can_read/can_write/can_edit_vault must precede any content or hash disclosure | ETags, 412 bodies, conflict JSON, stats pages |
| Store layer | sqlx bound-parameter queries only; migration 0011 additive ALTERs | vault_changes audit rows (actor emails, forced flags) |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-15-01 | Tampering | WebDAV GET/PROPFIND read paths | mitigate | `webdav::safe_join` unchanged on all read paths; hash takes already-joined bytes (webdav.rs:31,262); traversal tests green | closed |
| T-15-02 | Information disclosure | ETag emission on GET | mitigate | `can_read` gate (server.rs:840) strictly precedes ETag emission (:896, :867-870) | closed |
| T-15-03 | Denial of service | PROPFIND hash-on-read | accept | O(total bytes) bounded by 5 MiB/file + 8 MiB PUT caps + quotas (server.rs:3075,969,3222) — see Accepted Risks | closed |
| T-15-04 | Tampering | If-Match/If-None-Match parsers | mitigate | Total pure parsers; garbage/non-UTF-8 → Malformed → 400, never Absent (webdav.rs:418-460); table tests incl. non-UTF-8 | closed |
| T-15-05 | Information disclosure | `actor` column (emails) | mitigate | Stats page ACL-gated and renders no actor (server.rs:2783-2800); emails only in owner-only conflict JSON (:3543) | closed |
| T-15-06 | Integrity | Migration 0011 | mitigate | Two additive `ALTER … NOT NULL DEFAULT` only, no table rebuild; defaults roundtrip tests pass | closed |
| T-15-07 | Injection | `vault_changes_for_path` / `log_vault_change` | mitigate | Bound parameters only, no `format!` SQL (store.rs:1441-1483) | closed |
| T-15-08 | Tampering | Precondition bypass on PUT | mitigate | Gate order can_write → safe_join → Malformed 400 → lock → on-disk hash compare → 412 before any write (server.rs:915-968); tests assert bytes unchanged + no audit row | closed |
| T-15-09 | Repudiation/Race | Concurrent PUTs (TOCTOU) | mitigate | Per-vault tokio Mutex across check→write (server.rs:41-53,940-982); `write_source_atomic` temp+rename (webdav.rs:47-68) | closed |
| T-15-10 | Information disclosure | 412 body names current ETag | mitigate | Reachable only after `can_write` passed (:915 precedes :928) — requester already holds read | closed |
| T-15-11 | Tampering | PUT temp filename | mitigate | Temp name `.{file_name}.tmp-{uuid}` from sanitized file_name in same dir; never leaves src_dir (webdav.rs:49-54) | closed |
| T-15-12 | Denial of service | Lock contention / huge bodies | mitigate | 8 MiB body cap; lock released before `rebuild_book` (:982 vs :1006); lock-across-body-read accepted as LOW — see Accepted Risks | closed |
| T-15-13 | Tampering | Traversal via upload parts (update path) | mitigate | `sanitize_upload_name` + `parent.canonicalize().starts_with(staging_root)` before any write (server.rs:3326-3346); test `upload_update_rejects_traversal_path` | closed |
| T-15-14 | Spoofing / elevation | Ownership bypass on upload-update | mitigate | Post-fix 25290c9: `vault_source==None` AND `can_edit_vault` (admin OR owner_id) gate (server.rs:3205-3218); locked gate first; other-user + git-vault refusal tests pass | closed |
| T-15-15 | Elevation | `force=1` as precondition bypass | mitigate | Force used ONLY at conflict gate (:3501,:3531); locked/ownership/sanitization gates all precede it | closed |
| T-15-16 | Information disclosure | Conflict JSON (paths/ETags/emails) | mitigate | Emitted only under `update_mode` (can_edit_vault); non-owners get plain "already taken" error (:3251-3254) | closed |
| T-15-17 | Denial of service | Staging disk exhaustion | mitigate | 2000×5 MiB caps; `cleanup_staging`/`abort_staging` on every exit path; no-leftovers assertions in 6 tests; disconnect leak accepted as LOW | closed |
| T-15-18 | Race (TOCTOU) | Conflict compare vs live write | mitigate | Post-fix 2be4bd9: scan + force gate + apply under ONE `vault_lock` (server.rs:3481-3530); 409 JSON built after guard drop; residual team-scale cross-order race accepted — see Accepted Risks | closed |
| T-15-19 | Elevation (XSS) | Conflict list rendering in home JS | mitigate | All dynamic values via `createElement`+`textContent` `el()` helper (server.rs:2099-2160); sole innerHTML use is a static `ul.innerHTML = ""` clear | closed |
| T-15-20 | Spoofing (CSRF) | Cross-origin fetch to /vaults/upload | mitigate | Same-origin relative fetch mirrors existing cookie form POST; server-side gates unchanged — no weaker surface than Phase 14 | closed |
| T-15-21 | Tampering | Traversal via client-derived name/filename | mitigate | `webkitRelativePath` only shapes multipart filename; server sanitization rejects traversal before write (:3326-3347); traversal tests on both upload paths | closed |

*Status: open · closed*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-15-01 | T-15-03 | PROPFIND hash-on-read is O(total bytes); bounded by 5 MiB/file + 8 MiB PUT caps + per-group quotas; sha256 at GB/s — accepted per CONTEXT discretion ("hash-on-read is acceptable") | CONTEXT.md D-discretion | 2026-10-30 |
| AR-15-02 | T-15-18 | Residual cross-order race between separate processes at team scale — single-binary in-process lock is sufficient per pitfall C3 (FEATURES.md complexity note) | plan 15-04 register | 2026-10-30 |
| AR-15-03 | T-15-12 | Per-vault lock held across the (capped 8 MiB) body read — slow-client starvation possible; advisory review LOW-01 accepted | code review 15-REVIEW.md | 2026-10-30 |
| AR-15-04 | — | Review LOWs accepted: If-None-Match tag-list update returns 201 not 200; forced=1 stamped on audit rows without actual conflict; partial apply without rollback on mid-apply failure; staging dir leak on client disconnect (no sweep) | code review 15-REVIEW.md | 2026-10-30 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-30 | 21 | 21 | 0 | gsd-security-auditor (verify-only, register authored at plan time) |

Verification was run against HEAD `f6255a8` including post-plan review fixes `25290c9` (REV-15-01 ownership gate → `can_edit_vault`) and `2be4bd9` (REV-15-02 conflict scan under `vault_lock`) — both confirmed live. Evidence: per-threat file:line citations + targeted test runs (webdav 32, upload_update 9, traversal 12, vault_changes 8, precondition 7, +6 others — 0 failures).

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-30
