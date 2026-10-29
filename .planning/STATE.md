---
gsd_state_version: 1.0
milestone: v1.1.0
milestone_name: demo hardening + vault options
status: executing
stopped_at: Phase 13 context gathered (auto)
last_updated: "2026-10-29T01:27:37.243Z"
last_activity: 2026-10-29 -- Phase 14 planning complete
progress:
  total_phases: 3
  completed_phases: 2
  total_plans: 6
  completed_plans: 5
  percent: 67
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-25)

**Core value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site.
**Current focus:** Phase 14 — local vault upload (v1.1.0)

## Current Position

Phase: 14
Plan: Not started
Status: Ready to execute
Last activity: 2026-10-29 -- Phase 14 planning complete

Progress: [░░░░░░░░░░] 0% (0/3 phases of v1.1.0)

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

Last session: 2026-10-29T00:02:07.603Z
Stopped at: Phase 13 context gathered (auto)
Resume file: .planning/phases/MORI-13-wiki-changes-log-v1-1-0/13-CONTEXT.md

## Useful links

- ROADMAP: `.planning/ROADMAP.md` · REQUIREMENTS: `.planning/REQUIREMENTS.md`
- Handoff: `.planning/HANDOFF.md` · Changelog + AD/PRD + notable errors: `.planning/STATE-LOG.md`
- Pre-GSD phases: `.planning/phases/` · Live: https://morinet.treklair.fr
