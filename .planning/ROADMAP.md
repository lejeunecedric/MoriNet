# MoriNet — Roadmap

> Obsidian second brain exposed as an mdbook-like server with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP. All in Rust, `cargo install morinet`.

**Repo**: `github.com/lejeunecedric/MoriNet`
**Stack**: Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Delivery format**: static binary via `cargo install morinet`

---

## Overview (8 phases)

| #    | Phase             | Goal                                                                                     | Key REQ-IDs            | Status |
|------|-------------------|------------------------------------------------------------------------------------------|------------------------|--------|
| 00   | **bootstrap**     | GitHub repo + Cargo init + CI + first release                                            | REQ-INFRA-*            | ✅ done (v0.1.0) |
| 01   | **core-serve**    | Serve ONE static vault via compiled mdbook, behind a simple login                        | REQ-AUTH-*, REQ-VAULT-001 | ✅ done (v0.1.1) |
| 02   | **multi-vault**   | Sub-paths `/vaultA/`, `/vaultB/`, `/` index, per-vault perms                              | REQ-VAULT-002..006     | ✅ done (v0.1.1) |
| 03   | **vault-sources** | Source plugin: local folder OR GitHub repo (clone + periodic sync)                       | REQ-VAULT-007..010     | ✅ done |
| 04   | **admin-ui**      | Admin page: invite users, reset passwords, manage vaults + perms                         | REQ-ADMIN-*            | ✅ done (v0.1.4+) |
| 05   | **email**         | SMTP adapter + email invitations + password reset                                        | REQ-EMAIL-*            | ✅ done |
| 06   | **webdav**        | WebDAV endpoints on vaults (same creds), browse + read                                    | REQ-WEBDAV-*           | ✅ done |
| 07   | **mcp**           | MCP StreamableHttp on `/mcp`, list/read/search tools                                      | REQ-MCP-*              | ⬜ next |
| +08  | **polish**        | Clean one-line installer, full README, badges, Release v1.0                              | REQ-OPS-*              | ⬜ |

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

## Phase 7 — mcp

**Goal**: MCP StreamableHttp on `/mcp`, Bearer auth (token derived from password), tools on accessible vaults.

**Criteria**:
- `/mcp` endpoint (StreamableHttp)
- Tools exposed: `list_vaults`, `list_notes(slug, path?)`, `read_note(slug, path)`, `search_notes(query, slug?)`
- Resource template `vault://<slug>/...` (optional)
- JSON schema returned by the tools
- Tests via the `mcp-cli` client when available

## Phase +8 — polish

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
