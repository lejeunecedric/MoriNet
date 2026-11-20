---
phase: MORI-20-okf-ecosystem-quick-wins
plan: 02
subsystem: docs/infra
tags: [okf, okflint, okforest, validation, evidence]

requires:
  - phase: 20-01
    provides: OKF v0.2 frontmatter on the six docs pages (validation target)
provides:
  - Clean official-validator runs (okflint 0.5.1, okforest 0.14.0) on docs/morinet-docs at the D-04 bar (0 errors / 0 warnings)
  - docs/morinet-docs/okf-base.yaml — okflint manifest pinning the Guide/Reference/FAQ taxonomy
  - docs/morinet-docs/.okforest/settings.yaml — okforest ignore config mirroring the SUMMARY.md exclusion
  - 20-EVIDENCE.md — auditable evidence (versions, exact commands, full outputs, commit SHA) for the 20-06 README badge
affects: [20-03-checksh-gate, 20-06-readme-badge]

tech-stack:
  added: []
  patterns:
    - "okforest zero-warnings bar enforced by parsing --format json summary counts, never the exit code (exit 1 = errors only)"
    - "validator scan-genuineness probe: break a page in a scratch copy, confirm the validator flags it, before trusting a clean run"

key-files:
  created:
    - docs/morinet-docs/okf-base.yaml
    - docs/morinet-docs/.okforest/settings.yaml
    - .planning/phases/MORI-20-okf-ecosystem-quick-wins/20-EVIDENCE.md
  modified: []

key-decisions:
  - "SUMMARY.md (mdBook TOC index, not a concept, D-01) excluded via config in BOTH validators (okf-base.yaml exclude_patterns + .okforest/settings.yaml ignore) instead of a documented exception — both tools then judge the same six concept pages and the 20-03 check.sh gate can be a plain exit-code/JSON check"
  - "okf-base.yaml declares no status vocabulary and requires [type, title, description, generated] per type — the D-01 honesty rule is pinned into the manifest itself"
  - "Residual okforest finding okf/okf-version (no root index.md) is severity info — below the D-04 bar; recorded in the Exceptions table citing Phase 18 D-06 (index.md synthesized at zip time)"

requirements-completed: [OKFE-01]
requirements_completed: [OKFE-01]
one_liner: "Docs vault passes okflint 0.5.1 (exit 0) and okforest 0.14.0 (JSON: 0 errors, 0 warnings, 6 concepts) with a pinned Guide/Reference/FAQ manifest; clean runs recorded with versions, exact commands, full outputs and commit SHA in 20-EVIDENCE.md"

duration: ~25 min
completed: 2026-11-20
---

# Phase MORI-20, Plan 02: Official validator clean runs Summary

**The built-in docs vault passes both official OKF validators at the D-04 bar — okflint 0.5.1 exit 0 and okforest 0.14.0 JSON with 0 errors / 0 warnings across 6 concepts — and the clean runs are durably recorded (versions, exact commands, full outputs, validated commit SHA) in 20-EVIDENCE.md.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-11-20T15:10:00Z (approx)
- **Completed:** 2026-11-20T15:32:00Z (approx)
- **Tasks:** 2
- **Files created:** 3 (no content pages needed changes)

## Accomplishment details

- **Task 1** (`75a83ae`): `docs/morinet-docs/okf-base.yaml` — okflint manifest adapted from upstream `example/manifest.example.yaml`: types Guide/Reference/FAQ each requiring `[type, title, description, generated]`, **no status vocabulary** (D-01), `src/SUMMARY.md` excluded as the mdBook index. `docs/morinet-docs/.okforest/settings.yaml` — okforest has no manifest; its `.okforest/settings.yaml` `ignore:` globs (discovered in `@okforest/okf` `readTree`/`readSettings`) mirror the exclusion. Results: okflint `✅ All files are OKF-conformant.` exit 0; okforest `{"ok": true, "errors": 0, "warnings": 0, "infos": 1, "concepts": 6}` exit 0.
- **Task 2** (`d51f41f`): `20-EVIDENCE.md` — scope-limited header (docs vault + demo placeholder, NOT arbitrary user vaults), versions via `pip show okflint | head -2` + `okforest --version`, exact gate commands, full outputs, validated SHA `75a83ae`, scan-genuineness probe record, iterations, Exceptions table (info-level `okf/okf-version`), and the pinned literal `TODO(20-06)` demo placeholder.

## Acceptance criteria verification

- `okflint validate --manifest docs/morinet-docs/okf-base.yaml docs/morinet-docs` → exit **0** (PASS)
- okforest JSON parsed programmatically: `errors: 0, warnings: 0` (PASS; single residual finding is severity **info**, rule `okf/okf-version`, in the Exceptions table)
- `grep -rn -E '^(verified|status|stale_after):' docs/morinet-docs/src/` → no matches (PASS — honesty rule intact)
- `cargo test docs_seed` → 5 passed / 0 failed / 1 ignored (PASS)
- Scan-genuineness: scratch copy with faq.md frontmatter stripped → okflint flags F001 exit 1 (PASS — clean run is not vacuous)
- Gate: `scripts/check.sh` exit **0** — **603 passed / 0 failed / 4 ignored** (baseline held)

## Task Commits

1. **Task 1: validator configs, 0/0 achieved** — `75a83ae` (feat)
2. **Task 2: durable evidence note** — `d51f41f` (docs)

**Plan metadata:** this SUMMARY commit (docs(20-02)).

## Deviations from Plan

1. **[Rule 3 - Missing critical] `.okforest/settings.yaml` added** — not in `files_modified`. The plan anticipated "if the manifest supports ignore/exclude entries, exclude it there" for SUMMARY.md, but okforest has no manifest; its supported exclusion is `.okforest/settings.yaml` `ignore:` globs. Without it, okforest reports an **error** on SUMMARY.md (exit 1), which would force the 20-03 check.sh gate to hard-code rule-id whitelisting. Config-only exclusion keeps both validators judging the same six pages. Verified the exclusion is honored (6 concepts scanned).
2. **No content-page fixes were needed** — the 20-01 frontmatter shape validated clean on the first okflint run; the only findings were SUMMARY.md (config-excluded) and the info-level absent-index note.

## Issues Encountered

- okflint `--json` emits only the findings list (`[]` when clean) with no scanned-file count — a clean run could be vacuous if root resolution were wrong. Closed with the scratch-copy F001 probe (recorded in the evidence file).
- okforest's ignore mechanism is undocumented in its README; located by reading `@okforest/okf/dist/node.mjs` (`readTree` → `ignoreMatcher(readSettings(...)?.ignore)`).

## Next Phase Readiness

- **20-03 (check.sh gate):** both gate commands are stable and self-contained; okforest warning bar must be checked via `--format json` summary counts (exit code covers errors only).
- **20-06 (README badge):** evidence file exists at the pinned location with the `TODO(20-06)` marker for the demo-vault section.
- Note: `.okforest/settings.yaml` is a dev-time validation config in the repo copy only — `docs_seed.rs` embeds a fixed file list, so it is not seeded to installs (same as `okf-base.yaml`); no seed-marker bump needed.

## Self-Check: PASSED
