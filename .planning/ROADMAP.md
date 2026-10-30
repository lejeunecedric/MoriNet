# MoriNet — Roadmap

> Obsidian second brain exposed as an mdbook-like server with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP. All in Rust, `cargo install morinet`.

**Repo**: `github.com/lejeunecedric/MoriNet`
**Stack**: Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Delivery format**: static binary via `cargo install morinet`

---

## Milestones

- ✅ **v1.0** — Phases 00–11 (+10.1) — shipped 2026-10-25 (`v1.0.0`), hotfixes v1.0.1–v1.0.3 by 2026-10-28
- ✅ **v1.1.0 demo hardening + vault options** — Phases 12–14 — shipped 2026-10-29 (`v1.1.0`, live on holldemo) → full phase details: [milestones/v1.1.0-ROADMAP.md](milestones/v1.1.0-ROADMAP.md)
- 🚧 **v1.2 Team Access** — Phases 15–17 — planned (ETag conflict detection → read-only REST API → OIDC SSO)

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
- [ ] **Phase 15: etag-conflict** - Strong ETags + If-Match/If-None-Match on WebDAV; upload conflict list + force-overwrite (v1.2)
- [ ] **Phase 16: rest-api** - Read-only `/api/v1` (vaults/notes/read/search) on API tokens, ACL parity with MCP (v1.2)
- [ ] **Phase 17: oidc-sso** - Generic OIDC SSO (auth-code + PKCE) coexisting with password login (v1.2)

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
<summary>🚧 v1.2 Team Access (Phases 15–17) — PLANNED (roadmap created 2026-10-30)</summary>

**Build order rationale** (all three researchers independently converged, see `.planning/research/SUMMARY.md`): ETag first (self-contained, no new deps, its `etag_of()` hash helper feeds the later phases), REST API second (pure reuse of MCP store queries, exposes Phase 15's ETags), OIDC last (only new dependency tree `openidconnect` 4.0.1, only migration, and the only change that can break login for existing users — lands on a stable base).

### Phase 15: etag-conflict — Conflict-safe co-editing (CONF-01..05)

**Goal**: stop silent last-write-wins clobbering. Strong content-hash ETags on WebDAV reads, `If-Match`/`If-None-Match` enforcement on PUT, and upload-time conflict detection with an explicit force-overwrite path — all backward compatible for clients that never send preconditions.

**Success criteria**:
1. WebDAV GET responses carry a strong ETag = sha256 of the raw **source** markdown bytes (never mtime, never `W/`, never mdBook build output) — CONF-01
2. PROPFIND responses include `getetag` per file — CONF-01
3. PUT with non-matching `If-Match` → 412 Precondition Failed: nothing written, no rebuild, no `vault_changes` row; malformed precondition header → 400 — CONF-02
4. PUT with `If-None-Match: *` creates the file only if absent (412 if it exists); successful conditional PUT returns the new ETag — CONF-03
5. Local upload whose tree is stale vs server → rejected with a per-file conflict list; re-submit with explicit force-overwrite flag succeeds and force-overwrites are recorded in `vault_changes` — CONF-04
6. Unconditional writes (no precondition headers, plain uploads) behave exactly as before — rclone/Obsidian regression green — CONF-05
7. Check→write is race-safe: per-vault in-process write lock + temp-file/atomic-rename via one shared "write source file" helper used by WebDAV PUT and upload (pitfall C3/C5)

**Avoids**: C1 (ETag on wrong layer), C2 (weak ETags), C3 (TOCTOU race), C6 (designing for clients that never send If-Match).

### Phase 16: rest-api — Read-only REST API on `/api/v1` (API-01..06)

**Goal**: expose the MCP read surface as a JSON REST API authenticated with existing `/account` API tokens — same ACL, same read helpers, Bearer-only. New `src/api.rs` router branch; `mcp.rs` tools become thin wrappers over the extracted helpers so ACL stays single-sourced.

**Success criteria**:
1. `/api/v1` accepts Bearer API tokens only (via `user_for_api_token`); session cookies are rejected 401 on this surface — API-01
2. `GET /api/v1/vaults` lists exactly the vaults the token's user can access — API-02
3. `GET /api/v1/vaults/{slug}/notes` lists markdown notes with cursor pagination (`has_more`); `GET /api/v1/vaults/{slug}/notes/{path}` returns the raw markdown **source** (not build output) with the Phase 15 ETag header; paths canonicalized against vault escape — API-03
4. `GET /api/v1/search?q=` searches notes scoped to the token's accessible vaults — API-04
5. A vault the user cannot see answers 404 (not 403) on every endpoint including search and path reads; all errors are RFC 9457 `application/problem+json` — API-05
6. Per-token rate limiting returns 429 with `Retry-After` — API-06
7. ACL test matrix ported from MCP `tools_are_acl_scoped` passes on all four endpoints — API-05

**Avoids**: B1 (ACL bypass), B2 (token leakage), B3 (serving build output), B4 (cookie/Bearer auth mixing). No new dependencies (axum `Json` + serde DTOs).

### Phase 17: oidc-sso — Generic OIDC SSO (SSO-01..06)

**Goal**: teams sign in through their own OIDC-compliant IdP (Keycloak / Authentik / Authelia) via auth-code + PKCE, coexisting with password login. `src/oidc.rs` + `[oidc]` config section (env-wins, disabled by default) + migration 0011 (`users.provider`); callback mints the **same** HMAC session cookie as password login so downstream middleware is untouched. Only new dependency: `openidconnect` 4.0.1 (reqwest + rustls-tls). Intra-phase order: password-login regression tests first, then linking/provisioning, then group sync last.

**Success criteria**:
1. Login page shows an SSO button when `[oidc]` is configured; flow is authorization-code + PKCE (S256) with `state` + `nonce`, discovery + JWKS-validated ID token — SSO-01
2. A user whose `email_verified` IdP email matches an existing local account links + signs in; admin accounts are never auto-linked; every link is logged; `(iss, sub)` is the authoritative identity, email is never the join key — SSO-02
3. First-time SSO user with an `allowed_email_domains` email gets an account auto-created with the `reader` role; unmatched email fails closed — SSO-03
4. Configurable groups claim (claim name + mapping in config) syncs to MoriNet groups at each login, read only from the signature-verified ID token; admin-group mapping is opt-in and fail-closed on absent claim; demotions are logged — SSO-04
5. `[oidc]` section in config.toml (issuer, client id/secret, allowed domains, claim mapping) with env overrides following the env-wins pattern; SSO disabled by default and removable without affecting password login — SSO-05
6. Password login keeps working for every account including SSO users; bootstrap admin always remains available (IdP-outage safety net) — SSO-06
7. Migration 0011 (`users.provider` + unguessable bcrypt placeholder, no table rebuild) applies cleanly on existing databases; password-login regression suite green before and after

**Avoids**: A1 (email auto-link takeover), A2 (state/nonce/PKCE gaps), A5 (group-claim escalation), A6 (session fixation), A7 (gates bypassed by auto-provision), X1 (hand-rolled OIDC). Open decision for the phase plan: group-sync semantics additive vs replace-on-login (research Gaps to Address).

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
| 15. etag-conflict | v1.2 | 2/5 | In Progress|  |
| 16. rest-api | v1.2 | 0/TBD | Planned | - |
| 17. oidc-sso | v1.2 | 0/TBD | Planned | - |

**Next milestone:** v1.2 Team Access — Phase 15 (etag-conflict) planning via `/gsd-plan-phase`.

## Out of scope (v1)

- Real-time collaborative editing
- Native mobile apps
- ~~SSO (Google / Microsoft OAuth)~~ → delivered generically in v1.2 (Phase 17, OIDC)
- ~~Webhooks / public API to manage vaults programmatically~~ → v1.2 adds a **read-only** REST API (Phase 16); write/webhook management stays out
- Encryption at rest (rely on the server's FS)

> These items can move into a phase if a strong request emerges.
