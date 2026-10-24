# SUMMARY — 10-01 API tokens ✅ (2026-10-25)

**Livré** : migration `0007_api_tokens.sql` + `Store::{create_api_token, list_api_tokens, revoke_api_token, user_for_api_token, user_id_for}` + section « API tokens » sur `/account` (création nommée, plaintext affiché une seule fois, liste avec créé/dernier usage, révocation).

**TDD** : 4 tests store + 3 tests routes, tous vus échouer d'abord (RED : méthodes manquantes, puis 404). Suite : 204 tests verts, clippy `-D warnings` clean.

**Découverte en route** : la convention d'auth des POST protégés est un redirect 303 vers `/login` (pas 401) — le test « requires a session » a été corrigé pour matcher la convention établie (`loading_a_vault_requires_a_session`), pas l'inverse.

**Next** : 10-02 (rmcp StreamableHttp sur `/mcp`, Bearer via `user_for_api_token`).
