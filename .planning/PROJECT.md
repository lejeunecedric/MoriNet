# MoriNet

## What This Is

A Rust server that exposes Obsidian / llm-wiki second brains as mdBook sites, with multi-user auth, an admin UI, vault management (local folders + GitHub repos), WebDAV and MCP. A single static binary: `cargo install morinet`.

**Live instance**: https://morinet.treklair.fr (public demo — credentials on the server, never in the repo).

## Core Value

Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

## Current State

**v1.3 "OKF Bundle Support" shipped 2026-11-06** (released v1.4.0, live on holldemo): OKF conformance detection + vault badge, bundle export (zip), concept metadata on `/api/v1` (`?meta=1`) + MCP `type`/`tags` filters (OKF-01..04), plus Phase 18.1 tech-debt cleanup (fmt_date fix, LOW test gaps). 554 tests green. Milestone archive pending (`/gsd-complete-milestone v1.3`). Ecosystem: listing issue opened on `fabricioctelles/skills#26` (okf.md catalog); research: `.planning/research/OKF.md` + `OKF-ECOSYSTEM.md`.

## Current Milestone: v1.4 Statistics & Observability

**Goal:** Vault owners can see who reads their vaults and how — top viewed pages (web), API/MCP usage, MCP request↔response activity, top search keywords. Plus OKF ecosystem quick wins (external validator interop + viz.html in bundle export). (GitHub issue #10; research: `.planning/research/OKF-ECOSYSTEM.md`)

**Target features:**
- Access logging extended to all surfaces: web page views, REST API calls, MCP tool invocations (`access_log.source` CHECK widened — migration 0014)
- Top viewed pages per vault on the Statistics page
- MCP activity log: request (tool, args, actor) ↔ response (status, latency, excerpt)
- Top search keywords (MCP `search_notes` + REST `search`)
- Best-effort retention: 90-day boot prune, no full response bodies (excerpt + size), same conventions as `vault_changes`
- OKF ecosystem: docs/demo vaults pass okflint + OKForest CLI; official viz.html shipped inside the export zip

**Key context:** `access_log` exists since migration 0008 with `source IN ('web','mcp')` — REST API (Phase 16) currently logs nothing; SQLite CHECK change = table rebuild in migration. Issue #10 body: "For web we want which pages [viewed]… For MCP we want the MCP requests on left and answer on right and statistics of main keywords."

## Next Milestone Goals

- OKF-F01 trust-aware rendering (needs Impeccable pass), G1-G3 composed E2E tests from the v1.2 audit, deferred human UATs (rclone/Obsidian WebDAV smoke, real-IdP SSO smoke)
- Backlog: API-F01 write endpoints, API-F02 scoped tokens, API-F03 webhooks

## Requirements

### Validated

<!-- Shipped and confirmed valuable (v0.1.0 → v0.1.20). -->

- ✓ Single binary `cargo install morinet`, CI + cargo-dist release + auto-deploy — REQ-INFRA-* (v0.1.0..v0.1.16)
- ✓ HttpOnly + HMAC cookie login, SQLite bcrypt users, admin/reader roles — REQ-AUTH-* (v0.1.1)
- ✓ Multi-vault `/vault/<slug>/` with per-vault ACL — REQ-VAULT-001..006 (v0.1.1)
- ✓ Vault sources: local folder OR GitHub repo (clone + git2 sync + mdbook build) — REQ-VAULT-007..010 (v0.1.2+)
- ✓ Admin UI: invitations, password resets, vaults + permissions — REQ-ADMIN-* (v0.1.4+)
- ✓ Email: 24h single-use invitations + 1h reset (lettre SMTP, temp-password fallback) — REQ-EMAIL-* (v0.1.6+)
- ✓ WebDAV PROPFIND/GET/PUT on the vault **source** (markdown in, `mdbook build` out), HTTP Basic + ACL — REQ-WEBDAV-* (v0.1.7+)
- ✓ Native Obsidian/llm-wiki format: in-pipeline converter (AD-14) → mdBook tree generated in `.morinet-build/` — REQ-OBSFMT-001..006 (v0.1.8)
- ✓ Groups + personal vaults (`scope`/`owner_id`, `vault_group_acl`), per-group quotas (`free`=2, `admin`=∞) — REQ-GRP-*, REQ-VLT-005..008 (v0.1.9..v0.1.12)
- ✓ Vault UX: server-side folder picker, browser import, GitHub URL auto-name, owner Refresh/Delete — REQ-UX-* (v0.1.10..v0.1.15)
- ✓ Nav shell + design system (1 single header control, pill nav, zoom + Close vault on mdBook pages) — REQ-ACC-*, REQ-NAV-* (v0.1.14..v0.1.20)
- ✓ MCP StreamableHttp on `/mcp`: Bearer tokens, list/read/search tools on accessible vaults — REQ-MCP-* (v1.0.0)
- ✓ Polish + ship: minimal README, `morinet init`, local gate/release/deploy scripts — REQ-OPS-* (v1.0.0, 2026-10-25)
- ✓ Post-release fixes: global header on sub-pages (v1.0.1), vault chrome on every served page + Exit Settings (v1.0.2, issues #8/#9), numeric menu prefixes stripped (v1.0.3, issue #7 partial)
- ✓ Demo account lock (server refuse + home popup) + per-vault display options: keep-page-numbers toggle, mdBook theme select — REQ-DEMO-*, REQ-VOPT-* (v1.1.0, Phase 12)
- ✓ Wiki-changes log: `vault_changes` table fed at Refresh (git tree-diff) + WebDAV PUT, "Wiki changes" section in Statistics grouped per event — REQ-CHG-* (v1.1.0, Phase 13)
- ✓ Local vault upload: browser directory picker → sanitized multipart endpoint, quota/lock gates, shared convert/build/register pipeline — REQ-LOC-* (v1.1.0, Phase 14)
- ✓ Conflict-safe co-editing: strong sha256 ETags + If-Match/If-None-Match on WebDAV, upload conflict list + force-overwrite audit — CONF-01..05 (v1.2, Phase 15)
- ✓ Read-only REST API on existing API tokens (vaults, notes, read, search) — API-01..06 (v1.2, Phase 16)
- ✓ Generic OIDC SSO coexisting with password auth (IdP in config, auto-provisioning, group claim mapping) — SSO-01..06 (v1.2, Phase 17)

### Active

<!-- v1.4 Statistics & Observability — scoped 2026-11-18 from issue #10 + OKF ecosystem research. -->

- **STAT-01..05**: access logging all surfaces, top pages, MCP request↔response activity, top keywords, 90-day retention (see REQUIREMENTS.md)
- **OKFE-01..02**: external validator interop (okflint/OKForest), viz.html in bundle export


### Out of Scope

- Real-time collaborative editing (CRDT/shared editor) — v1.2 ships conflict detection instead; CRDT complexity still not justified
- Native mobile apps — responsive web is enough
- Webhooks / public API to **manage** vaults — v1.2 adds a read-only REST API; management stays on WebDAV + admin UI
- Encryption at rest — relying on the server's FS

## Context

- Stack: Rust stable, axum, sqlx (SQLite), mdbook (upstream CLI, called as a subprocess), rmcp (coming), lettre. Deployed with systemd + nginx on holldemo.
- Vault pipeline: sync → mirror in `<source>/.morinet-build/` (frontmatter consumed, wikilinks resolved, SUMMARY.md + book.toml generated) → `mdbook build` → served. mdBook themes are frozen at build: after changing `assets/zoom.*` or `MORINET_CSS`, you must click **Refresh** (pitfall #24).
- 554 tests, clippy `-D warnings`, fmt clean on every release (`scripts/check.sh`). No GitHub CI since 2026-10-25 — local gate only.
- v1.4.0 live on holldemo (2026-11-06): OKF conformance badge, bundle export, API/MCP concept metadata. Issues #7/#11 closed.
- Detailed version-by-version history: `.planning/STATE-LOG.md` and `.planning/HANDOFF.md`.
- Catalog of verifiable requirements: `.planning/REQUIREMENTS.md`.

## Constraints

- **Tech**: Rust only, static binary — `cargo install` distribution, zero runtime
- **External dependency**: the `mdbook` CLI must be in the PATH (LOUD failure if missing, no silent fallback)
- **Compat**: mdBook-native vaults (`book.toml`) keep working as-is alongside the Obsidian format
- **Security**: prod credentials out of the repo (systemd env); never hand-edit the cargo-dist `release.yml`
- **Deployment**: staging + atomic `mv` (ETXTBSY); the local release attaches its assets BEFORE publishing (draft→publish)
- **Build**: no more GitHub CI — `scripts/check.sh` runs the gate before every release

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Vault format = Obsidian/llm-wiki converted in-pipeline (AD-14) rather than a mdBook-native contract | Users have Obsidian vaults, not mdBooks; conversion > scaffold | ✓ Good (v0.1.8, E2E verified on ProgWiki 131 .md / 812 wikilinks) |
| Abandon the "merged book" → groups + personal vaults (Phase 8) | User decision 2026-10-21: clean isolation > merging | ✓ Good |
| Per-group quotas (`free`=2, `admin`=∞), demo demoted to reader | Prevent slot exhaustion + ghost Admin button | ✓ Good |
| CI Linux-only (dist 2 linux targets) | Flaky Mac runners = broken releases | ✓ Good (v0.1.16, no more re-dispatch) |
| `ensure_fallback_admin()` at boot | ADMIN_EMAIL bootstrap alone locked /admin if the only admin was demoted (treklair incident) | ✓ Good |
| 100% local compilation, GitHub ci/release workflows removed (2026-10-25) | GitHub runners useless/unstable; `scripts/check.sh` + `release.sh` (assets with the same names as cargo-dist) + `deploy.sh`; `deploy.yml` kept as-is | — Pending |
| GSD adopted as the planning mode (2026-10-25) | Toolled plan/execute/verify workflows; existing docs kept (ROADMAP/REQUIREMENTS/HANDOFF/STATE-LOG) | ✓ Good (v1.1.0 shipped through the full GSD loop) |
| Display options applied at SERVE time, not `book.toml` build time (Phase 12, D-08/D-10) | Theme/number changes take effect without Refresh/rebuild; visitor localStorage still wins | ✓ Good (v1.1.0) |
| WebDAV PUT records the single changed path instead of a full-tree hash-diff (Phase 13, D-02) | Full-tree hashing impractical at that call site; git refresh keeps the full tree-diff | ✓ Good (v1.1.0) |
| Uploaded vaults keep `vault_source = None` (Phase 14, D-04) | No upstream → Refresh correctly answers "no upstream"; re-upload is the update path | ✓ Good (v1.1.0) |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-11-18 — Milestone v1.4 "Statistics & Observability" started (issue #10 + OKF ecosystem quick wins).*
