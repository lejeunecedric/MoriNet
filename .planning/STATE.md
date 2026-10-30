---
gsd_state_version: 1.0
milestone: v1.2
milestone_name: Team Access
status: "Roadmap created 2026-10-30 (Phases 15–17); next step: plan Phase 15"
stopped_at: Phase 15 context gathered
last_updated: "2026-10-30T08:56:19.776Z"
last_activity: 2026-10-30 — v1.2 roadmap created (Phases 15–17, all 17 REQ-IDs mapped)
progress:
  total_phases: 3
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-25)

**Core value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site.
**Current focus:** v1.2 Team Access — roadmap created, Phase 15 planning

## Current Position

Phase: Phase 15 (etag-conflict) — planning
Plan: —
Status: Roadmap created 2026-10-30 (Phases 15–17); next step: plan Phase 15
Last activity: 2026-10-30 — v1.2 roadmap created (Phases 15–17, all 17 REQ-IDs mapped)

## Performance Metrics

**Velocity:**

- Total plans completed: n/a (pre-GSD phases used `.planning/phases/*/…-PLAN.md`)
- Historical detail: .planning/STATE-LOG.md

**By Phase:**

| Phase | Plans | Status |
|-------|-------|--------|
| 00–09.1 | pre-GSD | Complete (v0.1.0 → v0.1.20) |
| 10. mcp | TBD | Not started |
| 11. polish v1.0 | TBD | Not started |

## Accumulated Context

### Roadmap Evolution

- Phase 10.1 inserted after Phase 10: Home UX: vault actions, share by email, statistics (URGENT)
- 2026-10-28: v1.1.0 milestone opened — Phase 12 (demo lock + per-vault page-numbers toggle + mdBook theme), Phase 13 (wiki-changes log in Statistics), Phase 14 (local vault upload). Driven by user feedback on the live demo (issues #7/#8/#9 + new requests).
- 2026-10-30: v1.2 Team Access roadmap created — Phase 15 (etag-conflict), Phase 16 (rest-api), Phase 17 (oidc-sso). Order per research consensus (`.planning/research/SUMMARY.md`): ETag → REST → OIDC; numbering continues from 14.

### Decisions

Full log in PROJECT.md (Key Decisions) and STATE-LOG.md (AD-1..AD-14, PRD-1..PRD-6). Recent decisions affecting current work:

- [Phase 8b / AD-14]: vault = Obsidian/llm-wiki converted in-pipeline into `.morinet-build/` — MCP (Phase 10) must read the converted source, not the compiled `book/`
- [Phase 8]: groups + personal vaults + quotas — MCP tools only see the vaults accessible to the user (same ACL as WebDAV)
- [Phase 9b]: design system stabilized — do not touch the CSS without an Impeccable pass (skill `ui-design-passes`)
- [Ops]: CI Linux-only; cargo-dist `release.yml` untouchable; mdBook themes are frozen at build → Refresh after asset changes (pitfall #24)

### Pending Todos

None yet.

### Blockers/Concerns

- ~~MCP auth undecided~~ → RESOLVED 2026-10-25: dedicated API token (`.planning/phases/10-mcp/CONTEXT.md`)
- `mdbook` CLI required in the prod PATH (LOUD failure if missing) — to document in the README (Phase 11)

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| v1 out of scope | Real-time collab, mobile apps, SSO, webhooks, encryption at rest | Carried to post-v1 | v0.1 planning |

## Session Continuity

Last session: 2026-10-30T08:56:19.771Z
Stopped at: Phase 15 context gathered
Resume file: .planning/phases/MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05/15-CONTEXT.md

## Useful links

- ROADMAP: `.planning/ROADMAP.md` · REQUIREMENTS: `.planning/REQUIREMENTS.md`
- Handoff: `.planning/HANDOFF.md` · Changelog + AD/PRD + notable errors: `.planning/STATE-LOG.md`
- Pre-GSD phases: `.planning/phases/` · Live: https://morinet.treklair.fr

## Operator Next Steps

- Plan Phase 15 (etag-conflict) with /gsd-plan-phase
