---
phase: MORI-20-okf-ecosystem-quick-wins
plan: 03
subsystem: infra/testing
tags: [okf, okflint, okforest, quality-gate, check-sh, d-04]

requires:
  - phase: 20-01
    provides: OKF v0.2 frontmatter on the six docs pages (validation target)
  - phase: 20-02
    provides: docs/morinet-docs/okf-base.yaml manifest, .okforest/settings.yaml, pinned validator invocations, clean 0/0 baseline
provides:
  - scripts/check-okf.sh — standalone optional validator step (present-in-PATH probe, run, D-04 warning gate, skip notes)
  - scripts/check.sh — gate now runs the OKF validator step between tests and release build
affects: [20-06-readme-badge, future-docs-edits]

tech-stack:
  added: []
  patterns:
    - "optional external validator gate: command -v probe only; absent → explicit skip note + green; present + failing → hard failure (never swallowed, no `|| true`, never auto-installs, never touches the network)"
    - "D-04 zero-warnings bar enforced by piping okforest --format json into python3 stdlib and counting findings with severity in (error, warning) — okforest's exit code covers errors only"

key-files:
  created:
    - scripts/check-okf.sh
  modified:
    - scripts/check.sh

key-decisions:
  - "okforest warning gate counts findings[] by severity (plan-pinned mechanic) rather than summary counts — same result on the real 0.14.0 JSON shape (verified live), and prints each offending finding for the developer"
  - "skip notes name the install command (pip install okflint / npm install --global okforest) as informational text only — the script itself never invokes pip/npm/npx/curl"
  - "python3 one-liner avoids backslashes inside f-string expressions (invalid on Python < 3.12); JSON enters via stdin, never re-evaluated by the shell"

requirements-completed: [OKFE-01]
requirements_completed: [OKFE-01]
one_liner: "scripts/check.sh now runs okflint + okforest against docs/morinet-docs whenever the binaries are in PATH (skip-note + green when absent), failing the gate on any error or warning via a standalone scripts/check-okf.sh with a python3-parsed D-04 warning check — no GitHub CI, no auto-install"

duration: ~20 min
completed: 2026-11-20
---

# Phase MORI-20, Plan 03: Optional external-validator gate step Summary

**The local quality gate now runs the official OKF validators (okflint 0.5.1, okforest 0.14.0) against `docs/morinet-docs` whenever the operator has them in PATH — failing on any error or warning (D-04) and printing an explicit skip note when a validator is absent — via a standalone `scripts/check-okf.sh` wired into `scripts/check.sh`, with no GitHub CI and no auto-install path.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-11-20T15:36:00Z (approx)
- **Completed:** 2026-11-20T15:43:12Z
- **Tasks:** 2
- **Files:** 1 created, 1 modified

## Accomplishment details

- **Task 1** (`3c1d0b9`): `scripts/check-okf.sh` (executable) — `set -euo pipefail`, cd to repo root, `target="docs/morinet-docs"`. okflint block: `command -v okflint` probe → `okflint validate --manifest "$target/okf-base.yaml" "$target"` unguarded (set -e aborts on exit 1), else skip note. okforest block: `command -v okforest` probe → capture `okforest validate "$target" --format json`, echo it, pipe through python3 stdlib counting `findings[]` with severity in `("error","warning")` and exiting 1 with the offending list when non-zero (D-04), else skip note. Comment header documents the optional/no-auto-install/skip-vs-fail contract and the PATH trust boundary.
- **Task 2** (`78cabcb`): `scripts/check.sh` — inserted `echo "==> OKF external validators (optional)"; bash scripts/check-okf.sh` between the tests and release-build steps. No `|| true`. Nothing under `.github/` touched (`git status --porcelain .github/` empty).

## TDD evidence (RED → GREEN)

All scratch work in `/home/c/.hermes/profiles/devo/cache/scratch/20-03/` — the real docs vault was never mutated.

RED (gate correctly turns red):
1. **Real validators vs scratch copy of the vault with `src/faq.md` frontmatter stripped** → `❌ [F001] src/faq.md — frontmatter absent or unparsable`, `1 error(s), 0 warning(s).`, **exit 1**.
2. **Failing okflint PATH stub** (prints `boom`, exit 1) → output shows `boom`, **exit 1** (failure never swallowed).
3. **Warning-only okforest stub** (valid JSON, `warnings: 1`, exit 0) → `okforest: 1 error/warning finding(s) — D-04 bar is 0 errors / 0 warnings` + the finding line, **exit 1** (D-04 warning bar enforced despite okforest's exit 0).

GREEN:
1. **Passing stubs** (okflint exit 0 + clean-JSON okforest) → `okf-check: OK`, **exit 0**.
2. **Skip path** (`env PATH=/usr/bin:/bin bash scripts/check-okf.sh`) → both `skip: okflint not in PATH …` and `skip: okforest not in PATH …` notes printed, **exit 0**.
3. **Real vault, real validators** → okflint `✅ All files are OKF-conformant.`; okforest JSON `errors: 0, warnings: 0, infos: 1, concepts: 6`; `okf-check: OK`, **exit 0**.

## Acceptance criteria verification

- `scripts/check-okf.sh` exists, executable, contains `command -v okflint` and `command -v okforest` — PASS
- Absent-validators run exits 0, stdout contains both skip notes — PASS (TDD GREEN 2)
- Failing-stub run exits non-zero — PASS (TDD RED 2)
- No `pip install` / `npm install` / `npx` / `curl` **invocations** (the skip-note strings mention the install commands as informational text only, per the plan's own pinned snippet) — PASS
- `scripts/check.sh` contains `bash scripts/check-okf.sh` with no `|| true` suffix — PASS
- Full `bash scripts/check.sh` exit **0** on this host with validators **present and actually running** — PASS: log `scratch/check-20-03.log` shows `==> OKF external validators (optional)`, `==> okflint`, `==> okforest`, `okf-check: OK` (no skip notes), tests **607 passed / 0 failed / 4 ignored** (baseline held)
- No new files under `.github/` — PASS

## Task Commits

1. **Task 1: scripts/check-okf.sh (probe, run, D-04 gate)** — `3c1d0b9` (feat)
2. **Task 2: wire step into scripts/check.sh** — `78cabcb` (feat)

**Plan metadata:** this SUMMARY commit (docs(20-03)).

## Deviations from Plan

None — plan executed exactly as written. (One in-task portability fix during authoring: the python3 warning-counter initially used backslash-escaped quotes inside f-string expressions, invalid on Python < 3.12; rewritten with `%`-formatting before any commit.)

## Issues Encountered

- A first draft of the passing-stub test harness emitted unquoted (invalid) JSON via `printf '%s'` with shell-eaten quotes, producing a JSONDecodeError in the harness — test-fixture bug, not a script bug; fixed the stub to emit real JSON, then the passing case went green.
- Live okforest 0.14.0 JSON shape confirmed against the real vault before finalizing the parser (`summary` + `findings[].severity`), per the plan's "do not guess field names" instruction.

## Next Phase Readiness

- **20-06 (README badge):** the conformance claim is now self-enforcing on every `scripts/check.sh` run for any dev with the validators installed; 20-EVIDENCE.md remains the durable clean-run record.
- Future docs edits that break OKF conformance fail the local gate immediately (success criterion 2 closed).
- Note for operators: the gate only exercises the fail path on machines where okflint/okforest are installed (this host: okflint 0.5.1 via uv, okforest 0.14.0 npm global); elsewhere it prints the two skip notes and stays green by design.

## Self-Check: PASSED
