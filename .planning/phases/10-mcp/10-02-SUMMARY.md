# SUMMARY — 10-02 MCP StreamableHttp ✅ (2026-10-25)

**Livré** : `src/mcp.rs` (module rmcp 3.7) + montage `ANY /mcp` dans `server.rs`.

- **Auth** : middleware Bearer → `Store::user_for_api_token` ; absent/inconnu/révoqué → 401 + `WWW-Authenticate: Bearer`. Cookies et mot de passe refusés.
- **Transport** : `StreamableHttpService` stateless (`with_legacy_session_mode(false)` + `with_json_response(true)`), un `MoriMcp` frais par request lié à l'utilisateur du token — pas d'état croisable entre users.
- **Tools** (tous scoped ACL via `accessible_vaults`, lecture de la SOURCE markdown, AD-13) :
  `list_vaults`, `list_notes(slug, path?)`, `read_note(slug, path)`, `search_notes(query, slug?)` (insensible à la casse, cap 50 hits).
- **Sécurité** : `safe_join` refuse `..` et chemins absolus ; erreur d'outil générique « vault not found or access denied » (pas de fuite d'existence).

**TDD** : 5 tests unitaires mcp + 4 tests E2E HTTP (401 sans Bearer, forged/révoqué → 401, tools/list expose les 4 tools, tools/call bound au user du token), tous vus échouer d'abord. Suite : 213 verts, clippy `-D warnings` clean.

**Découvertes (rmcp 3.7)** :
- le serveur valide l'en-tête `Host` (anti DNS-rebinding) — les clients HTTP bruts doivent l'envoyer ;
- sans `MCP-Protocol-Version`, un request est traité comme legacy → session requise ; corrigé par `with_legacy_session_mode(false)` (stateless pour toutes les versions) ;
- `ServerInfo` est déprécié → `#[tool_handler(router = self.tool_router)]` sans override `get_info`.

**Hors périmètre** (optionnel, non fait) : resource template `vault://<slug>/…`.
