# CONTEXT — Phase 10: mcp

## Décisions verrouillées (discuss 2026-10-25)

- **Auth : token API dédié** (decision user). L'utilisateur génère des tokens sur `/account`
  (nom + valeur en clair affichée UNE fois), révocables. Stockés **hachés sha256** (même
  convention que `invitations`/`reset_tokens`, 0003_tokens.sql). Le MCP n'utilise JAMAIS
  le mot de passe ni le cookie de session.
- **Transport : StreamableHttp** sur `/mcp` via `rmcp` 3.7 (`transport-streamable-http-server`),
  mode stateless + `with_json_response(true)` — un `StreamableHttpService` est un service
  Tower, monté avec `nest_service("/mcp", …)` sur le routeur axum existant.
- **Périmètre tools** (ROADMAP) : `list_vaults`, `list_notes(slug, path?)`,
  `read_note(slug, path)`, `search_notes(query, slug?)`. Resource template `vault://<slug>/…`
  optionnel — hors des critères, à faire si le temps le permet.
- **ACL identique au WebDAV** : les tools ne voient que `list_vaults_for(email)` /
  `acl_allows(email, slug)` ; un slug deviné → erreur d'outil, pas de fuite d'existence.
- **Lecture sur la SOURCE du vault** (markdown, pas le `book/` compilé) — même choix que
  WebDAV (AD-13) ; les chemins sont validés par un safe_join (percent-decode AVANT le
  split — pitfall Phase 6).

## Contraintes

- `cargo clippy -D warnings` + fmt + suite verte (197 tests) à chaque commit.
- TDD strict : test échouant d'abord, par tranches verticales.
- Ne pas toucher au CSS/design (figé v0.1.20).
