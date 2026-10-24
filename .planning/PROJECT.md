# MoriNet

## What This Is

Un serveur Rust qui expose des second brains Obsidian / llm-wiki comme des sites mdBook, avec auth multi-utilisateurs, UI d'admin, gestion de vaults (dossiers locaux + repos GitHub), WebDAV et MCP. Un seul binaire statique : `cargo install morinet`.

**Live instance** : https://morinet.treklair.fr (démo publique — credentials sur le serveur, jamais dans le repo).

## Core Value

Charger un vault Obsidian/llm-wiki et le servir instantanément en site mdBook navigable, multi-utilisateurs et sécurisé — sans que l'utilisateur ait à toucher à mdBook lui-même.

## Requirements

### Validated

<!-- Shipped and confirmed valuable (v0.1.0 → v0.1.20). -->

- ✓ Binaire unique `cargo install morinet`, CI + release cargo-dist + auto-deploy — REQ-INFRA-* (v0.1.0..v0.1.16)
- ✓ Login cookie HttpOnly + HMAC, users SQLite bcrypt, rôles admin/reader — REQ-AUTH-* (v0.1.1)
- ✓ Multi-vault `/vault/<slug>/` avec ACL par vault — REQ-VAULT-001..006 (v0.1.1)
- ✓ Sources de vault : dossier local OU repo GitHub (clone + sync git2 + mdbook build) — REQ-VAULT-007..010 (v0.1.2+)
- ✓ Admin UI : invitations, reset passwords, vaults + permissions — REQ-ADMIN-* (v0.1.4+)
- ✓ Email : invitations 24 h single-use + reset 1 h (lettre SMTP, fallback temp-password) — REQ-EMAIL-* (v0.1.6+)
- ✓ WebDAV PROPFIND/GET/PUT sur la **source** du vault (markdown in, `mdbook build` out), HTTP Basic + ACL — REQ-WEBDAV-* (v0.1.7+)
- ✓ Format Obsidian/llm-wiki natif : convertisseur in-pipeline (AD-14) → arbre mdBook généré dans `.morinet-build/` — REQ-OBSFMT-001..006 (v0.1.8)
- ✓ Groupes + vaults personnels (`scope`/`owner_id`, `vault_group_acl`), quotas par groupe (`free`=2, `admin`=∞) — REQ-GRP-*, REQ-VLT-005..008 (v0.1.9..v0.1.12)
- ✓ UX vaults : picker de dossiers serveur, import navigateur, URL GitHub auto-nom, Refresh/Delete owner — REQ-UX-* (v0.1.10..v0.1.15)
- ✓ Nav shell + design system (1 seul contrôle header, pill nav, zoom + Close vault sur les pages mdBook) — REQ-ACC-*, REQ-NAV-* (v0.1.14..v0.1.20)

### Active

<!-- Milestone v1.0 — current scope. -->

- [ ] Phase 10 — MCP StreamableHttp sur `/mcp` : Bearer auth, tools `list_vaults`, `list_notes`, `read_note`, `search_notes` sur les vaults accessibles — REQ-MCP-*
- [ ] Phase 11 — Polish v1.0 : README minimal, `morinet init` (config.toml exemple), Cargo.lock committé, tag `v1.0.0` — REQ-OPS-*

### Out of Scope

- Édition collaborative temps réel — hors v1, complexité WebSocket/CRDT non justifiée
- Apps mobiles natives — le web responsive suffit
- SSO (Google/Microsoft OAuth) — bcrypt + email couvre le besoin self-hosted
- Webhooks / API publique de gestion des vaults — WebDAV + admin UI couvrent l'usage
- Chiffrement au repos — on s'appuie sur le FS du serveur

## Context

- Stack : Rust stable, axum, sqlx (SQLite), mdbook (CLI amont, appelé en sous-processus), rmcp (à venir), lettre. Déploiement systemd + nginx sur holldemo.
- Pipeline vault : sync → miroir dans `<source>/.morinet-build/` (frontmatter consommé, wikilinks résolus, SUMMARY.md + book.toml générés) → `mdbook build` → servi. Les thèmes mdBook sont figés au build : après modif de `assets/zoom.*` ou `MORINET_CSS`, il faut cliquer **Refresh** (pitfall #24).
- 197 tests, clippy `-D warnings`, fmt clean à chaque push. CI Linux-only depuis v0.1.16 (runners Mac flaky).
- Historique détaillé version par version : `.planning/STATE-LOG.md` et `.planning/HANDOFF.md`.
- Catalogue exigences vérifiables : `.planning/REQUIREMENTS.md`.

## Constraints

- **Tech** : Rust seul, binaire statique — distribution `cargo install`, zéro runtime
- **Dépendance externe** : `mdbook` CLI doit être dans le PATH (échec LOUD si absent, pas de fallback silencieux)
- **Compat** : les vaults mdBook-natifs (`book.toml`) continuent de marcher tels quels à côté du format Obsidian
- **Sécurité** : credentials de prod hors du repo (env systemd) ; `release.yml` cargo-dist ne jamais l'éditer à la main
- **Déploiement** : staging + `mv` atomique (ETXTBSY) ; le deploy attend la release publiée avec ses assets

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Format vault = Obsidian/llm-wiki converti in-pipeline (AD-14) plutôt que contrat mdBook-natif | Les users ont des vaults Obsidian, pas des mdBooks ; conversion > scaffold | ✓ Bon (v0.1.8, E2E vérifié sur ProgWiki 131 .md / 812 wikilinks) |
| Abandon du « merged book » → groupes + vaults personnels (Phase 8) | Decision user 2026-10-21 : cloisonnement propre > fusion | ✓ Bon |
| Quotas par groupe (`free`=2, `admin`=∞), demo dégradé en reader | Empêcher l'épuisement de slots + bouton Admin fantôme | ✓ Bon |
| CI Linux-only (dist 2 targets linux) | Runners Mac flaky = releases cassées | ✓ Bon (v0.1.16, plus de re-dispatch) |
| `ensure_fallback_admin()` au boot | Le bootstrap ADMIN_EMAIL seul verrouillait /admin si l'unique admin était dégradé (incident treklair) | ✓ Bon |
| GSD adopté comme mode de planification (2026-10-25) | Workflows plan/execute/verify outillés ; docs existants conservés (ROADMAP/REQUIREMENTS/HANDOFF/STATE-LOG) | — Pending |

---
*Last updated: 2026-10-25 — bootstrap GSD depuis les docs de planning existants (v0.1.20, phases 00→09b livrées).*
