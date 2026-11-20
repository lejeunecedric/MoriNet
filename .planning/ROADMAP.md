# MoriNet — Roadmap

> Obsidian second brain exposed as an mdbook-like server with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP. All in Rust, `cargo install morinet`.

**Repo**: `github.com/lejeunecedric/MoriNet`
**Stack**: Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Delivery format**: static binary via `cargo install morinet`

---

## Milestones

- ✅ **v1.0** — Phases 00–11 (+10.1) — shipped 2026-10-25 (`v1.0.0`), hotfixes v1.0.1–v1.0.3 by 2026-10-28
- ✅ **v1.1.0 demo hardening + vault options** — Phases 12–14 — shipped 2026-10-29 (`v1.1.0`, live on holldemo) → full phase details: [milestones/v1.1.0-ROADMAP.md](milestones/v1.1.0-ROADMAP.md)
- ✅ **v1.2 Team Access** — Phases 15–17 — shipped 2026-11-06 (`v1.3.0`, live on holldemo) → full phase details: [milestones/v1.2-ROADMAP.md](milestones/v1.2-ROADMAP.md)
- ✅ **v1.3 OKF Bundle Support** — Phase 18 (+18.1) — shipped 2026-11-06 (released as `v1.4.0`; issue #11 closed) → full phase details: [milestones/v1.3-ROADMAP.md](milestones/v1.3-ROADMAP.md)
- ✅ **v1.4 Statistics & Observability** — Phases 19–20 — shipped 2026-11-20 (issue #10 closed) → full phase details: [milestones/v1.4-ROADMAP.md](milestones/v1.4-ROADMAP.md)

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
- [x] **Phase 15: etag-conflict** - Strong ETags + If-Match/If-None-Match on WebDAV; upload conflict list + force-overwrite (v1.2) (completed 2026-10-30)
- [x] **Phase 16: rest-api** - Read-only `/api/v1` (vaults/notes/read/search) on API tokens, ACL parity with MCP (v1.2) (completed 2026-11-01)
- [x] **Phase 17: oidc-sso** - Generic OIDC SSO (auth-code + PKCE) coexisting with password login (v1.2) — **completed 2026-11-06**
- [x] **Phase 18: okf-bundle-support** - OKF detection + conformance badge, bundle export (zip), API/MCP frontmatter metadata (v1.3)
- [x] **Phase 18.1: tech-debt-cleanup** - fmt_date fix + phase-18 LOW test gaps (v1.3) (completed 2026-11-06)
- [x] **Phase 19: statistics-observability** (completed 2026-11-18) - access logging on all surfaces (web/API/MCP), top viewed pages, MCP request↔response activity, top search keywords (issue #10)
- [x] **Phase 20: okf-ecosystem-quick-wins** (completed 2026-11-20) - docs vault passes okflint + OKForest, conformance badge in README, viz.html shipped in bundle export

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

<details>
<summary>✅ v1.2 Team Access (Phases 15–17) — SHIPPED 2026-11-06 (`v1.3.0`, live on holldemo)</summary>

1. **Phase 15 — etag-conflict** (CONF-01..05): strong content-hash ETags on WebDAV reads + PROPFIND `getetag`; fail-closed `If-Match`/`If-None-Match` enforcement on PUT (412/304/400); atomic writes + per-vault lock (no TOCTOU); upload conflict gate (409 + per-file list) with audited `force=1` overwrite; "Load local" two-phase UX. Migration 0011 (`vault_changes.forced/actor`).
2. **Phase 16 — rest-api** (API-01..06): read-only JSON REST API on `/api/v1` — Bearer API tokens only (session cookies rejected), ACL parity with MCP via shared helpers in `src/api.rs`, RFC 9457 `problem+json` errors, opaque cursor pagination, search, raw-markdown note read with ETag/304, per-token rate limiter (SHA-256 keyed), public `/api/v1/docs`. No new dependencies.
3. **Phase 17 — oidc-sso** (SSO-01..06): generic OIDC SSO — auth-code + PKCE S256, JWKS validation, `[oidc]` config (env-wins, disabled by default), migration 0012 (`user_identities` + `auth_events`), verified-email auto-link (admin refusal), domain-gated reader provisioning, replace-on-login group sync with last-admin guard, byte-identical session mint with password login. Only new dep: `openidconnect` (rustls-only).

**Exit verified:** 493 tests green, `check.sh` OK, per-phase VERIFICATION 7/7 ×3 + SECURITY 67/67 threat blocks closed, milestone audit INTEGRATED (`v1.2-MILESTONE-AUDIT.md`). Releases v1.2.0 → v1.3.0 all deployed via GitHub Actions. Deferred at the time: rclone/Obsidian + real-IdP human smoke tests (resolved 2026-11-20), G1-G3 composed tests. Full details: [milestones/v1.2-ROADMAP.md](milestones/v1.2-ROADMAP.md).

</details>

<details>
<summary>✅ v1.3 OKF Bundle Support (Phase 18 + 18.1) — SHIPPED 2026-11-06 (released `v1.4.0`)</summary>

1. **Phase 18 — okf-bundle-support** (OKF-01..04): OKF conformance scan at refresh/register (per-file `type` frontmatter, reserved `index.md`/`log.md` shape, v0.1/v0.2 version guess), vault-page conformance badge, "Export as OKF bundle" zip (byte-identical source + in-memory root `index.md` synthesis, read-ACL gated), concept metadata (`type`/`tags`/`status`/trust tier) on `/api/v1` + MCP `type`/`tags` filters. Issue #11 closed.
2. **Phase 18.1 — tech-debt-cleanup** (DEBT-1..4 from the v1.3 audit): `fmt_date` Hinnant fixes + phase-18 LOW test gaps (`457173e`, `4fbc2b8`, `c508835`).

**Exit verified:** 554 tests green, VERIFICATION 7/7, milestone audit `tech_debt` with all DEBT items closed by 18.1. Full details: [milestones/v1.3-ROADMAP.md](milestones/v1.3-ROADMAP.md).

</details>

<details>
<summary>✅ v1.4 Statistics & Observability (Phases 19–20) — SHIPPED 2026-11-20</summary>

1. **Phase 19 — statistics-observability** (STAT-01..05): migration 0014 widens `access_log.source` CHECK (adds `api`); access events logged best-effort on web/API/MCP; Statistics page gains Top pages, MCP activity (request↔outcome two-column, newest-first, excerpt + size only, never full bodies), and Top search keywords; 90-day boot prune. Issue #10 closed.
2. **Phase 20 — okf-ecosystem-quick-wins** (OKFE-01..02): docs vault passes okflint 0.5.1 + okforest 0.14.0 at the 0-errors/0-warnings bar (evidence in 20-EVIDENCE.md); optional local validator gate `scripts/check-okf.sh` wired into `scripts/check.sh` (no GitHub CI); README conformance badge scoped honestly to the docs vault; official offline OKF viewer vendored (pinned `527e81a`) and synthesized as root `viz.html` into every bundle export (vault's own viz.html wins; T-18-10 caps enforced).

**Exit verified:** 611 tests green, `check.sh` OK, per-phase VERIFICATION 19: 6/6 + 20: 5/5, SECURITY 20: 17/17 threats closed, milestone audit PASSED (7/7 requirements, 11/11 integration, 3/3 E2E flows — `v1.4-MILESTONE-AUDIT.md`). Both v1.2-deferred human UATs resolved with real clients (rclone v1.76.0 WebDAV smoke, oidc-provider v9.15.1 real-IdP SSO smoke). Full details: [milestones/v1.4-ROADMAP.md](milestones/v1.4-ROADMAP.md).

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
| 15. etag-conflict | v1.2 | 5/5 | Complete    | 2026-10-30 |
| 16. rest-api | v1.2 | 5/5 | Complete    | 2026-11-01 |
| 17. oidc-sso | v1.2 | 5/5 | Complete    | 2026-11-06 |
| 18. okf-bundle-support (+18.1) | v1.3 | 6/6 + 1/1 | Complete | 2026-11-06 |
| 19. statistics-observability | v1.4 | 4/4 | Complete    | 2026-11-18 |
| 20. okf-ecosystem-quick-wins | v1.4 | 6/6 | Complete    | 2026-11-20 |

**Next milestone candidates:** release v1.5.0 for phases 19–20 (Cargo.toml still says 1.4.0 — collides with the 2026-11-06 `v1.4.0` tag, bump required); OKF-F01 trust-aware rendering (needs Impeccable pass); G1-G3 composed E2E tests from the v1.2 audit; write endpoints (API-F01) / scoped tokens (API-F02) / webhooks (API-F03) backlog.

## Out of scope (standing)

- Real-time collaborative editing
- Native mobile apps
- ~~SSO (Google / Microsoft OAuth)~~ → delivered generically in v1.2 (Phase 17, OIDC)
- ~~Webhooks / public API to manage vaults programmatically~~ → v1.2 adds a **read-only** REST API (Phase 16); write/webhook management stays out
- Encryption at rest (rely on the server's FS)
- Attested Computation *execution* (spec defers the runtime protocol; running bundle code is a security non-starter)

> These items can move into a phase if a strong request emerges.
