# Phase 17: oidc-sso — Discussion Log

**Date:** 2026-11-01
**Mode:** interactive (desktop clarify form, all answered)

## Area selection

**Question:** which gray areas to discuss before planning?
**Selected (all four):** group-sync semantics, identity storage, account-linking policy, login UX.

## Q1 — Group sync semantics (research conflict ARCHITECTURE vs PITFALLS A5)

**Options presented:**
1. Replace-on-login: managed groups recomputed from claim each login; claim absent = keep; claim present-but-empty = remove managed; demotions logged (recommended, closes A5)
2. Additive-only: never remove — stale memberships persist forever

**Decision:** Replace-on-login (option 1). → D-09, D-10 (managed set = groups named in the configured mapping; unmapped groups untouched; admin mapping opt-in, fail-closed on absent claim).

## Q2 — Identity storage for (iss, sub)

**Options presented:**
1. Dedicated `user_identities(provider, iss, sub, user_id, UNIQUE)` table (recommended)
2. `oidc_iss`/`oidc_sub` columns on `users`

**Decision:** Dedicated table (option 1) → D-11. Note: migration number shifts to **0012** — ROADMAP's "migration 0011" predates Phase 15 consuming 0011.

## Q3 — Linking existing accounts

**Options presented:**
1. Auto-link on `email_verified` match, never admin accounts, all links logged (recommended, matches ROADMAP criterion 2)
2. Explicit link requiring password re-auth on first SSO login

**Decision:** Auto-link (option 1) → D-12 (admin match = refuse with clear error).

## Q4 — Self-service unlink

**Options presented:**
1. No — account page shows "linked via SSO", unlink is admin/DB in v1 (recommended, keeps phase thin)
2. Yes — unlink button when a local password exists

**Decision:** No (option 1) → D-13.

## Q5 — Login page UX with [oidc] configured

**Options presented:**
1. Single "Se connecter via SSO" button above the password form, "or" divider, configurable `sso_label` (recommended)
2. Email-first unified flow deciding SSO vs password per address

**Decision:** Button above form (option 1) → D-14.

## Follow-up notes
- ROADMAP migration-number drift (0011 → 0012) flagged and recorded in CONTEXT.
- Deferred: multi-IdP, RP-initiated logout, self-service unlink, SCIM.
- Test strategy idea: in-test mock IdP (discovery + JWKS + token endpoints, test RSA key); no live Keycloak in CI.
