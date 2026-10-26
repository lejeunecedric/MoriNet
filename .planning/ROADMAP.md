# MoriNet — Roadmap

> Obsidian second brain exposed as an mdbook-like server with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP. All in Rust, `cargo install morinet`.

**Repo**: `github.com/lejeunecedric/MoriNet`
**Stack**: Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Delivery format**: static binary via `cargo install morinet`

---

## Phases

<!-- GSD-parseable checklist. Details for each phase live in the sections below. -->

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
- [ ] **Phase 11: polish** - Installer, README, `morinet init`, release v1.0.0

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 00–09 (incl. 08.1, 09.1) | pre-GSD | Complete | v0.1.0 → v0.1.20 |
| 10. mcp | 2/2 | Complete | 2026-10-25 |
| 11. polish | 0/TBD | Not started | - |

---

## Overview (10 phases)

| #    | Phase             | Goal                                                                                     | Key REQ-IDs            | Status |
|------|-------------------|------------------------------------------------------------------------------------------|------------------------|--------|
| 00   | **bootstrap**     | GitHub repo + Cargo init + CI + first release                                            | REQ-INFRA-*            | ✅ done (v0.1.0) |
| 01   | **core-serve**    | Serve ONE static vault via compiled mdbook, behind a simple login                        | REQ-AUTH-*, REQ-VAULT-001 | ✅ done (v0.1.1) |
| 02   | **multi-vault**   | Sub-paths `/vaultA/`, `/vaultB/`, `/` index, per-vault perms                              | REQ-VAULT-002..006     | ✅ done (v0.1.1) |
| 03   | **vault-sources** | Source plugin: local folder OR GitHub repo (clone + periodic sync)                       | REQ-VAULT-007..010     | ✅ done |
| 04   | **admin-ui**      | Admin page: invite users, reset passwords, manage vaults + perms                         | REQ-ADMIN-*            | ✅ done (v0.1.4+) |
| 05   | **email**         | SMTP adapter + email invitations + password reset                                        | REQ-EMAIL-*            | ✅ done |
| 06   | **webdav**        | WebDAV endpoints on vaults (same creds), browse + read                                    | REQ-WEBDAV-*           | ✅ done |
| 07   | **vault-ux**      | Vault management UX: server-side dir picker, browser-side import, URL field for GitHub   | REQ-UX-*               | ✅ done (v0.1.10..v0.1.15 — URL GitHub, picker, import) |
| 08   | **access-model**  | Groups + personal vaults: `group_members`, `vault_group_acl`, `scope`/`owner_id`          | REQ-GRP-*, REQ-VLT-*   | ✅ done (v0.1.9/v0.1.11/v0.1.12 — quotas, plans, load/refresh/delete) |
| 08.1 | **obsidian-format** | In-pipeline converter: llm-wiki/Obsidian vault → mdBook tree (SUMMARY + wikilinks + frontmatter) | REQ-OBSFMT-001..006 | ✅ done (v0.1.8) |
| 09   | **ux-nav**        | New shell: My vaults landing, account menu top-right, mdbook close button, admin menu    | REQ-ACC-*, REQ-NAV-*   | ✅ done (v0.1.12/v0.1.14) |
| 10   | **mcp**           | MCP StreamableHttp on `/mcp`, list/read/search tools                                      | REQ-MCP-*              | ⬜ |
| +11  | **polish**        | Clean one-line installer, full README, badges, Release v1.0                              | REQ-OPS-*              | ⬜ |

**Ops done alongside the roadmap** (REQ-OPS-006..008): CI → cargo-dist release → auto-deploy to holldemo (`morinet.treklair.fr`). v0.1.1 binary installed on holldemo; systemd/nginx/TLS provisioning owned by vops (brief lives in the vops workspace, demo instance creds live on the server — not in this public repo).

---

## Phase 00: bootstrap

**Status**: ✅ done — released as v0.1.0 (16 assets, CI green, cargo-dist pipeline).

**Goal**: lay the foundations — the repo exists on GitHub, the crate compiles, CI is green, the binary installs via `cargo install`.

**Success criteria**:

- `gh repo view lejeunecedric/MoriNet` → public repo exists ✅
- `cargo install --path .` from a cloned checkout succeeds (binary `morinet` available) ✅
- `morinet --version` returns `0.1.0` ✅
- GitHub Actions CI is green on push (fmt + lint + test + build) ✅
- First GitHub release created via cargo-dist ✅

**Delivered**: `.planning/phases/00-bootstrap/01-bootstrap-PLAN.md`

---

## Phase 01: core-serve

**Goal**: a single hardcoded admin (via env var at boot) can log in and see ONE compiled vault at `data/vaults/demo/book/`.

**Criteria**:

- `morinet serve --vault data/vaults/demo/book` starts a server on `:8080`
- `GET /` redirects to `/login`
- `POST /login` with the bootstrap admin from env `MORINET_ADMIN_EMAIL`/`MORINET_ADMIN_PASSWORD` (creds never recorded in this repo) sets a signed HttpOnly cookie
- `GET /vault/*` serves the compiled mdbook files, gated by the cookie

## Phase 02: multi-vault

**Goal**: the admin can register N vaults (`name`, `slug`, local `path`) and each user sees `/vault/<slug>/` for the vaults they have access to.

**Criteria**:

- `vaults` table (id, slug, name, path, created_at)
- `vault_acl` table (vault_id, user_id)
- `GET /` (logged in) lists the user's accessible vaults with links
- `/vault/<slug>/*` returns 403 if not in the ACL, otherwise serves files

## Phase 03: vault-sources

**Goal**: add a `GithubVaultSource` plugin (clone private/public git + periodic sync).

**Criteria**:

- `VaultSource` trait with `sync()` (idempotent) → returns a local path
- `LocalVaultSource` (resolves the path as-is)
- `GithubVaultSource` (clone via `git2` or shelling out `git`, periodic sync at configurable interval)
- mdbook rebuild triggered after a successful sync
- Tests: use a fixture test repo

## Phase 04: admin-ui

**Goal**: HTML pages served by axum (no SPA) to manage users + vaults + perms.

**Criteria**:

- `GET /admin/users`: table with an "Invite" button
- `POST /admin/users/invite { email }`: creates a user, generates a magic link
- `GET /admin/vaults`: table with "Add vault" button
- `GET /admin/vaults/<slug>/perms`: ACL editor (user checkboxes)
- Templates via Askama or Maud, minimal hand-written CSS

## Phase 05: email

**Goal**: configurable generic SMTP, send invitations and password reset emails.

**Criteria**:

- `[smtp]` section in `config.toml`: host, port, user, pass, from, tls
- `EmailSender::send(to, subject, body_html)` via `lettre`
- Templates: `invitation.html.tera`, `reset_password.html.tera`
- Tests: `MockSmtpServer` trait, impl `InMemoryMock`

## Phase 06: webdav

**Goal**: WebDAV exposure at `/webdav/<vault-slug>/*` with PROPFIND/GET/PUT, Basic auth (mdbook email + password).

**Criteria**:

- `PROPFIND`: returns the list of `.md` files in the vault (depth 1 = vault, depth 0 = file)
- `GET`: returns the raw `.md` (text/markdown)
- `PUT`: allowed for admin only (overwrites the `.md` and rebuilds mdbook for that vault)
- `MKCOL` / `DELETE`: out of scope v1 (read-only-ish)
- Tests: curl WebDAV + a `cadaver` walkthrough

## Phase 07: vault-ux ✅ DONE (v0.1.15)

**Goal**: adding a local vault must not require typing a path on the server. An admin clicks **Browse**, walks the server's directories, and picks one.

**Status**: shipped (v0.1.15). The native `<dialog>` lists the server's directories (`GET /admin/browse`), descends / goes up (breadcrumb + Up), **Select** fills path/slug/name. The browser-side import (`<input webkitdirectory>`) uploads the files into the chosen vault — the browser path is never trusted server-side. Pitfalls: global `[name="name"]` selectors intersect the account rename field → scoped; `root.join(path)` with an absolute path **replaces** the root → `trim_start_matches('/')`.

**User decision (2026-10-21)**: the picker browses **the server's filesystem**, not the admin's machine. A vault is a path on the host running MoriNet; offering a browser-side folder picker would upload a path string that does not exist there.

**Criteria**:

- `GET /admin/browse?path=<dir>` → JSON `{ current, parent, dirs[] }` (admin only)
- **Import**: a browser-side folder picker (`<input type=file webkitdirectory>`) uploads the selected files into the server-side vault chosen by the picker — the browser path is never trusted as a server path
- The "Add vault" form hides the path field behind a **Browse** button when `kind=local`
- Clicking a directory descends into it; "up" walks to the parent; a breadcrumb shows the current path
- Selecting a folder closes the modal and fills the path field
- GitHub vaults show a **URL** field only — no Browse button
- Traversal outside the configured root is refused (`400`); unauthenticated → `401`
- Readable error when the chosen directory is not a valid vault (no `book.toml`)

**Out of scope here**: upload, drag-and-drop, creating directories, file previews.

---

## Phase 08: access-model ✅ DONE (v0.1.9 → v0.1.13)

**Status**: shipped. Groups + personal vaults + quotas (v0.1.9), load GitHub vault into personal space (v0.1.10), delete owner-only + refresh (v0.1.11), autofill name (v0.1.12), `source_url` recovered from the git remote (v0.1.13).

**Goal** (user decision, 2026-10-21): abandon the merged-book idea and replace the flat ACL with **groups + personal vaults**.

**Model**:

| Table | Purpose |
|-------|---------|
| `groups` | admin-managed, named |
| `group_members` | user ↔ group |
| `vaults` + `scope` + `owner_id` | `global` (assigned to groups) or `personal` (solo, owner only) |
| `vault_group_acl` | **role per vault per group**: `read` or `write` |
| `users.display_name` | shown in the UI |
| `pending_emails` | unconfirmed email change |

A vault can be shared by several groups, each with its own level — B3Wiki as `read`, a team vault as `write`. A personal vault is strictly solo and its owner always has `write`.

**Existing rows migrate to `scope='global'` with no owner**, so nothing disappears.

**Criteria**:

- Migration adds the tables and the columns without losing existing vaults
- `list_vaults_for` returns: personal vaults of the user + global vaults where one of their groups has a row
- A personal vault of someone else is invisible (403 even on a guessed URL)
- Admins still see everything
- Existing behavior of the current admin pages keeps working

**Per-group quota (user decision 2026-10-21, admin UX feedback)**:

- `groups.personal_vault_limit` (NULL = unlimited) — REQ-GRP-005
- Built at bootstrap: `free` (limit **2**), `admin` (unlimited) — REQ-GRP-006
- **`demo` becomes a `reader` member of `free`** (no longer admin) — REQ-GRP-007; the
  bootstrap admin is a member of `admin`. This is what makes the Admin button
  disappear for demo (the index/`require_admin` code was already correct — demo was admin in the DB).

- The limit counts **personal vaults created**, not shares or visibility — REQ-VLT-008
- Effective limit = the most permissive of the user's groups — REQ-GRP-008

---

## Phase 08.1: obsidian-format ✅ DONE (v0.1.8)

**Goal**: load llm-wiki / Obsidian vaults (bare `[[wikilinks]]` + YAML frontmatter, no `book.toml`).

**Delivered**: in-pipeline converter (AD-14/PRD-6) — sync → `.morinet-build/` mirror (frontmatter consumed, wikilinks resolved, SUMMARY.md + book.toml generated, folder landing pages) → `mdbook build`. Validated on a real corpus (ProgWiki: 131 .md, 812 wikilinks, 542 aliases) + oracle `MORINET_CORPUS=… cargo test -- --ignored`. E2E verified in the browser, WebDAV PUT → live re-conversion. REQ-OBSFMT-001..006.

---

## Phase 09: ux-nav ✅ DONE (v0.1.14 → v0.1.20)

**Goal**: the new shell.

**Status**: shipped. Nav shell (v0.1.14) → restyle (v0.1.16) → design passes
Impeccable/Linear (v0.1.17-v0.1.20, see "Phase 09.1: design system").
The header carries **a single control** (account menu: identity → `/account`,
Admin entry, logout); the admin tab bar is the only section nav;
the mdBook pages carry the zoom bar + "Close vault" top-right
(below the Print button, href `/`).

**Criteria**:

- Shared header on every page: **My vaults** (landing) + account menu top-right
- Account menu: display name (editable), email, logout
- Logout clears the cookie and returns to `/login`
- The mdBook pages get a discreet close button (top-right) back to `/` — **DONE** (v0.1.12, injected via `theme/morinet.js`)
- A zoom bar (− / 100% / + / reset) on the mdBook pages, scaling the content AND the sidebar in lockstep, persisted in `localStorage` — **DONE** (v0.1.12, ported from oscbrain)
- The Admin menu appears only for role=admin (dropdown), linking to groups, users, global vaults
- Admin nav is NOT ambiguous: "My vaults" (landing) vs "Vaults mgmt" (admin) — no bare "Vaults" link
- Admin users page carries an **About** button showing the installed version (`morinet <version>`) and the GitHub repo URL — version updates with every release, no hardcoding

---

## Phase 09.1: design system ✅ DONE (v0.1.16 → v0.1.20)

**Goal**: a slop-free UI, measured and verifiable.

**Impeccable** passes (`/extract`, `/polish`, `/critique`) + **Linear** vocabulary
(skill `popular-web-designs`) + rules from the `ui-design-passes` skill:

- **v0.1.16** — light/dark theme tokens, panels, forms, pill nav
- **v0.1.17** — `/extract`: a single menu, non-duplicated nav, consolidated CSS
  (radius/spacing scales, `--on-accent`, flat forms), MENU_JS (one menu open
  at a time, Escape/outside click)

- **v0.1.18** — vault chrome: functional Close vault (href `/`), single
  top-right bar below the Print button (measured: no overlap)

- **v0.1.19** — `/critique` mdBook: `npx impeccable detect` detector on
  CSS-inlined snapshots → meta 13px without fade, line-height 1.65, solid
  wikilinks, `--icons` dark 4:1, system font

- **v0.1.20** — anti-clutter: 1 header control (2 admin buttons = reported
  showstopper), titles never duplicated, complete sections (empty states included),
  vaults as rows, rename form on `/account` (no form inside a menu)

**Design rules held** (details in the `ui-design-passes` skill): no
reduced-opacity text, body ≥ 13px / line-height 1.55-1.7, no card inside a
card, nav links only once, identity rendered once, fixed overlays never on top
of an existing control, a single accent, close/back → the parent surface.

---

## Phase 10: mcp ✅ DONE (2026-10-25)

**Goal**: MCP StreamableHttp on `/mcp`, Bearer auth (token derived from password), tools on accessible vaults.

**Criteria**:

- `/mcp` endpoint (StreamableHttp)
- Tools exposed: `list_vaults`, `list_notes(slug, path?)`, `read_note(slug, path)`, `search_notes(query, slug?)`
- Resource template `vault://<slug>/...` (optional)
- JSON schema returned by the tools
- Tests via the `mcp-cli` client when available

---

### Phase 10.1: Home UX — vault actions, share by email, statistics (INSERTED)

**Goal:** The landing page is the vault cockpit: every vault card exposes Edit / Refresh / Delete / Share / Statistics; "Shared with me" renders with the same card layout and actions as "My vaults"; "Load a vault" becomes its own section.
**Requirements**: REQ-VLT-009..012 (new)
**Depends on:** Phase 10
**Plans:** 0 plans

**Criteria:**

- Index redesign: section "My vaults" (cards with Edit, Refresh, Delete, Share, Statistics), section "Shared with me" (identical card layout/actions), section "Load a vault" (moved out of the My-vaults panel)
- **Edit**: rename vault + change GitHub source URL + per-vault `allow_reshare` toggle (owner decides whether sharees may re-share)
- **Share**: direct user-to-user sharing by email (new `vault_shares` table: vault, email, role reader/writer); sharee sees the vault in "Shared with me"; re-share only when `allow_reshare`
- **Statistics**: per-vault page — page count, disk size, last refresh, source URL + access counts (web page views + MCP tool calls) from a new lightweight `access_log` table (simplified first pass: counts per vault/day/source, top pages)
- Access logging wired into `vault_file`/`vault_index` and the 4 MCP tools (best-effort, never fails the request)
- ACL: `accessible_vaults` extended to union group grants + direct email shares; WebDAV + MCP honor shares
- Tests: share/reshare/ACL matrix + stats aggregation; UI parity check (same markup for both sections)
- **Default docs vault**: at boot, seed a global vault `morinet-docs` ("MoriNet Documentation") granted to every user (all groups, swept at boot like `ensure_default_memberships`), built from markdown embedded in the binary (`include_str!`) — idempotent, never overwrites operator edits; appears in "Shared with me" on first login

Plans:

- [ ] TBD (run /gsd-plan-phase 10.1 to break down)

**Cross-cutting constraints:**

- cargo test --lib green, clippy -D warnings clean

## Phase 11: polish (v1.0)

**Goal**: ready to ship.

**Criteria**:

- `cargo install morinet` works end-to-end (re-check README)
- Minimal/elegant README (5 sections: What / Install / Quick start / Status / License)
- `morinet init` creates an example `config.toml`
- Cargo.lock committed
- Tag `v1.0.0` set, GitHub release created via cargo-dist
- Announcement on the user's channel

---

## Out of scope (v1)

- Real-time collaborative editing
- Native mobile apps
- SSO (Google / Microsoft OAuth)
- Webhooks / public API to manage vaults programmatically
- Encryption at rest (rely on the server's FS)

> These items can move into a phase if a strong request emerges.
