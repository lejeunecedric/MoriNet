# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-25)

**Core value:** Charger un vault Obsidian/llm-wiki et le servir instantanément en site mdBook navigable, multi-utilisateurs et sécurisé.
**Current focus:** Phase 10 — MCP

## Current Position

Phase: 10 of 11 (mcp)
Plan: none yet — phase not planned
Status: Ready to plan
Last activity: 2026-10-25 — adoption GSD ; v0.1.20 déployé (phases 00→09.1 livrées)

Progress: [█████████░] 86% (12/14 phases)

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

### Decisions

Full log in PROJECT.md (Key Decisions) and STATE-LOG.md (AD-1..AD-14, PRD-1..PRD-6). Recent decisions affecting current work:

- [Phase 8b / AD-14]: vault = Obsidian/llm-wiki converti in-pipeline dans `.morinet-build/` — le MCP (Phase 10) doit lire la source convertie, pas le `book/` compilé
- [Phase 8]: groupes + vaults personnels + quotas — les tools MCP ne voient que les vaults accessibles à l'utilisateur (même ACL que WebDAV)
- [Phase 9b]: design system stabilisé — ne pas toucher au CSS sans passe Impeccable (skill `ui-design-passes`)
- [Ops]: CI Linux-only ; `release.yml` cargo-dist intouchable ; mdBook thèmes figés au build → Refresh après modif d'assets (pitfall #24)

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 10 : le schéma d'auth MCP (« Bearer token derived from password ») n'est pas encore arrêté — à trancher en discuss-phase (Bearer dédié vs session cookie vs mot de passe direct)
- `mdbook` CLI requis dans le PATH de prod (échec LOUD si absent) — à documenter dans le README (Phase 11)

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| v1 out of scope | Real-time collab, mobile apps, SSO, webhooks, encryption at rest | Carried to post-v1 | v0.1 planning |

## Session Continuity

Last session: 2026-10-25
Stopped at: bootstrap GSD terminé (PROJECT.md, config.json, STATE.md GSD, ROADMAP GSD-parseable). IDEA.md modifié en attente de commit (user edit).
Resume file: None — next: `/gsd:plan-phase 10` (MCP)

## Useful links

- ROADMAP: `.planning/ROADMAP.md` · REQUIREMENTS: `.planning/REQUIREMENTS.md`
- Handoff: `.planning/HANDOFF.md` · Changelog + AD/PRD + erreurs notables: `.planning/STATE-LOG.md`
- Phases pré-GSD: `.planning/phases/` · Live: https://morinet.treklair.fr
