---
phase: 21
plan: "21-02"
name: looks-color-scheme-picker
subsystem: frontend (served mdBook chrome)
tags: [ui, mdbook, theme, localStorage, issue-17]
one_liner: "MoriNet 'Looks' picker injected into mdBook's own theme popup (#mdbook-theme-list), writing mdBook's own localStorage['mdbook-theme'] key + reload — zero new CSS, zero server state (D-05)"
requirements_completed: [UI-02]
affects: [assets/zoom.js, src/obsidian/tree.rs, src/server.rs]
tech-stack:
  patterns: [ES5-IIFE, build-once-guard, DOMContentLoaded-guard, defensive-try-catch-localStorage]
key-files:
  modified: [assets/zoom.js, src/obsidian/tree.rs, src/server.rs]
key-decisions:
  - "Picker anchors on #mdbook-theme-list (this mdBook 0.5.4 renders no static About button — the popup IS the settings menu); About anchor kept defensively for other versions"
  - "Theme application via localStorage write + location.reload() — never call into book.js internals (21-PATTERNS §5)"
  - "admin_reads_vault_files body-read cap raised 8192 → 65536 (existing test convention) because the inlined chrome grew past 8 KiB"
completed: "2026-11-22"
tasks: 2
files-touched: 3
---

# Phase 21 Plan 2: Looks color-scheme picker Summary

UI-02 (issue #17) shipped: every served mdBook page now gets a MoriNet-owned
"Looks" section inside mdBook's own theme popup offering the five mdBook
palettes (Light, Navy, Rust, Coal, Ayu). Picking one writes
`localStorage['mdbook-theme']` — mdBook's own key — and reloads, so book.js's
`get_theme()` applies it and the visitor's choice wins over the vault's
serve-time default (Phase 12 D-08/D-10). Nothing is stored server-side (D-05).

## What was built

1. **`assets/zoom.js`** — new self-contained ES5 IIFE (separate from the zoom
   widget IIFE), DOMContentLoaded-guarded, build-once-guarded on
   `#morinet-looks-light`:
   - Locates `#mdbook-theme-list`; returns silently if absent (older/newer
     mdBook or non-book page — zero console errors, verified).
   - Builds a `Looks` heading `<li>` + five
     `<li role="none"><button role="menuitem" class="theme">` items with ids
     `morinet-looks-{light,navy,rust,coal,ayu}` — the popup's own markup
     vocabulary, so **zero new CSS** (Phase 19 D-03 rule held; `git diff
     --stat -- '*.css'` empty).
   - Click: `try { localStorage.setItem('mdbook-theme', palette); } catch (e) {}`
     then `window.location.reload()`.
   - Insertion (D-05 "before About"): `#about-btn`, else a `#mdbook-menu-bar`
     button/link whose trimmed text is exactly "About"; if found with a
     parentNode, insert before it, else insert at the top of the popup.
   - Marks the saved choice with `aria-current="true"` (localStorage read in
     try/catch).
   - Delivery: converted vaults get it via `theme/morinet.js` at next rebuild
     (convert.rs:129); native vaults instantly via serve-time inline
     (`inject_vault_chrome`).
2. **`src/obsidian/tree.rs`** — `#[test] fn morinet_js_ships_looks_picker()`
   asserting the embedded `MORINET_JS` bytes contain `mdbook-theme`,
   `morinet-looks-light`, `mdbook-theme-list` (proves both delivery paths).
3. **`src/server.rs`** (deviation, see below) — `admin_reads_vault_files`
   body-read cap 8192 → 65536.

## TDD RED/GREEN evidence

- **RED** (commit `b5e031b`, before any zoom.js change):
  `cargo test --lib morinet_js_ships_looks_picker` →
  `FAILED ... panicked at src/obsidian/tree.rs:531: the picker must write mdBook's own localStorage key`
  (0 passed; 1 failed).
- **GREEN** (commit `7b29b61`): same command → `ok. 1 passed; 0 failed`.

## Tests

- `obsidian::tree::tests::morinet_js_ships_looks_picker` — new (RED→GREEN).
- `server::tests::admin_reads_vault_files` — pre-existing; broke from my asset
  growth, fixed (see Deviations), passes isolated and in the full suite.
- Full suite: **619 passed / 0 failed / 4 ignored** (baseline 618 + 1 new).
  No flakes observed.
- `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean;
  `scripts/check.sh` → `check: OK`.

## Functional verification (real browser, beyond plan's "optional")

mdBook-popup markup fixture loaded via CDP `Page.setDocumentContent`:
- Fresh page: "Looks" heading + 5 buttons (`role=menuitem`, `class=theme`,
  correct ids/labels) inserted at popup top before the Auto item; `window.onerror` collected zero errors.
- Clicked Ayu → `localStorage['mdbook-theme'] === 'ayu'` (survives reload).
- Reload with saved `ayu` → `aria-current="true"` on Ayu only.
- About-anchor variant: section inserted immediately before `#about-btn`, popup untouched.
- No-popup page: silent no-op, zero errors, localStorage untouched.

## Deviations from plan

1. **[Rule 1 - introduced bug] `src/server.rs` touched (out-of-plan file)** —
   Found during: full-suite gate. `admin_reads_vault_files` calls
   `to_bytes(res.into_body(), 8192)` on a served book page; the page inlines
   `MORINET_JS`, which grew 3.3 KiB → 6.8 KiB with the picker, so the read hit
   sqlx/axum `LengthLimitError` (failed isolated too — not a flake). Fix:
   raised the two caps in that test to 65536, the dominant convention already
   used by ~20 other call sites in the same test module. Verified: test green
   isolated + full suite green. Commit `a3eae89`.
2. **Acceptance-criteria tightening during Task 1 gate**: palette button ids
   were initially built by string concatenation, so only `morinet-looks-light`
   appeared as a literal in the file; the criteria demand all five
   `morinet-looks-*` literals. Fixed by listing the full ids in the PALETTES
   table. No extra commit (caught before the feat commit).

**Total deviations:** 1 auto-fixed out-of-plan file + 1 in-task correction.
**Impact:** none on locked decisions; the server.rs change is test-only.

## Decisions

- Anchored the picker on `#mdbook-theme-list` (the popup) rather than the menu
  bar, because mdBook 0.5.4 renders no static About button — the popup IS the
  settings menu the issue refers to (plan context §Ground truth). The About
  lookup is kept as a defensive insertion anchor for other mdBook versions.
- Reload-based theme application (zero coupling to book.js internals), per
  21-PATTERNS §5.
- Test-only cap raise to 65536 instead of shrinking the asset or special-casing.

## Lessons

- **Embedded-asset size is load-bearing for tests**: `inject_vault_chrome`
  inlines `MORINET_JS` into every served page, so any growth in
  `assets/zoom.js` counts against hardcoded `to_bytes(..., 8192)` caps in
  serve-path tests. Prefer the 65536 convention for full-page reads.
- A failing test that fails **isolated** is a real regression, not a load
  flake — the LengthLimitError pointed straight at the body cap.
- CDP `Page.setDocumentContent` on a real-origin tab is a fast way to
  functionally smoke-test injected chrome (localStorage works, unlike data:
  URLs) — worth it for any future zoom.js work.
- For acceptance greps demanding literal ids, put literals in the source table
  rather than building ids by concatenation.

## Ready for

21-03 (vault Edit page realignment, UI-03 / D-06). Note from 21-01 still
standing: zero-new-CSS rule — reuse existing classes only.
