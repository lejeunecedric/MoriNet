# MoriNet — Planification (entry point humain)

> Planification GSD pour MoriNet — un serveur Rust qui sert un second brain Obsidian en mdbook avec auth multi-utilisateurs, admin UI, vault management (local + GitHub), WebDAV, et MCP.

## TL;DR

- **Stack** : tout Rust (`cargo install morinet` produit un binaire statique)
- **8 phases** : bootstrap → core-serve → multi-vault → vault-sources → admin-ui → email → webdav → mcp → polish
- **Repo** : github.com/`lejeunecedric`/morinet *(à confirmer)*

## Quoi lire dans quel ordre

1. **`../.planning/ROADMAP.md`** — vue d'ensemble des 8 phases, critères de succès, scope
2. **`../.planning/REQUIREMENTS.md`** — 48 REQ-IDs catalog avec commandes de vérification
3. **`../.planning/STATE.md`** — décisions architecturales (AD-1 à AD-10) + stats live
4. **`../.planning/phases/00-bootstrap/01-bootstrap-PLAN.md`** — premier bite à exécuter : bootstrap le repo + le crate + la CI

## Décisions verrouillées

| # | Décision | Tracée dans |
|---|----------|--------------|
| Stack 100% Rust | PRD-1 | STATE.md |
| Repo github.com/`lejeunecedric`/morinet | PRD-2 | STATE.md |
| Multi-vault = sub-paths `/vaultA/` | PRD-3 | STATE.md |
| SMTP = adapter lettre générique | PRD-4 | STATE.md |
| Cargo workspace mono-crate | AD-1 | STATE.md |
| mdbook upstream + thème custom injecté | AD-2 | STATE.md |
| SQLite par défaut | AD-3 | STATE.md |
| Cookies HttpOnly signés + bcrypt | AD-4 | STATE.md |
| WebDAV handlers axum custom | AD-5 | STATE.md |
| MCP via crate `rmcp` | AD-6 | STATE.md |
| SMTP via crate `lettre` | AD-7 | STATE.md |
| Vault source = trait, impl Local + Github | AD-8 | STATE.md |
| Templates HTML via Askama | AD-9 | STATE.md |
| mdbook rebuild après sync, pas par requête | AD-10 | STATE.md |

## Phase 0 — bootstrap (à exécuter)

**Goal** : repo GitHub + Cargo init + CI + premier release.

**Bites** :
- T0.1 → confirmer username GitHub (placeholder `lejeunecedric` à remplacer)
- T0.2 → `cargo install morinet` build un binaire trivial + `cargo test` vert
- T0.3 → CI GitHub Actions (lint + test + build)
- T0.4 → cargo-dist pour release multi-platform
- T0.5 → README minimal (What / Install / Quick start / Status / License)
- T0.6 → premier commit + push + tag `v0.1.0`

Voir `projets/.planning/phases/00-bootstrap/01-bootstrap-PLAN.md` pour le détail bite-sized.

## Prochaine étape

Une fois le username GitHub confirmé, je lance la Phase 0 avec subagent-driven-development (fresh delegate_task par bite + double review spec-compliance + code-quality).

## Hors-scope v1

Édition collaborative temps réel, mobile apps, SSO OAuth, webhooks, chiffrement at-rest (s'appuie sur le FS).
