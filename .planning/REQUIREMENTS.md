# MoriNet — Requirements Catalog

> Chaque REQ-ID est vérifiable par une commande / un test / un curl documenté.
> Format : `REQ-<DOMAIN>-<NNN>` — `<DOMAIN>` ∈ {INFRA, AUTH, VAULT, ADMIN, EMAIL, WEBDAV, MCP, OPS, OBS}.

---

## INFRA — Foundations

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-INFRA-001 | Le binaire s'installe via `cargo install morinet` | `cargo install --path .` puis `morinet --version` |
| REQ-INFRA-002 | CI GitHub Actions verte (lint + test + build) | push → GH Actions run success |
| REQ-INFRA-003 | Release GitHub avec binaire cross-platform | `gh release list` montre vX.Y.Z |
| REQ-INFRA-004 | DB SQLite par défaut | `data/morinet.db` créé au boot, schéma migré |
| REQ-INFRA-005 | Config via `config.toml` | `--config path/to/config.toml` |
| REQ-INFRA-006 | Logs structurés via `tracing` | `RUST_LOG=morinet=debug morinet serve` |

## AUTH — Users & sessions

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-AUTH-001 | Un user a : email (unique), password_hash (bcrypt), role ∈ {admin, reader}, created_at | migrations + tests CRUD |
| REQ-AUTH-002 | Login POST `/login` accepte email + mdp, set cookie HttpOnly signé | curl login + cookie reçu |
| REQ-AUTH-003 | Middleware axum vérifie le cookie sur routes protégées | curl non-auth → 401 |
| REQ-AUTH-004 | Logout POST `/logout` clear le cookie | curl logout + cookie absent |
| REQ-AUTH-005 | Reset mdp : token signé à TTL, `/reset?token=...` valide, set nouveau mdp | flow intégré testé |

## VAULT — Multi-vault & sources

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-VAULT-001 | Un vault = un dossier mdbook compilé (`book/index.html` existe) | Phase 1 : sert ce dossier via axum |
| REQ-VAULT-002 | `vaults` table persiste, CRUD complet | sqlite + tests |
| REQ-VAULT-003 | Sub-paths `/vault/<slug>/` routent vers le bon `book/` | curl 200 vs 404 |
| REQ-VAULT-004 | ACL `vault_acl(vault_id, user_id)` | user non-acl → 403 |
| REQ-VAULT-005 | Page `/` logged-in liste les vaults accessibles | snapshot du HTML |
| REQ-VAULT-006 | Switcher de vault dans la sidebar mdbook | injection de menu custom |
| REQ-VAULT-007 | Trait `VaultSource` (sync → PathBuf local) | unit tests mock impl |
| REQ-VAULT-008 | `LocalVaultSource` (chemin fourni tel quel) | test e2e avec path réel |
| REQ-VAULT-009 | `GithubVaultSource` (clone + pull périodique) | fixture repo GitHub + test intégration |
| REQ-VAULT-010 | Rebuild mdbook après sync | vérifier que `book/` est régénéré |

## ADMIN — UI admin

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-ADMIN-001 | Admin gate : toutes routes `/admin/*` requirent `role = admin` | curl `/admin/users` non-admin → 403 |
| REQ-ADMIN-002 | Page `GET /admin/users` (table + bouton invite) | snapshot HTML |
| REQ-ADMIN-003 | `POST /admin/users/invite { email }` crée user + envoie email | mock SMTP + DB check |
| REQ-ADMIN-004 | Page `GET /admin/vaults` (table + bouton add vault) | snapshot HTML |
| REQ-ADMIN-005 | Page `GET /admin/vaults/<slug>/perms` (ACL editor) | snapshot HTML |
| REQ-ADMIN-006 | `POST /admin/vaults/{create,delete,update-acl}` + tests | integration |

## EMAIL — SMTP

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-EMAIL-001 | `[smtp]` section dans `config.toml` | config loading tested |
| REQ-EMAIL-002 | `lettre` envoie via SMTP configurable | test avec mock SMTP local |
| REQ-EMAIL-003 | Template invitation (HTML) | snapshot généré |
| REQ-EMAIL-004 | Template reset password (HTML) | snapshot généré |
| REQ-EMAIL-005 | Lien d'invitation à TTL 24h, single-use | tests token expiry |

## WEBDAV — Vault access

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-WEBDAV-001 | Auth Basic sur `/webdav/<vault-slug>/` (mdbook email + mdp) | curl avec `-u` |
| REQ-WEBDAV-002 | `PROPFIND depth 0/1` retourne les `.md` du vault | cadaver / curl XML |
| REQ-WEBDAV-003 | `GET` retourne le `.md` brut en `text/markdown` | curl + Content-Type |
| REQ-WEBDAV-004 | `PUT` autorisé admin uniquement | curl PUT reader → 403 |
| REQ-WEBDAV-005 | ACL vault respectée sur WebDAV (user sans acl → 401) | curl non-acl → 401 |

## MCP — Model Context Protocol

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-MCP-001 | Endpoint `/mcp` (StreamableHttp) | curl initialize → 200 |
| REQ-MCP-002 | Auth Bearer (token dérivé du mdp) | curl sans token → 401 |
| REQ-MCP-003 | Outil `list_vaults()` → liste des vaults autorisés | invoke + assert |
| REQ-MCP-004 | Outil `list_notes(slug, path?)` → liste des `.md` | invoke + assert |
| REQ-MCP-005 | Outil `read_note(slug, path)` → contenu du `.md` | invoke + assert |
| REQ-MCP-006 | Outil `search_notes(query, slug?)` → résultats | invoke + assert |
| REQ-MCP-007 | ACL vault respectée côté MCP (user sans acl → empty list) | invoke + assert |

## OPS — Operations / polish

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-OPS-001 | `morinet init` génère un `config.toml` exemple | `--help` + run |
| REQ-OPS-002 | README minimal (What/Install/Quick start/Status/License) | review |
| REQ-OPS-003 | `morinet --version` répond `1.0.0` au tag v1.0.0 | `morinet --version` |
| REQ-OPS-004 | Migration SQLite versionnée (table `_migrations`) | boot success + log |
| REQ-OPS-005 | Healthcheck `GET /health` → 200 | curl |

## OBS — Visual mods (mdbook custom)

| REQ-ID | Description | Verification |
|--------|-------------|--------------|
| REQ-OBS-001 | Zoom images style medium-zoom (clic = overlay zoom) | snapshot HTML + JS chargé |
| REQ-OBS-002 | Sections repliables (fold) sur `<details>` ou custom class | snapshot HTML |
| REQ-OBS-003 | Liens externes ouvrent en new tab + `rel=noopener` | snapshot HTML |
| REQ-OBS-004 | Logo custom + favicon servi | curl favicon |
| REQ-OBS-005 | Switcher de vault dans la sidebar | snapshot HTML |

---

## Cumul par phase

- Phase 0 : REQ-INFRA-001, 002, 003, 005, 006, REQ-OPS-001
- Phase 1 : REQ-AUTH-001..005 (sans 005), REQ-VAULT-001, REQ-INFRA-004
- Phase 2 : REQ-VAULT-002..006
- Phase 3 : REQ-VAULT-007..010
- Phase 4 : REQ-ADMIN-001..006
- Phase 5 : REQ-EMAIL-001..005, REQ-AUTH-005
- Phase 6 : REQ-WEBDAV-001..005
- Phase 7 : REQ-MCP-001..007
- Phase 8 : REQ-OPS-002..005, REQ-OBS-001..005 (mods visuelles + finalisation)
