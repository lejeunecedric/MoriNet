# SUMMARY — 10-01 API tokens ✅ (2026-10-25)

**Delivered** : migration `0007_api_tokens.sql` + `Store::{create_api_token, list_api_tokens, revoke_api_token, user_for_api_token, user_id_for}` + "API tokens" section on `/account` (named creation, plaintext shown once, list with created/last used, revocation).

**TDD** : 4 store tests + 3 route tests, all observed failing first (RED : missing methods, then 404). Suite : 204 tests green, clippy `-D warnings` clean.

**Discovery along the way** : the auth convention for protected POSTs is a 303 redirect to `/login` (not 401) — the "requires a session" test was fixed to match the established convention (`loading_a_vault_requires_a_session`), not the other way around.

**Next** : 10-02 (rmcp StreamableHttp on `/mcp`, Bearer via `user_for_api_token`).
