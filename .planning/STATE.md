# MoriNet — STATE (live session state)

> Mis à jour à chaque bite livré / décision prise / erreur notable.

---

## Position actuelle (2026-10-20, post planification initiale)

- **Phase active** : 00 bootstrap, pas démarrée
- **Statut planification** : 🔵 complète — ROADMAP / REQUIREMENTS / Phase 0 PLAN rédigés
- **Prochaine action** : confirmer username GitHub (`lejeunecedric`), puis lancer Phase 0 T0.1

## Décisions Architecturales (AD)

> Tracées ici pour qu'on puisse les reverser plus tard. Chaque AD = une décision + rationale + date.

- **AD-1** *(2026-10-20)* — **Cargo workspace MONO-CRATE** (pas de multi-crates).
  - Rationale : MoriNet est un seul binaire, pas besoin de découpage en crates. On évite la complexité workspace multi-crates pour un gain YAGNI. Si on split un jour, on le fait quand le code atteint ~3000 LOC.
- **AD-2** *(2026-10-20)* — **mdbook UPSTREAM + thème custom "mori" injecté** (no fork).
  - Rationale : mdbook est un outil stable, on n'a pas besoin de le modifier. On injecte nos assets CSS/JS (zoom.css, fold.css, etc.) via `additional-css` / `additional-js` dans le `book.toml` généré pour chaque vault. Si mdbook upstream casse une compat, on patche `additional-css` plutôt que le binaire.
- **AD-3** *(2026-10-20)* — **SQLite par défaut**, Postgres = feature flag `db = "postgres"`.
  - Rationale : SQLite portable, zero-setup pour un MoriNet mono-instance. Postgres supporté plus tard si besoin de multi-instances / HA. sqlx abstrait la couche.
- **AD-4** *(2026-10-20)* — **Auth cookies HttpOnly signés** (clé HMAC dans config).
  - Rationale : pas de JWT (XSS + revocation complexity). Cookie HttpOnly signé côté serveur → revocation trivial via blacklist Redis (out of scope v1) ou expiration courte. bcrypt pour mdp.
- **AD-5** *(2026-10-20)* — **WebDAV = handlers axum custom** (pas de crate).
  - Rationale : la spec WebDAV qu'on supporte est minimale (PROPFIND, GET, PUT optionnel). Un crate ajoute ~10 deps et un état en plus. On fait 3 handlers axum en ~200 lignes, full contrôle.
- **AD-6** *(2026-10-20)* — **MCP = crate `rmcp`** (officiel Rust MCP).
  - Rationale : rmcp est officiel, StreamableHttp supporté, schémas JSON auto-générés. Pas la peine de réinventer. Transport : StreamableHttp (le plus simple pour HTTP, déprécie SSE lentement). Outils Phase 1 : list_vaults, list_notes, read_note, search_notes.
- **AD-7** *(2026-10-20)* — **SMTP = crate `lettre`**, config TOML.
  - Rationale : lettre supporte SMTP + STARTTLS + SMTPS, API async propre, multi-TLS backends. Config TOML hôte/port/user/pass/from/tls. Test via un mock in-memory.
- **AD-8** *(2026-10-20)* — **Vault source = trait `VaultSource`**, impl `Local` + `Github`.
  - Rationale : le routing admin stocke `vaults(id, slug, source_kind, source_config_json, book_path)`. Le sync est périodique (background task), pas à chaque requête. Github utilise `git2` (binding libgit2) plutôt que shelling out — plus déterministe et auditable. Periodic sync default = 5 minutes, configurable.
- **AD-9** *(2026-10-20)* — **Templates HTML côté serveur via `askama`** (pas de SPA, pas de JS framework).
  - Rationale : admin UI minimaliste, Askama = templates Jinja-like compilés en Rust, zero runtime overhead, type-safe. Pas de React/Vue/Svelte = pas de toolchain frontend à maintenir. JS only pour les customs mdbook (zoom, fold).
- **AD-10** *(2026-10-20)* — **mdbook compilé déclenché par sync**, pas à chaque requête.
  - Rationale : `mdbook build` prend ~1-3s pour un vault de 100 pages. Rebuild à chaque GET est inutile. Sync rebuild après changement, hot-reload désactivé en prod.

## Décisions produit (PRD)

- **PRD-1** *(2026-10-20)* — Stack 100% Rust.
  - User a tranché : `cargo install morinet` (binaire statique).
- **PRD-2** *(2026-10-20)* — Repo sur github.com/`lejeunecedric`/morinet (public).
  - Username à confirmer — placeholder dans tous les artefacts.
- **PRD-3** *(2026-10-20)* — Multi-vault = un mdbook par vault, sub-paths `/vaultA/`, `/vaultB/`.
  - User a tranché. Pas de switcher dans la sidebar (plus tard possible).
- **PRD-4** *(2026-10-20)* — SMTP = adapter générique configurable (pas de Brevo intégré).
  - User a tranché. On code un adapter lettre générique host/port/user/pass.

## Erreurs notables & apprentissages

*(vide pour l'instant)*

## Stats

- Fichiers planification : 5 (ROADMAP, REQUIREMENTS, STATE, index, Phase 0 PLAN)
- AD verrouillées : 10
- REQ catalog : 48 (5 INFRA + 5 AUTH + 10 VAULT + 6 ADMIN + 5 EMAIL + 5 WEBDAV + 7 MCP + 5 OPS + 5 OBS approx)
- Phases définies : 8

## Liens utiles

- ROADMAP : `projets/.planning/ROADMAP.md`
- REQUIREMENTS : `projets/.planning/REQUIREMENTS.md`
- Phase 0 PLAN : `projets/.planning/phases/00-bootstrap/01-bootstrap-PLAN.md`
- Index humain : `projets/.hermes/plans/00-index.md`
- Référence oscbrain : `https://oscbrain.hollitech.fr/` (pour la phase 8 — assets zoom/fold)
