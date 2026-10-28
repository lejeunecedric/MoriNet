# MoriNet

## What This Is

A Rust server that exposes Obsidian / llm-wiki second brains as mdBook sites, with multi-user auth, an admin UI, vault management (local folders + GitHub repos), WebDAV and MCP. A single static binary: `cargo install morinet`.

**Live instance**: https://morinet.treklair.fr (public demo — credentials on the server, never in the repo).

## Core Value

Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

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

### Active

<!-- Milestone v1.1.0 — current scope (2026-10-28). -->

- [ ] Phase 12 — Demo account lock (server refuse + popup) + per-vault display options: keep-page-numbers toggle, mdBook theme select — REQ-DEMO-*, REQ-VOPT-*
- [ ] Phase 13 — Wiki-changes log: `vault_changes` table fed at refresh + "Wiki changes" section in Statistics — REQ-CHG-*
- [ ] Phase 14 — Local vault upload: client file picker → convert/build/register — REQ-LOC-*

### Out of Scope

- Real-time collaborative editing — out of v1, WebSocket/CRDT complexity not justified
- Native mobile apps — responsive web is enough
- SSO (Google/Microsoft OAuth) — bcrypt + email covers the self-hosted need
- Webhooks / public API for vault management — WebDAV + admin UI cover the use case
- Encryption at rest — relying on the server's FS

## Context

- Stack: Rust stable, axum, sqlx (SQLite), mdbook (upstream CLI, called as a subprocess), rmcp (coming), lettre. Deployed with systemd + nginx on holldemo.
- Vault pipeline: sync → mirror in `<source>/.morinet-build/` (frontmatter consumed, wikilinks resolved, SUMMARY.md + book.toml generated) → `mdbook build` → served. mdBook themes are frozen at build: after changing `assets/zoom.*` or `MORINET_CSS`, you must click **Refresh** (pitfall #24).
- 197 tests, clippy `-D warnings`, fmt clean on every push. CI Linux-only since v0.1.16 (flaky Mac runners).
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
| GSD adopted as the planning mode (2026-10-25) | Toolled plan/execute/verify workflows; existing docs kept (ROADMAP/REQUIREMENTS/HANDOFF/STATE-LOG) | — Pending |

---
*Last updated: 2026-10-25 — GSD bootstrapped from the existing planning docs (v0.1.20, phases 00→09b shipped).*
