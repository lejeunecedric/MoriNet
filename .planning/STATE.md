---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: OKF Bundle Support
status: executing
stopped_at: Phase 16 context gathered
last_updated: "2026-11-06T18:22:42.317Z"
last_activity: 2026-11-06 -- Phase 18 planning complete
progress:
  total_phases: 1
  completed_phases: 0
  total_plans: 6
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-25)

**Core value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site.
**Current focus:** Milestone complete

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Ready to execute
Last activity: 2026-11-06 -- Phase 18 planning complete

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
| v1.2 human UAT | 15-HUMAN-UAT rclone/Obsidian real WebDAV smoke test | Pending operator | 2026-11-06 milestone close |
| v1.2 human UAT | 17-HUMAN-UAT real-IdP SSO smoke test (Keycloak/Authentik/Authelia) | Pending operator | 2026-11-06 milestone close |
| v1.2 test gaps | G1 SSO→API token→/api/v1 composed test; G2 group-revocation direct test; G3 shared-vault coexistence | Open (v1.3 quick task) | 2026-11-06 milestone audit |
| v1.2 accepted debt | JWKS/discovery uncached (REV-17-02); no Secure cookie attr (pre-existing, D-02); no Cache-Control on API; fixed-window burst; Bearer case-sensitivity | Accepted | 2026-11-06 milestone audit |

## Session Continuity

Last session: 2026-11-04T08:04:07.041Z
Stopped at: Phase 16 context gathered
Resume file: None

## Useful links

- ROADMAP: `.planning/ROADMAP.md` · Shipped requirements: `.planning/milestones/`
- Handoff: `.planning/HANDOFF.md` · Changelog + AD/PRD + notable errors: `.planning/STATE-LOG.md`
- Pre-GSD phases: `.planning/phases/` · Live: https://morinet.treklair.fr

## Operator Next Steps

- v1.3 not yet scoped — candidates in PROJECT.md "Next Milestone Goals" (issues #10/#11, G1-G3 tests)
- Run the two deferred human UATs when staging access is available (rclone/Obsidian + real IdP)
