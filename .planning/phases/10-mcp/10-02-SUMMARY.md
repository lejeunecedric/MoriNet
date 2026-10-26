# SUMMARY — 10-02 MCP StreamableHttp ✅ (2026-10-25)

**Delivered** : `src/mcp.rs` (rmcp 3.7 module) + `ANY /mcp` mount in `server.rs`.

- **Auth** : Bearer middleware → `Store::user_for_api_token` ; missing/unknown/revoked → 401 + `WWW-Authenticate: Bearer`. Cookies and passwords rejected.
- **Transport** : stateless `StreamableHttpService` (`with_legacy_session_mode(false)` + `with_json_response(true)`), a fresh `MoriMcp` per request bound to the token's user — no state crossable between users.
- **Tools** (all ACL-scoped via `accessible_vaults`, reading the markdown SOURCE, AD-13) :
  `list_vaults`, `list_notes(slug, path?)`, `read_note(slug, path)`, `search_notes(query, slug?)` (case-insensitive, capped at 50 hits).
- **Security** : `safe_join` rejects `..` and absolute paths ; generic tool error "vault not found or access denied" (no existence leak).

**TDD** : 5 mcp unit tests + 4 HTTP E2E tests (401 without Bearer, forged/revoked → 401, tools/list exposes the 4 tools, tools/call bound to the token's user), all observed failing first. Suite : 213 green, clippy `-D warnings` clean.

**Discoveries (rmcp 3.7)** :
- the server validates the `Host` header (anti DNS-rebinding) — raw HTTP clients must send it ;
- without `MCP-Protocol-Version`, a request is treated as legacy → session required ; fixed by `with_legacy_session_mode(false)` (stateless for all versions) ;
- `ServerInfo` is deprecated → `#[tool_handler(router = self.tool_router)]` without a `get_info` override.

**Out of scope** (optional, not done) : resource template `vault://<slug>/…`.
