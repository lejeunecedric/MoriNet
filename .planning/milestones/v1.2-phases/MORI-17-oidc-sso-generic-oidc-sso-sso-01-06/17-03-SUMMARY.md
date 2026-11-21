# 17-03 Summary — identity-resolution-linking-provisioning

**Status:** complete (recovered mid-execution by orchestrator after executor context-limit crash)
**Commits:**
- `bf9d5c2` feat(store): normalized SSO email lookup + unguessable-placeholder provisioning (D-12, D-04, SSO-03)
- `b897425` feat(oidc): identity-resolution policy — iss/sub sign-in, verified-email auto-link, domain-gated provisioning (SSO-02, SSO-03, D-03/D-04/D-05/D-12)
- `da873f3` feat(server): account page Linked-via-SSO indicator (D-13, SSO-03)

## What shipped
- **Store (task 1):** `LOWER(email)=?` normalized lookup helper (SSO path only — password login untouched, still exact-match), `create_user_without_password` with bcrypt over random placeholder, role hard-pinned `reader`.
- **Callback resolution (task 2):** returning `(iss,sub)` signs in directly; first login auto-links on `email_verified` + trim+to_ascii_lowercase match (admin match → 403 refusal via error_page status override, deny-logged); unknown email → exact-domain allowlist → provision reader + link + `provision`/`auto_provision` auth_events, else `domain_not_allowed` deny. All three success paths converge on the single session-mint tail (D-02; `DEFAULT_TTL_SECS` count in oidc.rs == 1).
- **Account page (task 3):** "Linked via SSO" row in Profile panel when `identity_for_user` returns a row; degrades gracefully on store error; password-only users see no row.

## TDD evidence
- Task 1: RED (compile fail on missing store helpers) → GREEN 6/6.
- Task 2: RED (resolution-policy tests failing against wave-2 placeholder 403) → GREEN (455 passed mid-run).
- Task 3: RED `account_page_sso_row_survives_token_create_render` FAILED → GREEN after server patch (2/2 targeted).

## Final gates
`cargo test --lib` **458 passed / 0 failed / 4 ignored**; clippy `-D warnings` clean; `fmt --check` clean; grep gates: `DEFAULT_TTL_SECS` oidc.rs == 1, `Linked via SSO` server.rs == 1, `LOWER(email)` store.rs == 4. `sso_reg_*` baseline unedited.

## Deviations / notes
- Executor crashed on provider context limit (315k > 262k) during task 3 GREEN; its applied patches were verified green by the orchestrator, committed as `da873f3`, and this SUMMARY written in its place.
- Task 3 tests live in `oidc.rs` tests mod (keeps the `Linked via SSO` grep gate at one match in server.rs).
