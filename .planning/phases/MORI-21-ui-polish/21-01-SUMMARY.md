---
phase: 21-ui-polish
plan: 21-01
subsystem: ui
tags: [okf, serde, details-disclosure, html-escape, axum]

requires:
  - phase: 18-okf-conformance
    provides: OkfStats scan + stored okf_json blob, defensive badge serde (T-18-03)
  - phase: 19-statistics-observability
    provides: zero-new-CSS rule (D-03) — badge panel built from existing small/ul/li + .menu-btn
provides:
  - OkfStats extended with capped non_conformant path list + misses_overflow (backward-compatible serde)
  - okf_badge_html is now a shared in-flow <details> disclosure: conformant panel (version, concepts/total, index.md/log.md presence, scan date, escaped capped path list, "+N more") and not-a-bundle reason list (0 concepts / below 80 % ratio / missing index.md / missing log.md)
affects: [21-02, 21-03, future badge/vault_row work, okf_json consumers]

tech-stack:
  added: []
  patterns: ["persisted-JSON struct extension via #[serde(default)] fields", "in-flow <details><summary class=menu-btn> disclosure (no .menu/.menu-body)"]

key-files:
  created: []
  modified: [src/okf.rs, src/server.rs]

key-decisions:
  - "Plain <details> without class menu/menu-body (those are absolutely positioned for the header)"
  - "Panel built only from small/ul/li + .menu-btn — zero new CSS (Phase 19 D-03)"
  - "server.rs okf_stats test helper updated in the Task 1 commit (compile requirement), not Task 2"

patterns-established:
  - "okf_json field extension: new fields carry #[serde(default)]; old rows deserialize to empty list / 0"
  - "Every blob-sourced string (paths, version) reaches HTML only via html_escape (T-21-01)"

requirements-completed: [UI-01]
requirements_completed: [UI-01]

duration: ~25min
completed: 2026-11-22
one_liner: "OKF badge pills are now clickable details disclosures showing pass/fail concepts, reasons, and a capped escaped list of non-conformant files from the stored scan"
---

# Phase 21 Plan 01: OKF badge conformance details Summary

**OKF badge pills (conformant and not-a-bundle) are clickable inline `<details>` disclosures rendering version, concepts/total, index.md/log.md presence, scan date, failing reasons, and a capped (200 + "+N more") html-escaped list of non-conformant files — all from the stored `okf_json` scan, old rows still rendering.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-11-22T08:40Z (approx, first edit)
- **Completed:** 2026-11-22T09:03Z
- **Tasks:** 2
- **Files modified:** 2 (+ this SUMMARY)

## Accomplishment
- `scan_conformance` now records relative paths of readable `.md` files lacking a non-empty `type:` frontmatter, capped at `MAX_MISSES = 200` with `misses_overflow` beyond the cap (D-01, T-21-02); unreadable/non-UTF-8 files keep counting toward `total` only.
- `OkfStats` grew `#[serde(default)] non_conformant: Vec<String>` + `#[serde(default)] misses_overflow: usize` — pre-phase `okf_json` rows deserialize unchanged (D-01 backward-compat).
- `okf_badge_html` (the single shared component threaded through `vault_row`, covering both home own/shared call sites — D-03) returns a `<details>` with `<summary class="menu-btn">` in both arms; the not-a-bundle arm lists reasons per D-04.
- Single persist site (`record_okf_scan` → `store.set_vault_okf`) untouched; no re-scan at page render (D-01).

## Task Commits

1. **Task 1: Record non-conformant paths in the scan (D-01)** — `be6ec18` (feat(21-01))
2. **Task 2: okf_badge_html shared details disclosure (D-02/D-03/D-04)** — `255c0ab` (feat(21-01), amended once to fold in rustfmt reflow)

## Files Created/Modified
- `src/okf.rs` — `MAX_MISSES = 200`; `OkfStats` +2 serde-default fields; scan loop else-branch records capped misses; 3 new tests
- `src/server.rs` — `okf_badge_html` rewritten as `<details>` component (both arms); `okf_stats` test helper extended; 4 new badge tests + 2 existing tests strengthened
- No `.css` file touched (`git diff --stat -- '*.css'` empty — zero-new-CSS rule honored)

## TDD RED/GREEN evidence

Task 1 (tests written first):
- RED: `cargo test --lib okf::` failed to compile — `error[E0609]: no field 'misses_overflow' on type 'okf::OkfStats'` (+ E0425 `MAX_MISSES` not found, 9 errors total).
- GREEN: `cargo test --lib okf::` → 31 passed / 0 failed, including `scan_records_non_conformant_paths`, `scan_misses_capped_at_max_with_overflow`, `old_okfstats_json_without_new_fields_deserializes`.

Task 2 (tests written first):
- RED: `cargo test --lib okf_badge` → 6 failed / 4 passed — `okf_badge_html_escapes_nonconformant_paths`, `okf_badge_html_overflow_shows_plus_n_more`, `okf_badge_html_not_bundle_explains_reasons`, `okf_badge_html_old_json_renders` (new) plus the 2 strengthened existing tests (old static-pill markup lacked `<summary class="menu-btn">` / `&lt;script&gt;` / `+5 more` / reason lines).
- GREEN: `cargo test --lib okf_badge` → 10 passed / 0 failed.

## Gate results
- Full `cargo test`: **618 passed / 0 failed / 4 ignored** (baseline 611 + 7 new tests). No flakes observed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- `grep -c scan_conformance src/server.rs` = 1 (single persist call site unchanged).

## Deviations from Plan

1. **`okf_stats` test helper (src/server.rs) updated in the Task 1 commit instead of Task 2.** The plan assigned the helper update to Task 2, but the crate cannot compile with the extended `OkfStats` until the struct literal matches — Task 1's GREEN was impossible without it. Same content the plan prescribed (empty vec / 0).
2. **The two "exact-string" tests were already `contains`-based**, not whole-string equality as the plan assumed. They were instead strengthened to assert the new `<summary class="menu-btn">` / `<details` markup (their old assertions would have passed unchanged on the new markup, so the strengthening is what makes them real coverage of D-02).
3. **rustfmt reflow of one `if/else` format arg** after the Task 2 commit — folded into the same commit via amend (nothing pushed).

No out-of-plan files touched. STATE.md / ROADMAP.md intentionally not updated (orchestrator owns them).

## Decisions
- In-flow plain `<details>` (no `menu`/`menu-body` classes) exactly per plan note — those classes are absolutely positioned for the header.
- Conformant panel line: `version {v} · {c}/{t} concepts · index.md: present|missing · log.md: present|missing` + `scanned {YYYY-MM-DD}` via `epoch_secs_to_utc_date` (no chrono).
- Scope decision (per plan): the badge exists only in `vault_row` (home own/shared lists); `/vault/{slug}/` serves raw mdBook HTML with no per-vault DB data in the serve path (21-PATTERNS "No-analog gaps" #2), so D-03's "everywhere the badge exists" is satisfied by the single shared component covering both call sites. Injecting a badge into served book pages is out of scope.
- Threat model honored: T-21-01 (all paths via `html_escape`, test-locked), T-21-02 (MAX_MISSES cap), T-21-03 (serde-default + existing defensive parse kept).

## Issues Encountered
None. No load flakes in the full parallel suite.

## Next Phase Readiness
- 21-02 / 21-03 (Looks picker, Vault Edit realignment) unaffected; badge data contract is now `OkfStats` v2 (two optional keys).
- Any future consumer of `okf_json` must keep `#[serde(default)]` on new fields to preserve old-row compatibility.

---
*Phase: 21-ui-polish*
*Completed: 2026-11-22*
