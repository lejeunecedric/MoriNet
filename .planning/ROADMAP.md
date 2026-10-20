# MoriNet — Roadmap

> Second brain Obsidian exposé en serveur mdbook-like, avec auth multi-utilisateurs, admin UI, vault management (local + GitHub), WebDAV, et MCP. Tout en Rust, `cargo install morinet`.

**Repo** : `github.com/lejeunecedric/morinet` *(username à confirmer)*
**Stack** : Rust stable, axum, sqlx (SQLite), mdbook (CLI upstream), rmcp, lettre
**Format de livraison** : binaire statique via `cargo install morinet`

---

## Vue d'ensemble (8 phases)

| # | Phase | Goal | REQ-ID principaux | Statut |
|---|-------|------|--------------------|--------|
| 00 | **bootstrap** | Repo GitHub + Cargo init + CI + premier release | REQ-INFRA-* | 🔵 à démarrer |
| 01 | **core-serve** | Servir UN vault statique via mdbook compilé, derrière login simple | REQ-AUTH-*, REQ-VAULT-001 | ⬜ |
| 02 | **multi-vault** | Sub-paths `/vaultA/`, `/vaultB/`, switcher sur `/`, perms par vault | REQ-VAULT-002..006 | ⬜ |
| 03 | **vault-sources** | Source plugin : local folder OU repo GitHub (clone + sync) | REQ-VAULT-007..010 | ⬜ |
| 04 | **admin-ui** | Page admin : liste/invite users, reset mdp, gérer vaults + perms | REQ-ADMIN-* | ⬜ |
| 05 | **email** | SMTP adapter + invitations par email + reset mdp par email | REQ-EMAIL-* | ⬜ |
| 06 | **webdav** | Endpoints WebDAV sur les vaults (mêmes creds), navigation + read | REQ-WEBDAV-* | ⬜ |
| 07 | **mcp** | MCP StreamableHttp sur `/mcp`, outils list/read/search | REQ-MCP-* | ⬜ |
| +08 | **polish** | Installer one-liner propre, README complet, badges, Release v1.0 | REQ-OPS-* | ⬜ |

---

## Phase 0 — bootstrap

**Goal** : poser les fondations — le repo existe sur GitHub, le crate compile, la CI passe, le binaire s'installe via `cargo install`.

**Critères de succès** :
- `gh repo view lejeunecedric/morinet` → repo public existant
- `cargo install --path .` depuis un checkout cloné aboutit (binaire `morinet` disponible)
- `morinet --version` retourne `0.1.0`
- CI GitHub Actions verte sur push (lint + test + build)
- Premier release GitHub créé via cargo-dist

**Livré** : `projets/.planning/phases/00-bootstrap/01-bootstrap-PLAN.md`

---

## Phase 1 — core-serve

**Goal** : un admin unique hardcodé (via env var au boot) peut se logger et voir UN vault compilé en `data/vaults/demo/book/`.

**Critères** :
- `morinet serve --vault data/vaults/demo/book` démarre un serveur sur `:8080`
- `GET /` redirige vers `/login`
- `POST /login` avec `admin@example.com / [REDACTED-DEV-PASSWORD]` (env `MORINET_ADMIN_*`) set un cookie HttpOnly signé
- `GET /vault/*` sert les fichiers mdbook compilés, gated par le cookie

## Phase 2 — multi-vault

**Goal** : l'admin peut enregistrer N vaults (`name`, `slug`, `path` local) et chaque user voit `/vault/<slug>/` des vaults autorisés.

**Critères** :
- Table `vaults` (id, slug, name, path, created_at)
- Table `vault_acl` (vault_id, user_id)
- `GET /` (logged in) liste les vaults accessibles avec lien
- `/vault/<slug>/*` retourne 403 si pas dans l'acl, sinon sert les fichiers

## Phase 3 — vault-sources

**Goal** : ajouter un source plugin `GithubVaultSource` (clone git private/public + sync périodique).

**Critères** :
- Trait `VaultSource` avec `sync()` (idempotent) → retourne un path local
- `LocalVaultSource` (résout le path tel quel)
- `GithubVaultSource` (clone via `git2` ou shell `git`, periodic sync par intervalle configurable)
- Rebuild mdbook déclenché après sync réussie
- Tests : utiliser un repo test fixture

## Phase 4 — admin-ui

**Goal** : pages HTML servies par axum (pas de SPA) pour gérer users + vaults + perms.

**Critères** :
- `GET /admin/users` : table avec bouton "Invite"
- `POST /admin/users/invite { email }` : crée user, génère magic link
- `GET /admin/vaults` : table avec bouton "Add vault"
- `GET /admin/vaults/<slug>/perms` : ACL editor (checkboxes users)
- Templates via Askama ou Maud, CSS minimal maison

## Phase 5 — email

**Goal** : SMTP générique configurable, envoi d'invitations et reset mdp.

**Critères** :
- `config.toml` section `[smtp]` : host, port, user, pass, from, tls
- `EmailSender::send(to, subject, body_html)` via `lettre`
- Templates : `invitation.html.tera`, `reset_password.html.tera`
- Tests : `MockSmtpServer` trait, impl `InMemoryMock`

## Phase 6 — webdav

**Goal** : exposition WebDAV sur `/webdav/<vault-slug>/*` avec PROPFIND/GET/PUT, auth Basic (mdbook email + mdp).

**Critères** :
- `PROPFIND` : renvoie la liste des `.md` du vault (depth 1 = vault, depth 0 = file)
- `GET` : renvoie le `.md` brut (text/markdown)
- `PUT` : autorisé pour admin uniquement (écrase le `.md` et rebuild mdbook pour ce vault)
- `MKCOL`/`DELETE` : out of scope v1 (read-only-ish)
- Tests : curl WebDAV + rangage `cadaver`

## Phase 7 — mcp

**Goal** : MCP StreamableHttp sur `/mcp`, auth Bearer (token dérivé du mdp), outils sur les vaults autorisés.

**Critères** :
- Endpoint `/mcp` (StreamableHttp)
- Outils exposés : `list_vaults`, `list_notes(slug, path?)`, `read_note(slug, path)`, `search_notes(query, slug?)`
- Resource template `vault://<slug>/...` (optionnel)
- Schéma JSON retourné par les tools
- Tests via le client mcp-cli si dispo

## Phase +8 — polish

**Goal** : prêt à publier.

**Critères** :
- `cargo install morinet` fonctionne end-to-end (relire le README)
- README minimal/élégant (5 sections : What / Install / Quick start / Status / License)
- `morinet init` crée un `config.toml` exemple
- Cargo.lock committed
- Tag `v1.0.0` posé, release GitHub créée via cargo-dist
- Annonce sur le channel de l'user

---

## Hors-scope (v1)

- Édition collaborative temps réel
- Mobile apps natives
- SSO (Google/Microsoft OAuth)
- Webhooks / API publique pour gérer les vaults programmatiquement
- Chiffrement at-rest (on s'appuie sur le FS du serveur)

> Ces items peuvent migrer en phase si une demande forte émerge.
