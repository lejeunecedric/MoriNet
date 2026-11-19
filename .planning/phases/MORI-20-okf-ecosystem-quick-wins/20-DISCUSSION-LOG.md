# Phase 20: okf-ecosystem-quick-wins - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-18
**Phase:** 20-okf-ecosystem-quick-wins
**Areas discussed:** Docs vault conformance

---

## Gray area selection

Presented four areas (docs vault conformance, validator wiring, viz.html in export, conformance badge). First selection form timed out; retry succeeded.

**User's choice:** Docs vault conformance only.

---

## Docs vault conformance

### Q1: What OKF frontmatter should the docs vault pages carry?

| Option | Description | Selected |
|--------|-------------|----------|
| B3Wiki house pattern (Recommended) | type + title + description + generated {by, at} per page; honest, no fabricated trust fields | ✓ |
| Minimal | Just `type:` per page; least churn, passes §9 conformance, thinner OKForest coverage | |
| Rich | House pattern + `sources:` per page; richest graph but self-referential for product docs | |

**User's choice:** B3Wiki house pattern.
**Notes:** Mirrors the user's own B3Wiki conversion (`OKF-MAPPING.md`, 161 files, 0 errors / 0 warnings) including its honesty rule.

### Q2: How should the demo-vault half of OKFE-01 be handled?

| Option | Description | Selected |
|--------|-------------|----------|
| One-off manual validation (Recommended) | Run validators against demo vault content (holldemo or local copy), fix at source, record evidence; no permanent automation | ✓ |
| Scope down: docs vault only | OKFE-01 becomes docs-vault-only; demo vault stays out of the claim | |
| Vendor demo content in-repo | Like docs/morinet-docs so check.sh validates it too; heavier | |

**User's choice:** One-off manual validation.

### Q3: Should existing installs re-seed their docs vault after frontmatter is added?

| Option | Description | Selected |
|--------|-------------|----------|
| Bump marker, force re-seed (Recommended) | Every existing install re-seeds once at next boot; docs vault is first-party content | ✓ |
| No re-seed — fresh installs only | Existing installs keep the old docs vault; claim describes repo content only | |

**User's choice:** Bump marker, force re-seed.

### Q4: Warning bar for the validators

| Option | Description | Selected |
|--------|-------------|----------|
| Zero errors AND zero warnings (Recommended) | B3Wiki standard; exceptions documented in the evidence note | (pending — timed out) |
| Zero errors, warnings tolerated | Warnings listed in a short exceptions note | |

**User's choice:** No response (user away). Recommended default adopted in CONTEXT.md D-04, flagged for confirmation during planning.

### Continuation check

Follow-up round (warning bar + more areas vs ready) timed out — user away. Remaining areas (validator wiring, viz.html, badge) were never selected for discussion; their constraints were written into CONTEXT.md as locked-constraints + planner discretion.

## Claude's Discretion

- Validator wiring mechanics, viz.html sourcing/injection details, badge style/wording — not selected for discussion; constrained in CONTEXT.md.
- Exact frontmatter values per docs page, evidence-note location/format.

## Deferred Ideas

- No-code ecosystem quick wins (GitHub topics, okf.md listing issue, awesome-okf PRs, BundleDex) — operator actions.
- Obsidian-consumer / MCP-consumer recipes cross-listing — marketing work.

---

*2026-11-19 (plan-phase): re-asked D-04 (validation bar) via interactive gate — timed out unanswered; recommended default (zero errors AND zero warnings) stands as Claude-recommended default, mode: timed out, defaults applied.*
