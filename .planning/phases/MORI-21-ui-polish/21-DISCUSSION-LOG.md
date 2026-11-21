# Phase 21: ui-polish - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-21
**Phase:** 21-ui-polish
**Areas discussed:** OKF badge details (selected); Looks and Vault Edit left to Claude's discretion

---

## OKF badge details (UI-01, #16)

### Question 1 — What does the click reveal?

| Option | Description | Selected |
|--------|-------------|----------|
| File list + breakdown (Recommended) | Scan stores non-conformant file paths (cap ~200) plus version, N/M, index.md/log.md, scan date | ✓ |
| Aggregate breakdown only | No scan change; no per-file list | |

**User's choice:** File list + breakdown.
**Notes:** Directly answers #16's "show a list".

### Question 2 — Interaction style

| Option | Description | Selected |
|--------|-------------|----------|
| Inline `<details>` under the badge (Recommended) | Same pattern as account menu; zero new CSS | ✓ |
| Centered scrollable modal | More visible; conflicts with no-new-CSS rule | |

**User's choice:** Inline `<details>`.

### Question 3 — Placement

| Option | Description | Selected |
|--------|-------------|----------|
| Home cards + vault page (Recommended) | Same component wherever the badge exists | ✓ |
| Vault page only | Home cards keep static badge | |

**User's choice:** Home cards + vault page.

---

## Claude's Discretion

- **Looks (UI-02, #17):** entry in the mdBook book menu before "About", mdBook palette, localStorage persistence under mdBook's theme keys; no server-side per-reader storage this phase.
- **Vault Edit alignment (UI-03, #13):** minimal layout fix (field-wide wrapping), no form restructure.

## Deferred Ideas

- Server-side per-reader theme preference (`user_prefs` column) — own phase if ever needed.

## Plan-Phase Gate (2026-11-21)

Mode: timed out, defaults applied. The plan-phase clarify gate (research? / UI-SPEC? / lock D-05+D-06?) returned unanswered; recommended defaults applied and marked visibly in CONTEXT.md:
- Research: skipped (config `workflow.research: false` + phases 19–20 precedent).
- UI-SPEC: skipped — phase adds only small controls to existing server-rendered pages; D-02/D-06 lock "zero new CSS, reuse existing classes".
- D-05 / D-06: locked as written (Claude-recommended, override possible).

## Plan-Checker Corrections (2026-11-21)

Plan-checker returned 0 blockers / 4 warnings / 2 INFOs; all applied inline by the orchestrator:
- **D-03 / ROADMAP criterion 1 amended**: badge exists only on home vault cards (`vault_row` call sites server.rs:641/:660); served book pages render no badge — plan scope ruled a faithful code reading, wording corrected instead of expanding scope.
- **ROADMAP criterion 2 amended**: "Looks" lives in the mdBook book menu (locked D-05), not the account menu; app-chrome restyling explicitly deferred (would need new CSS + server prefs).
- **21-01 fixes**: `okf_stats` test helper (server.rs:10333) added to Task 2 (compile-breaking struct literal); vacuous artifact gate `<details>` → `<summary class="menu-btn"`; `MAX_EXPORT_ENTRIES` anchor corrected to okf.rs:131-137.
