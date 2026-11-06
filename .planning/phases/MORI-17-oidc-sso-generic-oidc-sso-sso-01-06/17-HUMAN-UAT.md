---
phase: 17
status: pending
created: 2026-11-06
---

# Phase 17 Human UAT — Real-IdP SSO Smoke Test

The 62 oidc tests run against an in-process mock IdP. This checklist proves interop with a REAL IdP (Keycloak / Authentik / Authelia). Takes ~15 min on staging.

## Setup
1. In the IdP, create a confidential client `morinet` with redirect URI `https://<host>/sso/callback`, note client id/secret.
2. On the server, add to `/etc/morinet/config.toml`:
   ```toml
   [oidc]
   issuer = "https://<idp>/realms/<realm>"
   client_id = "morinet"
   allowed_email_domains = ["<your-domain>"]
   # client secret via env: MORINET_OIDC_CLIENT_SECRET=...
   ```
   Restart the service. Confirm the login page shows the SSO button.

## Checklist
- [ ] **New user**: IdP account on allowed domain, never seen by MoriNet → SSO login lands signed-in; account page shows "Linked via SSO"; role is reader (no vault admin).
- [ ] **Existing user**: local reader account with matching email → first SSO login links silently (no new account created — check Statistics/users).
- [ ] **Admin refusal**: try SSO with the bootstrap admin's email → clear refusal page, no session.
- [ ] **Second login**: sign out, sign in again via SSO → works (returning path keyed on iss/sub).
- [ ] **Coexistence**: password login still works for admin and readers while `[oidc]` is configured.
- [ ] **Safety net**: stop the IdP (or wrong issuer) → password login + bootstrap admin unaffected; SSO shows a refusal, not a crash.
- [ ] **Group sync (if configured)**: `groups_claim` + mapping → membership follows IdP groups across logins; removing the group in the IdP demotes on next login and appears in the auth log.

## Expected evidence
Each row: screenshot or `sqlite3 morinet.db "SELECT kind,detail,actor_email FROM auth_events ORDER BY id DESC LIMIT 10"` excerpt.
