# MoriNet

> Serveur [mdBook](https://rust-lang.github.io/mdBook/) qui expose un second brain Obsidian avec auth multi-utilisateurs, WebDAV et MCP.

[![CI](https://github.com/lejeunecedric/MoriNet/actions/workflows/ci.yml/badge.svg)](https://github.com/lejeunecedric/MoriNet/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

> 🚧 **Phase 0 — bootstrap**. Voir [`.planning/ROADMAP.md`](.planning/ROADMAP.md) pour la roadmap complète (8 phases).

## What

- 📚 Sert un ou plusieurs vaults Obsidian (compilés en mdbook) sur HTTP
- 🔐 Auth par email + mot de passe (cookies HttpOnly signés)
- 👤 Rôles `admin` / `reader`
- 📂 Sources = dossier local OU repo GitHub (clone + sync périodique)
- 📡 Vaults accessibles en **WebDAV** (mêmes creds mdbook)
- 🤖 Vaults accessibles en **MCP** (Model Context Protocol) — intégration agents IA
- 📨 Invitations et reset mdp par email
- 🎨 Mods visuelles mdbook custom : zoom images, sections repliables, liens externes (réplique oscbrain)
- 🚀 Distribution : un seul binaire statique Rust, `cargo install morinet`

## Install

> Phase 0 : depuis ce repo. Phase 8+ : depuis `crates.io`.

```bash
cargo install --git https://github.com/lejeunecedric/MoriNet morinet
```

## Quick start

```bash
# 1. Génère un config.toml d'exemple
morinet init --out ./config.toml

# 2. Édite-le (cookie_secret obligatoire : `openssl rand -hex 32`)
$EDITOR ./config.toml

# 3. Lance le serveur (Phase 1+)
morinet serve --config ./config.toml
# → http://localhost:8080
```

## Status

| Phase | Goal | Status |
|-------|------|--------|
| 00 bootstrap | repo + Cargo scaffold + CI | ✅ this phase |
| 01 core-serve | un vault + login simple | ⬜ |
| 02 multi-vault | sub-paths /vaultA/, /vaultB/ | ⬜ |
| 03 vault-sources | sources local + GitHub | ⬜ |
| 04 admin-ui | invite, perms, vault mgmt | ⬜ |
| 05 email | SMTP + invitations + reset | ⬜ |
| 06 webdav | exposition WebDAV | ⬜ |
| 07 mcp | MCP StreamableHttp | ⬜ |
| 08 polish | installer + README + release v1.0 | ⬜ |

Voir [`.planning/REQUIREMENTS.md`](.planning/REQUIREMENTS.md) pour les 48 REQ-IDs catalog.

## Architecture & decisions

Toutes les décisions architecturales sont tracées dans [`.planning/STATE.md`](.planning/STATE.md) (AD-1 à AD-10) avec rationale. Points clés :

- **Stack 100% Rust** (mono-crate `morinet`)
- **mdbook upstream** + thème "mori" injecté via `additional-css` / `additional-js` (pas de fork)
- **SQLite par défaut** (sqlx), fichier `data/morinet.db`
- **Cookies HttpOnly signés** (HMAC) + bcrypt pour mdp
- **WebDAV** = 3 handlers axum custom (~200 lignes, full contrôle)
- **MCP** via crate `rmcp`, StreamableHttp

## License

MIT OR Apache-2.0 — voir [LICENSE](LICENSE).

## Crédits

Inspiré visuellement par [oscbrain.hollitech.fr](https://oscbrain.hollitech.fr/) (mods CSS/JS zoom + fold).
