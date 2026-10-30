---
phase: 15
plan: 01
subsystem: webdav
tags: [etag, sha256, webdav, propfind, http-preconditions, axum]

requires:
  - phase: 6 (WebDAV surface)
    provides: PROPFIND/GET/PUT handlers, safe_join, multistatus rendering
  - phase: 13 (wiki changes log)
    provides: vault source semantics this hash covers
provides:
  - "webdav::etag_of(bytes: &[u8]) -> String — quoted sha256-hex strong ETag helper (15-03/15-04 reuse)"
  - "webdav::Precondition enum + if_match_of / if_none_match_of fail-closed parsers (15-03 enforces)"
  - "ETag header on WebDAV GET; <D:getetag> per file in PROPFIND"
affects: [15-03, 15-04, phase-16-rest-api]

tech-stack:
  added: []
  patterns:
    - "One hash, three consumers: single etag_of() helper is the only sha256+quote site"
    - "Fail-closed precondition parsing: Malformed never degrades to Absent"

key-files:
  created: []
  modified:
    - src/webdav.rs
    - src/server.rs

key-decisions:
  - "etag_of returns the QUOTED wire form \"<sha256-hex>\" — one source of truth for quoting (planner decision, followed)"
  - "PROPFIND hash-on-read per file, no caching (CONTEXT 'Claude's Discretion')"
  - "Parser tokens must be plain quoted strings; inner-quote smuggling (\"a\"b\") also refused"

requirements_completed: [CONF-01]
requirements-completed: [CONF-01]

one_liner: "Strong sha256 content ETags on WebDAV GET and PROPFIND getetag via a single quoted etag_of() hash helper, plus fail-closed If-Match/If-None-Match parsers ready for 15-03 enforcement"

duration: ~12min
completed: 2026-10-30
---

# Phase 15 Plan 01: etag-core Summary

**Strong sha256 content ETags on WebDAV GET + `<D:getetag>` in PROPFIND (CONF-01), one `etag_of()` hash helper, and tested fail-closed `If-Match`/`If-None-Match` parsers for 15-03.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-10-30T12:17Z (approx)
- **Completed:** 2026-10-30T12:29Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `pub fn etag_of(bytes: &[u8]) -> String` in `src/webdav.rs` (line 31) — returns the QUOTED wire form `"<sha256-hex>"`; the single hash+quote source of truth for GET, PROPFIND, and the 15-03/15-04 consumers ("one hash, three consumers"). Strong only: never `W/`, never mtime (C1/C2).
- WebDAV GET emits `ETag: "<sha256-hex>"` on the raw source bytes, after the existing `can_read` authz gate (authz-before-emission preserved).
- PROPFIND: `etag: Option<String>` threaded through `prop_entry` / `Entry` / `multistatus` / `list_dir` / `file_propfind_response`; `<D:getetag>` emitted inside `<D:prop>` for files only (hash-on-read per file; collections carry none). Single-file depth-0 PROPFIND reads bytes and passes `Some(etag)`.
- `pub enum Precondition { Absent, Any, Tags(Vec<String>), Malformed }` + `pub fn if_match_of` / `pub fn if_none_match_of` (webdav.rs:419/425) — pure, total, fail-closed: invalid UTF-8 header bytes → Malformed (never Absent); `W/` weak tags, bare tokens, empty values, mixed valid+weak lists → Malformed. Tags keep quotes for direct comparison against `etag_of` output.

## Task Commits

1. **Task 1: etag-helper-get-propfind-emission** — `da6093d` (test, RED), `5c713ab` (feat, GREEN)
2. **Task 2: conditional-header-parsers** — `db6255a` (test, RED), `2b07166` (feat, GREEN)
3. **Formatting** — `1c1ea91` (style: rustfmt on new code)

**Plan metadata:** this commit (docs: complete plan)

## TDD Evidence

- **RED 1:** `cargo test --lib etag_of` → compile failure, `cannot find function etag_of` (3 errors). **GREEN 1:** `etag_of_is_strong_sha256`, `get_returns_strong_etag`, `propfind_includes_getetag_for_files_only`, `multistatus_marks_dirs_and_sorts_them_first` (extended), all `propfind*`/`list_dir*` tests pass.
- **RED 2:** `cargo test --lib if_match` → compile failure, `Precondition`/`if_match_of`/`if_none_match_of` not found (23 errors). **GREEN 2:** `if_match_parses_per_rfc9110`, `if_none_match_parses_per_rfc9110` pass (table-driven incl. `W/"abc"` → Malformed and non-UTF-8 bytes → Malformed).

## Acceptance Criteria (all PASS)

- `grep -n "pub fn etag_of" src/webdav.rs` → line 31 ✓
- GET ETag matches `^"[0-9a-f]{64}"$` and equals `etag_of(file bytes)` — asserted in `get_returns_strong_etag` ✓
- PROPFIND Depth:1 has `<D:getetag>` per file, none on collections — asserted in `propfind_includes_getetag_for_files_only` + multistatus unit test (count == 1) ✓
- No `W/` string in any new response code path (grep clean) ✓
- `grep -n "pub fn if_match_of\|pub fn if_none_match_of"` → both match (419/425) ✓
- `cargo test --lib etag_of` / `propfind` / `if_match` / `if_none_match` pass ✓
- Full `cargo test --lib`: **339 passed, 0 failed, 4 ignored** ✓
- `cargo clippy --all-targets`: no warnings ✓; `cargo fmt --check`: clean ✓
- Manual curl PROPFIND check covered equivalently by the e2e req-helper test (same handler path)

## Files Created/Modified
- `src/webdav.rs` — `etag_of()` helper; `Precondition` enum + `if_match_of`/`if_none_match_of`; `etag: Option<String>` threaded through prop_entry/Entry/multistatus/list_dir/file_propfind_response; 3 new tests + extended multistatus test
- `src/server.rs` — GET branch adds `ETAG` header (after can_read gate); single-file PROPFIND branch hashes bytes; 2 new e2e tests

## Decisions Made
- Followed all planner discretion decisions verbatim (quoted wire form, hash-on-read, no strict-mode flag).
- Parser additionally rejects quoted tokens containing inner quotes (`"a"b"`) — cheap hardening beyond the plan's token rules, still Malformed.

## Deviations from Plan

None — plan executed exactly as written. (One test-helper lifetime fix during RED phase: `HeaderName::try_from` instead of `&str` insert, part of the test commit.)

## Issues Encountered
- `git add -u` during the fmt commit swept a pre-existing orchestrator modification of `.planning/STATE.md`; the commit was reset and redone without it. STATE.md remains untouched by this plan (still uncommitted for the orchestrator).

## Next Phase Readiness
- **15-03:** `webdav::etag_of`, `webdav::Precondition`, `webdav::if_match_of`, `webdav::if_none_match_of` are public in `src/webdav.rs` — evaluate after `access.can_write()` + `safe_join`, before body read; Malformed → 400, mismatch → 412 naming the current ETag.
- **15-04:** upload conflict compare reuses `etag_of` on incoming vs current bytes (D-02 sha256 compare).
- No blockers.

## Self-Check: PASSED

---
*Phase: MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05*
*Completed: 2026-10-30*
