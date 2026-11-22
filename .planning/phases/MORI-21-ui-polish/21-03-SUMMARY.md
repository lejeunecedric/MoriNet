---
phase: 21-ui-polish
plan: 21-03
name: vault-edit-realignment-and-docs
subsystem: ui
tags: [ui, form-layout, flex, docs, issue-13]
one_liner: "Vault Edit Theme select forced onto its own flex row (field field-wide) below the allow… checkboxes and above Save, plus user docs for the 21-01 badge detail and 21-02 Looks picker"
requirements_completed: [UI-03]
requirements-completed: [UI-03]
affects: [src/server.rs, docs/morinet-docs/src/sharing-and-roles.md]
key-files:
  modified: [src/server.rs, docs/morinet-docs/src/sharing-and-roles.md]
key-decisions:
  - "Markup-only fix: theme_field wrapper `field` → `field field-wide` (flex: 1 1 20rem forces its own row); no form restructure, zero new CSS (D-06, Phase 19 D-03)"
  - "Docs claims kept strictly to shipped behavior: Looks is browser-local only, no server-side per-reader preference (D-05)"
completed: "2026-11-22"
tasks: 2
files-touched: 2
---

# Phase 21 Plan 3: vault-edit-realignment-and-docs Summary

UI-03 (issue #13) done: on the Vault Edit page the Theme select now renders on
its own flex row — below the "allow…" checkbox line, directly above Save —
via a single class change (`field` → `field field-wide`) in the
`vault_edit_page` `theme_field` markup. The user-facing docs now describe the
21-01 badge click-detail and the 21-02 Looks picker.

## What was built

1. **Task 1 (src/server.rs)** — `theme_field` wrapper div changed from
   `class="field"` to `class="field field-wide"` (server.rs ~:2565).
   `.field-wide { flex: 1 1 20rem }` (assets/style.css:256) forces the Theme
   select onto its own row inside `form.stacked` (flex-wrap): Source URL owns
   row 1, the two checkbox fields share row 2, the wide Theme field takes
   row 3, Save wraps below it. Everything else byte-identical: same `sel()`
   idiom, same auto/light/navy/rust/coal/ayu options, same `{tf}` slot, same
   converted-vault gating, `vault_edit_submit` / `EditForm` / `THEMES`
   whitelist untouched (T-21-13 stays mitigated).
2. **Task 2 (docs/morinet-docs/src/sharing-and-roles.md)** — two short
   paragraphs in the existing sections:
   - *Vault display options*: the **Looks** section in the book menu's theme
     popup, remembered in the visitor's own browser, overriding the vault
     default until changed, never affecting other readers (no server-side
     preference claims — D-05).
   - *Statistics & wiki changes*: the **OKF badge** click-detail — version,
     concepts ratio, `index.md`/`log.md` presence, scan date, capped
     non-conformant file list with "+N more"; the "not an OKF bundle" pill
     explains which checks failed.

## Tests

- **New:** `server::tests::edit_page_theme_field_sits_below_checkboxes_above_save`
  — converted vault (wiki_repo fixture), GET `/vault/orderwiki/edit`, asserts
  source order `id="ev-reshare"` < `id="ev-keep"` <
  `class="field field-wide"><label for="ev-theme"` < `>Save</button>` (also
  locks the `field field-wide` mechanism).
- **Pre-existing green:** `edit_page_shows_display_options`,
  `edit_page_native_vault_hides_theme_select`, `edit_submit_persists_display_options`,
  `edit_submit_without_checkbox_clears_keep_page_numbers`,
  `edit_submit_rejects_bad_theme` (whitelist untouched).

## RED/GREEN evidence

- **RED** (commit `9efcf11`): `cargo test --lib edit_page_theme_field_sits_below_checkboxes_above_save`
  → `FAILED … 0 passed; 1 failed` with panic
  `theme field must carry field-wide: <page body dump>` (theme div still plain
  `class="field"`).
- **GREEN** (commit `0268e3a`): `cargo test --lib edit_page_` →
  `ok. 5 passed; 0 failed`; `edit_submit_rejects_bad_theme` → 1 passed.
- Task 2 is docs-only (RED/GREEN-exempt); verified by grep: `Looks` count 1;
  line 110 contains `non-conformant` and `"+N more"`; grep for
  `server-side|per-account|saved to your account` → no matches.

## Verification (plan `<verification>` + orchestrator gate)

- `cargo test --lib edit_page` green (5/5).
- Full `cargo test`: **620 passed / 0 failed / 4 ignored** (baseline 619 + 1
  new test).
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean (after one fmt reformat of the new test —
  commit `style(21-03)`).
- `git diff --stat -- '*.css'` empty across the plan — zero-new-CSS rule held.

## Task Commits

| Task | Commits |
| --- | --- |
| 1 — Theme field own row (D-06) | `9efcf11` test (RED), `0268e3a` fix (GREEN), `style` fmt commit |
| 2 — docs badge + Looks | `987c7fa` docs |

## Deviations

- **Commit granularity**: 4 commits instead of the implied 2 — the TDD
  RED test was committed separately (per orchestrator TDD mandate), and
  `cargo fmt` reformatted two lines of the new test after the fix commit,
  shipped as a small `style(21-03)` commit. No out-of-plan files touched;
  files modified are exactly the plan's `files_modified` list.
- **Baseline count**: plan text says "baseline 611 passed"; the orchestrator
  baseline at HEAD `fdb0c02` was 619 (21-01/21-02 added tests). Actual result
  620 = 619 + 1 new test. No action needed.
- `scripts/check.sh` was not run as a single script; its three components
  (full cargo test, clippy -D warnings, fmt --check) were each run green,
  which is the orchestrator-specified gate.

## Decisions

- Kept the fix strictly to the class change D-06 prescribes — no CSS, no
  markup restructure, POST path untouched (T-21-13/T-21-14 unchanged posture).
- Docs placed in the two existing sections (no new page, no SUMMARY wiring),
  voice matched to surrounding prose; badge paragraph anchored to "last
  Refresh's conformance scan" per 21-01's actual data flow.

## Lessons

- The `field-wide` flex mechanism is the established lever for "own row" in
  `form.stacked` — any future Vault Edit field that must own a row should use
  it rather than new CSS.
- Render-order tests on `format!`-built pages work well with `find()` index
  comparisons; the body dump in the panic message made the RED failure
  self-explanatory.
- `cargo fmt` can disagree with hand-wrapped chained calls in tests — run
  `cargo fmt` before the fix commit to fold it in and save a style commit.
