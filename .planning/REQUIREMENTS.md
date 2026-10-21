# MoriNet — Requirements Catalog

> Each REQ-ID is verifiable via a documented command / test / curl.
> Format: `REQ-<DOMAIN>-<NNN>` — `<DOMAIN>` ∈ {INFRA, AUTH, VAULT, ADMIN, EMAIL, WEBDAV, MCP, OPS, OBS}.

---

## INFRA — Foundations

| REQ-ID          | Description                                                              | Verification                                   |
|-----------------|--------------------------------------------------------------------------|------------------------------------------------|
| REQ-INFRA-001   | Binary installs via `cargo install morinet`                              | `cargo install --path .` then `morinet --version` |
| REQ-INFRA-002   | GitHub Actions CI is green (fmt + lint + test + build)                   | push → GH Actions run success                  |
| REQ-INFRA-003   | GitHub release with cross-platform binary                                | `gh release list` shows vX.Y.Z                 |
| REQ-INFRA-004   | SQLite DB by default                                                     | `data/morinet.db` created at boot, schema migrated |
| REQ-INFRA-005   | Config via `config.toml`                                                 | `--config path/to/config.toml`                 |
| REQ-INFRA-006   | Structured logs via `tracing`                                            | `RUST_LOG=morinet=debug morinet serve`         |

## AUTH — Users & sessions

| REQ-ID        | Description                                                                     | Verification                              |
|---------------|---------------------------------------------------------------------------------|-------------------------------------------|
| REQ-AUTH-001  | A user has: email (unique), password_hash (bcrypt), role ∈ {admin, reader}, created_at | migrations + CRUD tests                   |
| REQ-AUTH-002  | Login `POST /login` accepts email + password, sets signed HttpOnly cookie       | curl login + cookie received              |
| REQ-AUTH-003  | axum middleware checks cookie on protected routes                               | curl non-auth → 401                       |
| REQ-AUTH-004  | Logout `POST /logout` clears the cookie                                         | curl logout + cookie absent               |
| REQ-AUTH-005  | Password reset: signed token with TTL, `/reset?token=...` validates, sets new password | end-to-end flow tested                |

## VAULT — Multi-vault & sources

| REQ-ID         | Description                                                            | Verification                              |
|----------------|------------------------------------------------------------------------|-------------------------------------------|
| REQ-VAULT-001  | A vault = a compiled mdbook directory (`book/index.html` exists)       | Phase 1: serve this directory via axum    |
| REQ-VAULT-002  | `vaults` table persisted, full CRUD                                    | sqlite + tests                            |
| REQ-VAULT-003  | Sub-paths `/vault/<slug>/` route to the right `book/`                  | curl 200 vs 404                           |
| REQ-VAULT-004  | ACL `vault_acl(vault_id, user_id)`                                     | non-ACL user → 403                        |
| REQ-VAULT-005  | `/` (logged in) lists accessible vaults                                | HTML snapshot                             |
| REQ-VAULT-006  | Vault switcher injected in the mdbook sidebar                          | custom menu injection                     |
| REQ-VAULT-007  | `VaultSource` trait (sync → local PathBuf)                             | unit tests with a mock impl               |
| REQ-VAULT-008  | `LocalVaultSource` (resolves the path as-is)                           | e2e test with a real path                 |
| REQ-VAULT-009  | `GithubVaultSource` (clone + periodic pull)                            | fixture GitHub repo + integration test    |
| REQ-VAULT-010  | mdbook rebuild after sync                                              | verify `book/` is regenerated             |

## ADMIN — Admin UI

| REQ-ID          | Description                                                          | Verification                      |
|-----------------|----------------------------------------------------------------------|-----------------------------------|
| REQ-ADMIN-001   | Admin gate: all `/admin/*` routes require `role = admin`             | curl `/admin/users` non-admin → 403 |
| REQ-ADMIN-002   | `GET /admin/users` page (table + invite button)                      | HTML snapshot                     |
| REQ-ADMIN-003   | `POST /admin/users/invite { email }` creates user + sends email       | mock SMTP + DB check              |
| REQ-ADMIN-004   | `GET /admin/vaults` page (table + add vault button)                 | HTML snapshot                     |
| REQ-ADMIN-005   | `GET /admin/vaults/<slug>/perms` page (ACL editor)                   | HTML snapshot                     |
| REQ-ADMIN-006   | `POST /admin/vaults/{create,delete,update-acl}` + tests              | integration                       |

## EMAIL — SMTP

| REQ-ID         | Description                                                | Verification                          |
|----------------|------------------------------------------------------------|---------------------------------------|
| REQ-EMAIL-001  | `[smtp]` section in `config.toml`                          | config loading tested                 |
| REQ-EMAIL-002  | `lettre` sends via configurable SMTP                       | test with a local mock SMTP           |
| REQ-EMAIL-003  | HTML invitation template                                   | generated snapshot                    |
| REQ-EMAIL-004  | HTML password reset template                               | generated snapshot                    |
| REQ-EMAIL-005  | Invitation link at TTL 24h, single-use                     | token expiry tests                    |

## WEBDAV — Vault access

| REQ-ID          | Description                                                            | Verification                          |
|-----------------|------------------------------------------------------------------------|---------------------------------------|
| REQ-WEBDAV-001  | Basic auth on `/webdav/<vault-slug>/` (mdbook email + password)        | curl with `-u`                        |
| REQ-WEBDAV-002  | `PROPFIND depth 0/1` returns the `.md` files of the vault              | cadaver / curl XML                   |
| REQ-WEBDAV-003  | `GET` returns the raw `.md` as `text/markdown`                         | curl + Content-Type                   |
| REQ-WEBDAV-004  | `PUT` allowed for admin only                                            | curl PUT as reader → 403              |
| REQ-WEBDAV-005  | Vault ACL respected on WebDAV (user without ACL → 401)                 | curl non-ACL → 401                    |

## FUSION — Per-user merged book

| REQ-ID        | Description                                                                  | Verification                          |
|---------------|------------------------------------------------------------------------------|---------------------------------------|
| REQ-FUSE-001  | Each user sees ONE merged mdbook containing only the vaults they may access   | reader A vs reader B see different books |
| REQ-FUSE-002  | Merged `SUMMARY.md` groups entries **by vault** (one top-level section each)   | generated SUMMARY snapshot           |
| REQ-FUSE-003  | The sidebar groups chapters by vault with a visible group heading             | browser screenshot                    |
| REQ-FUSE-004  | Revoking a vault ACL removes it from the merged book on next build           | grant → revoke → book changes        |
| REQ-FUSE-005  | Merged books are cached on disk under `data/books/<user>/`, reused if fresh   | second request does not rebuild       |
| REQ-FUSE-006  | Cache key includes the vault set + ACL state (no stale cross-user reads)     | different users → different dirs     |
| REQ-FUSE-007  | Vaults stay independently managed (own dir, own git, own sync)               | editing one vault does not touch another |
| REQ-FUSE-008  | A merged build never leaks a vault the user cannot read                       | reader without ACL → not in SUMMARY   |

## MCP — Model Context Protocol

| REQ-ID        | Description                                                                | Verification                          |
|---------------|----------------------------------------------------------------------------|---------------------------------------|
| REQ-MCP-001   | `/mcp` endpoint (StreamableHttp)                                           | curl initialize → 200                 |
| REQ-MCP-002   | Bearer auth (token derived from password)                                 | curl without token → 401              |
| REQ-MCP-003   | Tool `list_vaults()` → list of accessible vaults                          | invoke + assert                       |
| REQ-MCP-004   | Tool `list_notes(slug, path?)` → list of `.md` files                      | invoke + assert                       |
| REQ-MCP-005   | Tool `read_note(slug, path)` → `.md` contents                             | invoke + assert                       |
| REQ-MCP-006   | Tool `search_notes(query, slug?)` → results                               | invoke + assert                       |
| REQ-MCP-007   | Vault ACL respected on MCP side (user without ACL → empty list)          | invoke + assert                       |

## UX — Vault management UI

| REQ-ID        | Description                                                                    | Verification                  |
|---------------|--------------------------------------------------------------------------------|-------------------------------|
| REQ-UX-001    | "Add vault" form has a **Browse** button for local vaults (not a raw text input) | browser click → modal opens    |
| REQ-UX-002    | Browse modal lists **directories on the server**, click to descend              | browser: click into a subdir  |
| REQ-UX-003    | "Up one level" + current path breadcrumb in the modal                          | browser: navigate up          |
| REQ-UX-004    | Selecting a directory fills the vault path field and closes the modal          | form value populated          |
| REQ-UX-005    | GitHub vaults use a plain **URL field** — no file browser                      | browser: no Browse button     |
| REQ-UX-006    | Browse endpoint is **admin-only** and cannot escape a configured root          | curl unauth → 401; `..` → 400 |
| REQ-UX-007    | Invalid selections give a readable message (not a raw 500)                     | browser: missing book.toml    |

| REQ-UX-008    | A **Import** button opens a browser-side folder picker (`webkitdirectory`)     | browser: folder chosen             |
| REQ-UX-009    | Imported files are uploaded to the **server-side** vault dir (never the path)  | files land under the vault source  |
| REQ-UX-010    | Import **creates a new vault** from the chosen local folder (slug = folder name)    | vault exists after import          |
| REQ-UX-011    | Import refuses an existing slug/folder rather than overwriting a live vault     | same folder twice → refused        |
| REQ-UX-012    | Upload size limit + path sanitisation on every uploaded filename               | huge file → 413; `..` name → 400   |

## OPS — Operations / polish

| REQ-ID        | Description                                                       | Verification                  |
|---------------|-------------------------------------------------------------------|-------------------------------|
| REQ-OPS-001   | `morinet init` generates an example `config.toml`                 | `--help` + run                |
| REQ-OPS-002   | Minimal README (What / Install / Quick start / Status / License) | review                        |
| REQ-OPS-003   | `morinet --version` returns `1.0.0` at tag v1.0.0                 | `morinet --version`           |
| REQ-OPS-004   | Versioned SQLite migrations (`_migrations` table)                 | boot success + log            |
| REQ-OPS-005   | Healthcheck `GET /health` → 200                                  | curl                          |
| REQ-OPS-006   | Each published release auto-deploys to holldemo (morinet.treklair.fr) | Deploy workflow green + `morinet --version` over SSH |
| REQ-OPS-007   | holldemo runs morinet as a systemd unit `morinet.service`         | `systemctl status morinet` on host |
| REQ-OPS-008   | morinet.treklair.fr → reverse-proxied to the unit with TLS        | `curl https://morinet.treklair.fr/health` |

## OBS — Visual mods (custom mdbook)

| REQ-ID        | Description                                                            | Verification                          |
|---------------|------------------------------------------------------------------------|---------------------------------------|
| REQ-OBS-001   | Image zoom medium-zoom style (click = overlay zoom)                    | HTML snapshot + JS loaded            |
| REQ-OBS-002   | Collapsible sections (fold) on `<details>` or custom class             | HTML snapshot                         |
| REQ-OBS-003   | External links open in new tab + `rel=noopener`                        | HTML snapshot                         |
| REQ-OBS-004   | Custom logo + favicon served                                           | curl favicon                          |
| REQ-OBS-005   | Vault switcher in the sidebar                                          | HTML snapshot                         |

---

## Breakdown per phase

- Phase 0: REQ-INFRA-001, 002, 003, 005, 006, REQ-OPS-001
- Phase 1: REQ-AUTH-001..004, REQ-VAULT-001, REQ-INFRA-004
- Phase 2: REQ-VAULT-002..006
- Phase 3: REQ-VAULT-007..010
- Phase 4: REQ-ADMIN-001..006
- Phase 5: REQ-EMAIL-001..005, REQ-AUTH-005
- Phase 6: REQ-WEBDAV-001..005
- Phase 7: REQ-MCP-001..007
- Phase 7: REQ-UX-001..007 (vault management UX — inserted before MCP)
- Phase 8: REQ-MCP-001..007 (renumbered)
- Phase 9: REQ-OPS-002..005, REQ-OBS-001..005 (visual mods + finalization)
