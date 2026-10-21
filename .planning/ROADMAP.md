# MoriNet — Roadmap

> Obsidian second brain exposed as an mdbook-like server with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP. All in Rust, `cargo install morinet`.

**Repo**: `github.com/lejeunecedric/MoriNet`
**Stack**: Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Delivery format**: static binary via `cargo install morinet`

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
| 08b  | **obsidian-format** | In-pipeline converter : vault llm-wiki/Obsidian → arbre mdBook (SUMMARY + wikilinks + frontmatter) | REQ-OBSFMT-001..006 | ✅ done (v0.1.8) |
| 09   | **ux-nav**        | New shell: My vaults landing, account menu top-right, mdbook close button, admin menu    | REQ-ACC-*, REQ-NAV-*   | ✅ done (v0.1.12/v0.1.14) |
| 10   | **mcp**           | MCP StreamableHttp on `/mcp`, list/read/search tools                                      | REQ-MCP-*              | ⬜ |
| +11  | **polish**        | Clean one-line installer, full README, badges, Release v1.0                              | REQ-OPS-*              | ⬜ |

**Ops done alongside the roadmap** (REQ-OPS-006..008): CI → cargo-dist release → auto-deploy to holldemo (`morinet.treklair.fr`). v0.1.1 binary installed on holldemo; systemd/nginx/TLS provisioning owned by vops (brief lives in the vops workspace, demo instance creds live on the server — not in this public repo).

---

## Phase 0 — bootstrap

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

## Phase 1 — core-serve

**Goal**: a single hardcoded admin (via env var at boot) can log in and see ONE compiled vault at `data/vaults/demo/book/`.

**Criteria**:
- `morinet serve --vault data/vaults/demo/book` starts a server on `:8080`
- `GET /` redirects to `/login`
- `POST /login` with `admin@example.com / [REDACTED-DEV-PASSWORD]` (env `MORINET_ADMIN_*`) sets a signed HttpOnly cookie
- `GET /vault/*` serves the compiled mdbook files, gated by the cookie

## Phase 2 — multi-vault

**Goal**: the admin can register N vaults (`name`, `slug`, local `path`) and each user sees `/vault/<slug>/` for the vaults they have access to.

**Criteria**:
- `vaults` table (id, slug, name, path, created_at)
- `vault_acl` table (vault_id, user_id)
- `GET /` (logged in) lists the user's accessible vaults with links
- `/vault/<slug>/*` returns 403 if not in the ACL, otherwise serves files

## Phase 3 — vault-sources

**Goal**: add a `GithubVaultSource` plugin (clone private/public git + periodic sync).

**Criteria**:
- `VaultSource` trait with `sync()` (idempotent) → returns a local path
- `LocalVaultSource` (resolves the path as-is)
- `GithubVaultSource` (clone via `git2` or shelling out `git`, periodic sync at configurable interval)
- mdbook rebuild triggered after a successful sync
- Tests: use a fixture test repo

## Phase 4 — admin-ui

**Goal**: HTML pages served by axum (no SPA) to manage users + vaults + perms.

**Criteria**:
- `GET /admin/users`: table with an "Invite" button
- `POST /admin/users/invite { email }`: creates a user, generates a magic link
- `GET /admin/vaults`: table with "Add vault" button
- `GET /admin/vaults/<slug>/perms`: ACL editor (user checkboxes)
- Templates via Askama or Maud, minimal hand-written CSS

## Phase 5 — email

**Goal**: configurable generic SMTP, send invitations and password reset emails.

**Criteria**:
- `[smtp]` section in `config.toml`: host, port, user, pass, from, tls
- `EmailSender::send(to, subject, body_html)` via `lettre`
- Templates: `invitation.html.tera`, `reset_password.html.tera`
- Tests: `MockSmtpServer` trait, impl `InMemoryMock`

## Phase 6 — webdav

**Goal**: WebDAV exposure at `/webdav/<vault-slug>/*` with PROPFIND/GET/PUT, Basic auth (mdbook email + password).

**Criteria**:
- `PROPFIND`: returns the list of `.md` files in the vault (depth 1 = vault, depth 0 = file)
- `GET`: returns the raw `.md` (text/markdown)
- `PUT`: allowed for admin only (overwrites the `.md` and rebuilds mdbook for that vault)
- `MKCOL` / `DELETE`: out of scope v1 (read-only-ish)
- Tests: curl WebDAV + a `cadaver` walkthrough

## Phase 7 — vault-ux ✅ DONE (v0.1.15)

**Goal**: adding a local vault must not require typing a path on the server. An admin clicks **Browse**, walks the server's directories, and picks one.

**Status**: livré (v0.1.15). Le `<dialog>` natif liste les dossiers du serveur (`GET /admin/browse`), descend / remonte (breadcrumb + Up), **Select** remplit path/slug/name. L'import browser-side (`<input webkitdirectory>`) téléverse les fichiers dans le vault choisi — le chemin navigateur n'est jamais cru côté serveur. Pièges : les sélecteurs `[name="name"]` globaux croisent le champ de renommage du compte → scoped ; `root.join(path)` avec un chemin absolu **remplace** la racine → `trim_start_matches('/')`.

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

## Phase 8 — access-model ✅ DONE (v0.1.9 → v0.1.13)

**Status**: livré. Groups + personal vaults + quotas (v0.1.9), load GitHub vault en espace personnel (v0.1.10), delete owner-only + refresh (v0.1.11), autofill name (v0.1.12), `source_url` retrouvé depuis le remote git (v0.1.13).

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

**Quota par groupe (user decision 2026-10-21, feedback UX admin)**:
- `groups.personal_vault_limit` (NULL = illimité) — REQ-GRP-005
- Builtin au bootstrap : `free` (limit **2**), `admin` (illimité) — REQ-GRP-006
- **`demo` devient un `reader` membre de `free`** (plus admin) — REQ-GRP-007 ; l'admin
  du bootstrap est membre de `admin`. C'est CE qui fait disparaître le bouton Admin
  pour demo (le code index/`require_admin` était déjà correct — demo était admin en base).
- La limite compte les vaults **personnels créés**, pas les partages ni la visibilité — REQ-VLT-008
- Limite effective = la plus permissive des groupes du user — REQ-GRP-008

---

## Phase 9 — ux-nav ✅ DONE (v0.1.14 → v0.1.20)

**Goal**: the new shell.

**Status**: livré. Nav shell (v0.1.14) → restyle (v0.1.16) → passes design
Impeccable/Linear (v0.1.17-v0.1.20, voir « Phase 9b — design system »).
Le header porte **un seul contrôle** (menu compte : identité → `/account`,
entrée Admin, logout) ; la tab bar admin est la seule nav des sections ;
les pages mdBook portent la barre zoom + « Close vault » en haut à droite
(sous le bouton Print, href `/`).

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

## Phase 9b — design system ✅ DONE (v0.1.16 → v0.1.20)

**Goal**: un UI sans slop, mesuré et vérifiable.

Passes **Impeccable** (`/extract`, `/polish`, `/critique`) + vocabulaire
**Linear** (skill `popular-web-designs`) + règles du skill `ui-design-passes` :

- **v0.1.16** — tokens de thème light/dark, panels, formulaires, pill nav
- **v0.1.17** — `/extract` : un seul menu, nav non dupliquée, CSS consolidé
  (radius/spacing scales, `--on-accent`, flat forms), MENU_JS (un menu ouvert
  à la fois, Échap/clic extérieur)
- **v0.1.18** — chrome vault : Close vault fonctionnel (href `/`), barre
  top-right unique sous le bouton Print (mesuré : pas de chevauchement)
- **v0.1.19** — `/critique` mdBook : detector `npx impeccable detect` sur
  snapshots CSS-inlinés → meta 13px sans fade, line-height 1.65, wikilinks
  pleins, `--icons` dark 4:1, police système
- **v0.1.20** — anti-clutter : 1 contrôle header (2 boutons admin = showstopper
  signalé), titres jamais dupliqués, sections complètes (empty states inclus),
  vaults en rangées, form de rename sur `/account` (pas de form dans un menu)

**Règles design tenues** (détail dans le skill `ui-design-passes`) : pas de
texte en opacity réduite, corps ≥ 13px / line-height 1.55-1.7, pas de carte
dans une carte, liens de nav une seule fois, identité rendue une fois, overlays
fixes jamais par-dessus un contrôle existant, un seul accent, close/back → la
surface parente.

---

## Phase 10 — mcp

**Goal**: MCP StreamableHttp on `/mcp`, Bearer auth (token derived from password), tools on accessible vaults.

**Criteria**:
- `/mcp` endpoint (StreamableHttp)
- Tools exposed: `list_vaults`, `list_notes(slug, path?)`, `read_note(slug, path)`, `search_notes(query, slug?)`
- Resource template `vault://<slug>/...` (optional)
- JSON schema returned by the tools
- Tests via the `mcp-cli` client when available

---

## Phase 11 — polish (v1.0)

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
