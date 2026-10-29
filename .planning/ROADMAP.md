# MoriNet — Roadmap

> Obsidian second brain exposed as an mdbook-like server with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP. All in Rust, `cargo install morinet`.

**Repo**: `github.com/lejeunecedric/MoriNet`
**Stack**: Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Delivery format**: static binary via `cargo install morinet`

---

## Milestones

- ✅ **v1.0** — Phases 00–11 (+10.1) — shipped 2026-10-25 (`v1.0.0`), hotfixes v1.0.1–v1.0.3 by 2026-10-28
- ✅ **v1.1.0 demo hardening + vault options** — Phases 12–14 — shipped 2026-10-29 (`v1.1.0`, live on holldemo) → full phase details: [milestones/v1.1.0-ROADMAP.md](milestones/v1.1.0-ROADMAP.md)

## Phases

<!-- GSD-parseable checklist. Full historical details archived under .planning/milestones/. -->

- [x] **Phase 00: bootstrap** - Repo + Cargo + CI + first release (v0.1.0)
- [x] **Phase 01: core-serve** - Serve ONE static vault behind login (v0.1.1)
- [x] **Phase 02: multi-vault** - Sub-paths, index, per-vault perms (v0.1.1)
- [x] **Phase 03: vault-sources** - Local folder OR GitHub repo (clone + sync)
- [x] **Phase 04: admin-ui** - Invite users, reset passwords, manage vaults (v0.1.4+)
- [x] **Phase 05: email** - SMTP + invitations + password reset
- [x] **Phase 06: webdav** - PROPFIND/GET/PUT on vault sources (HTTP Basic + ACL)
- [x] **Phase 07: vault-ux** - Dir picker, browser import, GitHub URL field (v0.1.10..15)
- [x] **Phase 08: access-model** - Groups + personal vaults + quotas (v0.1.9/11/12)
- [x] **Phase 08.1: obsidian-format** - In-pipeline llm-wiki → mdBook converter (v0.1.8)
- [x] **Phase 09: ux-nav** - Nav shell + design system (v0.1.14..20)
- [x] **Phase 10: mcp** - MCP StreamableHttp on `/mcp`, list/read/search tools (2026-10-25)
- [x] **Phase 10.1: home-ux** - Vault actions, share by email, statistics, docs vault
- [x] **Phase 11: polish** - Installer, README, `morinet init`, release v1.0.0 (2026-10-25)
- [x] **Phase 12: demo-lock + vault display options** - demo account guard; per-vault page-numbers toggle + mdBook theme (v1.1.0) (completed 2026-10-28)
- [x] **Phase 13: wiki-changes log** - record vault file changes at refresh; "Wiki changes" section in Statistics (v1.1.0) (completed 2026-10-29)
- [x] **Phase 14: local-vault-upload** - "Local vault" button: client file picker → upload → register (v1.1.0) (completed 2026-10-29)

<details>
<summary>✅ v1.0 (Phases 00–11) — SHIPPED 2026-10-25 (v1.0.0; hotfixes v1.0.1–v1.0.3)</summary>

- Phase 00 bootstrap: repo + cargo-dist CI + `cargo install` — v0.1.0
- Phase 01 core-serve: login (HttpOnly + HMAC cookie) + one compiled vault — v0.1.1
- Phase 02 multi-vault: `vaults` + `vault_acl`, `/vault/<slug>/` index — v0.1.1
- Phase 03 vault-sources: `VaultSource` trait, local + GitHub (git2 clone/sync)
- Phase 04 admin-ui: invite/reset/vaults/perms pages
- Phase 05 email: `[smtp]` + lettre, invitations + password reset
- Phase 06 webdav: PROPFIND/GET/PUT on vault source, HTTP Basic + ACL
- Phase 07 vault-ux: server-side dir picker `<dialog>`, browser import, GitHub URL — v0.1.15
- Phase 08 access-model: groups + personal vaults + per-group quotas (`free`=2, `admin`=∞) — v0.1.9..13
- Phase 08.1 obsidian-format: in-pipeline llm-wiki/Obsidian converter (AD-14), validated on ProgWiki (131 .md / 812 wikilinks) — v0.1.8
- Phase 09 ux-nav: My vaults landing, account menu, zoom + Close vault chrome — v0.1.14
- Phase 09.1 design system: Impeccable + Linear passes, one header control — v0.1.16..20
- Phase 10 mcp: StreamableHttp `/mcp`, Bearer API tokens, 4 tools — 2026-10-25
- Phase 10.1 home-ux: Edit/Refresh/Delete/Share/Statistics cards, `access_log`, default docs vault
- Phase 11 polish: README, `morinet init`, tag v1.0.0 + GitHub release — 2026-10-25

</details>

<details>
<summary>✅ v1.1.0 demo hardening + vault options (Phases 12–14) — SHIPPED 2026-10-29</summary>

- Phase 12 demo-lock + vault display options (3/3 plans, completed 2026-10-28): `[security] locked_accounts` + `MORINET_LOCKED_ACCOUNTS` env; server-side refusal on load/upload/delete/edit/share + home popup; per-vault keep-page-numbers toggle + mdBook theme select applied at SERVE time (locked decision D-08/D-10, supersedes book.toml approach); issue #7 closed
- Phase 13 wiki-changes log (2/2 plans, completed 2026-10-29): `vault_changes` table (migration 0010), git tree-diff capture at Refresh + WebDAV PUT capture, best-effort, 90-day boot prune; "Wiki changes" section in Statistics grouped per refresh event
- Phase 14 local-vault-upload (1/1 plan, completed 2026-10-29): home "Local vault" directory picker → `/vaults/upload` multipart endpoint (sanitizer, 5 MB/file + 2000 files + route-scoped 256 MB body limit), lock/quota gates before disk write, shared LocalSource convert/build pipeline, `vault_source` stays None (no Refresh for uploads)

</details>

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 00–09 (incl. 08.1, 09.1) | v1.0 | pre-GSD | Complete | v0.1.0 → v0.1.20 |
| 10. mcp (+10.1) | v1.0 | 2/2 | Complete | 2026-10-25 |
| 11. polish | v1.0 | — | Complete | 2026-10-25 |
| 12. demo-lock + display options | v1.1.0 | 3/3 | Complete | 2026-10-28 |
| 13. wiki-changes log | v1.1.0 | 2/2 | Complete | 2026-10-29 |
| 14. local-vault-upload | v1.1.0 | 1/1 | Complete | 2026-10-29 |

**Next milestone:** not yet planned — start with `/gsd-new-milestone`.

## Out of scope (v1)

- Real-time collaborative editing
- Native mobile apps
- SSO (Google / Microsoft OAuth)
- Webhooks / public API to manage vaults programmatically
- Encryption at rest (rely on the server's FS)

> These items can move into a phase if a strong request emerges.
