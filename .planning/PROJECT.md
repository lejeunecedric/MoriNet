# MoriNet

## What This Is

A Rust server that exposes Obsidian / llm-wiki second brains as mdBook sites, with multi-user auth, an admin UI, vault management (local folders + GitHub repos), WebDAV and MCP. A single static binary: `cargo install morinet`.

**Live instance**: https://morinet.treklair.fr (public demo — credentials on the server, never in the repo).

## Core Value

Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

## Current Milestone: v1.2 Team Access

**Goal:** a team can sign in through its own OIDC identity provider, read its vaults programmatically through a REST API, and edit in parallel without clobbering each other's work.

**Target features:**
- **Generic OIDC SSO** — any OIDC-compliant IdP (Keycloak / Authentik / Authelia) configured in `config.toml`; coexists with password login (SSO button on the login page); account auto-created at first SSO login when the email matches an existing account or an allowed domain; OIDC groups claim → MoriNet groups (e.g. admin)
- **Read-only REST API** — list vaults / list notes / read note / search, authenticated with the existing `/account` API tokens, ACL-scoped like MCP
- **Safe asynchronous co-editing** — conflict detection via hash/ETag before overwrite: WebDAV `If-Match`/`If-None-Match` + upload-time check; no locks, no CRDT

**Key context:** all three were Out of Scope in v1 — real-time CRDT editing stays out (replaced by conflict detection); the IdP is external to MoriNet; phase numbering continues from 14 → Phase 15+.

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

### Active

<!-- Milestone v1.2 Team Access — scope being defined (2026-10-29). -->

- [ ] OIDC SSO coexisting with password auth (IdP in config, auto-provisioning, group claim mapping)
- [ ] Read-only REST API on existing API tokens (vaults, notes, read, search)

### Out of Scope

- Real-time collaborative editing (CRDT/shared editor) — v1.2 ships conflict detection instead; CRDT complexity still not justified
- Native mobile apps — responsive web is enough
- Webhooks / public API to **manage** vaults — v1.2 adds a read-only REST API; management stays on WebDAV + admin UI
- Encryption at rest — relying on the server's FS

## Context

- Stack: Rust stable, axum, sqlx (SQLite), mdbook (upstream CLI, called as a subprocess), rmcp (coming), lettre. Deployed with systemd + nginx on holldemo.
- Vault pipeline: sync → mirror in `<source>/.morinet-build/` (frontmatter consumed, wikilinks resolved, SUMMARY.md + book.toml generated) → `mdbook build` → served. mdBook themes are frozen at build: after changing `assets/zoom.*` or `MORINET_CSS`, you must click **Refresh** (pitfall #24).
- 334 tests, clippy `-D warnings`, fmt clean on every release (`scripts/check.sh`). CI Linux-only since v0.1.16 (flaky Mac runners).
- v1.1.0 live on holldemo (2026-10-29): locked demo account, per-vault display options, wiki-changes log, local vault upload. Issue #7 closed.
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
*Last updated: 2026-10-30 — Phase 15 (conflict-safe co-editing) complete; remaining v1.2 scope: REST API (Phase 16), OIDC SSO (Phase 17).*
