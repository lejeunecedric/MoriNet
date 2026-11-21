---
phase: MORI-20-okf-ecosystem-quick-wins
plan: 06
subsystem: docs
tags: [okf, okflint, okforest, readme, conformance-badge, d-02, d-04]

requires:
  - phase: 20-02
    provides: docs-vault clean 0/0 okflint + okforest evidence, 20-EVIDENCE.md skeleton with TODO(20-06) demo placeholder
  - phase: 20-03
    provides: optional scripts/check-okf.sh validator gate step (docs-vault gate)
provides:
  - README.md OKF v0.2 conformance badge + scope-honest claim citing 20-EVIDENCE.md
  - 20-EVIDENCE.md demo-vault section completed with real one-off transcripts (D-02) and the resolved checkpoint decision
affects: [future-readme-edits, future-demo-vault-revalidation]

tech-stack:
  added: []
  patterns:
    - "public conformance claims cite a durable evidence file and certify only content the project controls — never arbitrary user vaults"

key-files:
  created: []
  modified:
    - README.md — OKF v0.2 shields.io badge beside License badge; scoped claim paragraph in the OKF bundle support section
    - .planning/phases/MORI-20-okf-ecosystem-quick-wins/20-EVIDENCE.md — demo-vault run recorded (Task 1); checkpoint status resolved (Task 2)

key-decisions:
  - "Checkpoint (user, 2026-11-20): Option 1/C — publish with the plan's pending-operator variant: badge + claim cite DOCS-VAULT evidence only (0/0 clean runs); demo-vault validation explicitly marked in-progress/pending-operator; demo-vault conformance NOT claimed"
  - "Demo vaults (B3Wiki, ProgWiki) failed the D-04 zero-warnings bar; fixes are upstream operator-repo work, out of scope for this repo — exact fix lists recorded in 20-EVIDENCE.md"
  - "Static shields.io badge (no CI generation), matching the existing License badge style"

patterns-established:
  - "Badge/claim wording must not overstate: certify MoriNet's own content only, with an inline citation of the auditable evidence path"

requirements-completed: [OKFE-01]
requirements_completed: [OKFE-01]
one_liner: "README now carries a static OKF v0.2 conformance badge and a scope-honest claim — docs vault passes okflint + OKForest CLI 0 errors/0 warnings (evidence cited), demo-vault validation explicitly marked in-progress per the user's Option 1 checkpoint decision — after a one-off D-02 demo validation recorded real findings (b3wiki 0 err/6+89 warn, progwiki pre-conversion) with fixes listed for upstream operator repos"

duration: ~25 min (this continuation; Task 1 recorded earlier at be1b4bb)
completed: 2026-11-20
---

# Phase 20, Plan 06: OKF conformance badge & claim Summary

**README publishes a static OKF v0.2 conformance badge with a scope-honest claim backed by 20-EVIDENCE.md — docs vault certified clean (0/0), demo-vault validation marked in-progress per the user's checkpoint decision.**

## Performance

- **Duration:** ~25 min (continuation: Task 2 + summary; Task 1 committed earlier as be1b4bb)
- **Started:** 2026-11-20T15:52Z (Task 1 evidence run)
- **Completed:** 2026-11-20T16:11Z
- **Tasks:** 2 (Task 1 one-off demo validation; Task 2 README badge/claim) + blocking checkpoint resolved
- **Files modified:** 2 (README.md, 20-EVIDENCE.md)

## Accomplishments
- One-off D-02 demo-vault validation executed with real transcripts in 20-EVIDENCE.md (b3wiki: okflint 0 err/6 warn, okforest 0 err/89 warn @ 1344f08; progwiki: pre-conversion, many errors @ 2c2668e); exact upstream fixes listed for the operator repos; `TODO(20-06)` placeholder removed.
- Blocking human checkpoint resolved: user chose Option 1 (pending-operator variant) on 2026-11-20.
- README badge line gained `![OKF v0.2 conformance]` (static shields.io, links to `#okf-bundle-support`); the OKF bundle support section gained the scoped claim: docs vault passes okflint + OKForest CLI with zero errors/warnings, demo validation **in progress**, evidence path cited, "not arbitrary user vaults" preserved.
- Gate verified green after the change: `scripts/check.sh` exit 0 — 607 passed / 0 failed / 4 ignored (baseline held).

## Task Commits

1. **Task 1: One-off demo-vault validation (D-02)** — `be1b4bb` (docs: record run, findings pending source fixes)
2. **Checkpoint resolution + Task 2: Publish README badge + scoped claim** — `c7a1285` (docs: badge + claim + evidence status resolved)

## Files Created/Modified
- `README.md` — OKF v0.2 conformance badge beside the License badge; scoped claim paragraph in "OKF bundle support" citing `.planning/phases/MORI-20-okf-ecosystem-quick-wins/20-EVIDENCE.md`
- `.planning/phases/MORI-20-okf-ecosystem-quick-wins/20-EVIDENCE.md` — demo-vault section completed (be1b4bb); status section updated to record the Option C decision (c7a1285)

## Decisions Made
- Checkpoint Option 1/C (user-approved 2026-11-20): publish now with the plan's pending-operator variant — claim cites docs-vault evidence only; demo-vault conformance explicitly NOT claimed.
- No automation added for demo validation (D-02 locked: one-off only).

## Deviations from Plan
None — plan executed as written; the checkpoint's sanctioned pending-operator wording variant was used as instructed.

## Issues Encountered
- Neither demo vault meets the D-04 0/0 bar and cannot until upstream operator repos (B3Wiki, ProgWiki) are fixed — handled via the checkpoint, not worked around; fix lists are durable in 20-EVIDENCE.md.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- OKFE-01 complete; phase 20 plans 01–06 all shipped. If the operator ever fixes B3Wiki/ProgWiki sources, re-run the two gate commands and append a clean re-run section to 20-EVIDENCE.md, then upgrade the README demo clause.
- No blockers.

---
*Phase: MORI-20-okf-ecosystem-quick-wins*
*Completed: 2026-11-20*
