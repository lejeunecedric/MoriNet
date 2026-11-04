# Phase 17: oidc-sso — Generic OIDC SSO - Context

**Gathered:** 2026-11-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Teams sign in through their own OIDC-compliant IdP (Keycloak / Authentik / Authelia) via authorization-code + PKCE, **coexisting** with password login. New `src/oidc.rs` + `[oidc]` config section (env-wins, disabled by default) + migration 0012 (`user_identities` table). The callback mints the **same** HMAC session cookie as password login so every downstream middleware (web, WebDAV gates, admin, API tokens) is untouched. No multi-IdP, no RP-initiated logout, no self-service unlink in this phase.

</domain>

<decisions>
## Implementation Decisions

### Locked by ROADMAP (constraints, not gray areas)
- **D-01:** Flow is authorization-code + PKCE (S256) with `state` + `nonce`; discovery + JWKS-validated ID token. Only new dependency: `openidconnect` 4.0.1 (reqwest + rustls-tls). No hand-rolled OIDC (research X1).
- **D-02:** The SSO callback mints the **same** HMAC session cookie as `login_submit` (`morinet_session`, `src/session.rs`) — no parallel session mechanism, no gate bypasses (avoids A6, A7).
- **D-03:** `(iss, sub)` is the authoritative identity after first link; email is never the join key for returning users (avoids A1).
- **D-04:** Auto-provision: first-time SSO user whose verified email matches `allowed_email_domains` gets a `reader` account; unmatched email fails closed (avoids A7).
- **D-05:** Admin-role local accounts are **never auto-linked**; every link event is logged (avoids A1).
- **D-06:** `[oidc]` config section (issuer, client id/secret, allowed domains, claim mapping) following the env-wins pattern in `src/config.rs`; SSO disabled by default and fully removable without affecting password login (SSO-05).
- **D-07:** Bootstrap admin login (password) always remains available — IdP-outage safety net (SSO-06).
- **D-08:** Intra-phase order: password-login regression tests FIRST, then linking/provisioning, then group sync LAST. Migration applies cleanly on existing DBs, no table rebuild.

### Group sync semantics (discussed 2026-11-01 — resolves the ARCHITECTURE-vs-PITFALLS A5 conflict)
- **D-09:** **Replace-on-login**: at each SSO login the user's OIDC-managed group memberships are recomputed from the groups claim. Claim **absent** → keep existing managed memberships untouched; claim **present but empty** → remove all managed memberships. Demotions are logged. (PITFALLS A5 wins over ARCHITECTURE's additive-only.)
- **D-10:** The "managed set" = MoriNet groups that appear as targets in the configured claim mapping. Groups never named in the mapping (e.g. manually granted) are never added or removed by sync. Admin-group mapping is opt-in and fail-closed when the claim is absent.

### Identity storage (discussed 2026-11-01)
- **D-11:** New table `user_identities(id, provider, iss, sub, user_id → users, linked_at, UNIQUE(provider, iss, sub))` via migration **0012** — NOT `users.provider` columns (the ROADMAP's "migration 0011" sketch is superseded: 0011 was consumed by Phase 15 `vault_changes.forced/actor`). Extensible to future providers; `users` stays clean.

### Account linking (discussed 2026-11-01)
- **D-12:** Auto-link when `email_verified` is true and the IdP email matches an existing local account (normalized case, same rule as password login): create the `user_identities` row, sign in, log the link. Matching an **admin** account → refuse with a clear error (no auto-link, no sign-in).
- **D-13:** No self-service unlink in v1: the account page shows a "linked via SSO" indicator; unlinking is an admin/DB operation.

### Login UX (discussed 2026-11-01)
- **D-14:** When `[oidc]` is configured, the login page shows a single "Se connecter via {sso_label}" button **above** the password form, separated by an "or" divider; `sso_label` is configurable (default "SSO"). No SSO UI at all when the section is absent.

### Groups claim shape (locked requirement, config-driven)
- **D-15:** Claim name configurable (`groups_claim`, default `groups`) + mapping table `claim_value = "morinet group"` in config; read ONLY from the signature-verified ID token (never userinfo, never access token).

### Claude's Discretion
- Exact error pages/messages for refused links (admin-match, domain-not-allowed), state/nonce storage (signed short-lived cookie vs store table), rate limiting of the callback, and whether link/log rows go in `vault_changes`-style audit or a dedicated `auth_events` table — planner decides, fail-closed everywhere.

</decisions>

<canonical_refs>
## Canonical References

**Phase requirements**
- `.planning/REQUIREMENTS.md` — SSO-01..SSO-06 (all Pending → Phase 17)
- `.planning/ROADMAP.md` §"Phase 17: oidc-sso" — goal + 7 success criteria + Avoids A1/A2/A5/A6/A7/X1
- `.planning/research/SUMMARY.md` §"Gaps to Address" — group-sync conflict (now resolved by D-09), `(iss,sub)` storage (now resolved by D-11)

**Code to reuse / read before planning**
- `src/config.rs` — `FileConfig` sections + env-wins pattern (`ENV_*` consts, `:11-16`, merge at `:169+`); add `OidcSection` here
- `src/session.rs` — HMAC session mint/verify; callback must reuse it verbatim (D-02)
- `src/server.rs` — `login_form` (`:423`), `login_submit` (`:433`), logout; login page chrome
- `migrations/0001_users.sql` — users table shape (email normalization, role, bcrypt hash)
- `src/store.rs` — user lookup/creation, groups (`admin_groups` surface), logging helpers

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Session cookie machinery in `src/session.rs` is the single sign-in artifact — SSO success path ends exactly where `login_submit` ends.
- Config env-wins pattern (`MORINET_*` env over `[section]` file keys) has an established shape to copy for `[oidc]`.
- Groups already exist (admin groups page + membership tables) — group sync writes there.

### Established Patterns
- Fail-closed security posture everywhere (Phase 15 precondition parsers, Phase 16 404-not-403).
- TDD per plan; full `cargo test --lib` green gate; `./scripts/check.sh` gate; no `--no-verify`.
- Migrations are additive-only, never rebuild tables (SQLite).

### Integration Points
- `login_form`/`login_submit` (SSO button + divider), `account_page` ("linked via SSO" indicator), `config.rs` (`[oidc]`), new `src/oidc.rs` module + 2 routes (`/sso/login`, `/sso/callback`), migration `0012_user_identities.sql`.

</code_context>

<specifics>
## Specific Ideas
- Test strategy: mock OIDC provider in tests (local `wiremock`-style or a tiny in-test IdP exposing discovery/JWKS/token endpoints with a test RSA key) — no live Keycloak in CI; `openidconnect` accepts a manually constructed ProviderMetadata for this.
- Log link/demote events with actor + iss + sub (never tokens, never codes).

</specifics>

<deferred>
## Deferred Ideas
- Multi-IdP (multiple `[oidc]` providers) — table is ready (`provider` column), UI/config later
- Self-service unlink (D-13) — admin/DB only in v1
- RP-initiated logout (end_session_endpoint) — local `/logout` only in v1
- SCIM / full user lifecycle sync
- Just-in-time role elevation UI (admin mapping stays config-only)

</deferred>
