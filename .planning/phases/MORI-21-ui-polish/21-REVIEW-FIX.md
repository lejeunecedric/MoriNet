---
phase: MORI-21-ui-polish
status: all_fixed
date: 2026-11-22
baseline: 620 passed / 0 failed / 4 ignored (HEAD a6402b0)
result: 623 passed / 0 failed / 4 ignored
---

# Phase 21 Review Fix Report

All five findings from `21-REVIEW.md` fixed. TDD observed for the two
test-backed findings (RED failure recorded before each GREEN fix).
`cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean.

| Finding | Severity | Disposition | Commit |
|---|---|---|---|
| 1. Looks pick handler lets mdBook's popup-level click listener also fire, clobbering `localStorage['mdbook-theme']` with the invalid `morinet-looks-*` value | HIGH | Fixed — pick handler now takes the event and calls `ev.preventDefault(); ev.stopPropagation();` before the localStorage write. Regression test `morinet_js_looks_picker_stops_event_propagation` asserts (on the embedded `MORINET_JS` bytes) that `stopPropagation` appears before `setItem('mdbook-theme'` inside the pick handler body. RED observed: "pick handler must call e.stopPropagation()". | test: `266c56f` / fix: `10d6e0f` |
| 2. "Looks" header `<li>` lacks `role="none"` inside `ul[role="menu"]` | LOW | Fixed — `head.setAttribute("role", "none");` added to match mdBook's own popup items. | `c23f159` |
| 3. `window.location.reload()` runs even when `localStorage.setItem` throws | LOW | Fixed — a `saved` flag is set inside the try block; reload only fires on successful write. | `d8ac41e` |
| 4. `MAX_MISSES` caps the count but not the length of recorded non-conformant paths (hostile long paths could bloat one `okf_json` row) | LOW | Fixed — new `MAX_MISS_PATH_CHARS = 200` + `clip_miss_path()` (`chars().take`, char-safe) applied at the push site. Tests: `scan_truncates_overlong_miss_paths` (end-to-end, ~1500-char multibyte path through `scan_conformance`; RED observed: "must be truncated to 200 chars, got 1519") and `clip_miss_path_truncates_5000_char_path_char_safely` (direct 5000-char unit test — the filesystem PATH_MAX of 4096 bytes blocks creating a real 5000-char path, so the 5000-char proof lives at the unit level). | test: `1ecb08d` / fix: `a38a445` |
| 5. Body-read caps of exactly 65536 in `admin_reads_vault_files` brittle against chrome growth | LOW | Fixed — both caps raised to `1 << 20` (test-only change). | `a57b99e` |

## Notes

- Byte-level test is the required floor for finding 1 (per review); no
  browser fixture against mdBook's real `book.js` was spun up.
- Out-of-scope items untouched: duplicate-palette-list (plan-intended,
  D-05), `aria-current` staleness, `STATE.md`/`ROADMAP.md`/`21-REVIEW.md`.
- Full suite: 623 passed / 0 failed / 4 ignored (baseline 620 + 3 new tests).
