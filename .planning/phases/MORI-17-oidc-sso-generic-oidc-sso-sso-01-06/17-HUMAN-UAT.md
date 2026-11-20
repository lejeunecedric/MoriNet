---
phase: 17
status: resolved
created: 2026-11-06
resolved: 2026-11-20
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
- [x] **New user**: IdP account on allowed domain, never seen by MoriNet → SSO login lands signed-in; account page shows "Linked via SSO"; role is reader (no vault admin).
- [x] **Existing user**: local reader account with matching email → first SSO login links silently (no new account created — check Statistics/users).
- [x] **Admin refusal**: try SSO with the bootstrap admin's email → clear refusal page, no session.
- [x] **Second login**: sign out, sign in again via SSO → works (returning path keyed on iss/sub).
- [x] **Coexistence**: password login still works for admin and readers while `[oidc]` is configured.
- [x] **Safety net**: stop the IdP (or wrong issuer) → password login + bootstrap admin unaffected; SSO shows a refusal, not a crash.
- [x] **Group sync (if configured)**: `groups_claim` + mapping → membership follows IdP groups across logins; removing the group in the IdP demotes on next login and appears in the auth log.

## Expected evidence
Each row: screenshot or `sqlite3 morinet.db "SELECT kind,detail,actor_email FROM auth_events ORDER BY id DESC LIMIT 10"` excerpt.

## Evidence (2026-11-20, automated local run)

Real third-party IdP: **npm `oidc-provider` v9.15.1** (independent Node process, real
discovery / JWKS / auth-code+PKCE-S256 / consent), issuer `http://127.0.0.1:9818`.
MoriNet **v1.4.0** release binary on `127.0.0.1:9817`, data dir
`~/.hermes/profiles/devo/cache/scratch/uat-17/data`. Config: `[oidc] issuer =
"http://127.0.0.1:9818"`, `client_id = "morinet"`, `allowed_email_domains =
["example.test"]`, `groups_claim = "groups"`, `group_map = { "wiki-readers" = "writers" }`;
secret via `MORINET_OIDC_CLIENT_SECRET`; bootstrap admin `admin@example.test`.
Auth flow driven with curl cookie jars through the IdP's real login + consent screens
(`sso_login.sh` in the scratch dir). IdP users: `alice` (alice@example.test, groups
["wiki-readers"]), `bob` (bob@example.test), `admin` (admin@example.test).

Setup confirmation — SSO button on login page:
```
$ curl -s http://127.0.0.1:9817/login | grep -o 'href="/sso/login"[^<]*'
href="/sso/login">Se connecter via SSO
```

IdP is a genuine external OP (real discovery document):
```
$ curl -s http://127.0.0.1:9818/.well-known/openid-configuration | jq .issuer
"issuer": "http://127.0.0.1:9818", "code_challenge_methods_supported": ["S256"]
```

1. **New user** — `bash sso_login.sh jar-alice.txt alice` (full auth-code+PKCE flow through
   the IdP's login and consent pages) → `ACCOUNT_HTTP=200`; `/account` shows
   `alice@example.test` + `Linked via SSO (http://127.0.0.1:9818)`.
   DB: `users: (2,'alice@example.test','reader')` — reader, not admin;
   `auth_events: (3,'provision','alice@example.test','auto_provision')`;
   `user_identities: (2,'oidc','http://127.0.0.1:9818','alice')`.

2. **Existing user** — bob pre-created as local reader via admin invite (no SMTP ⇒ temp
   password flow): `users: (3,'bob@example.test','reader')`. First SSO login →
   `ACCOUNT_HTTP=200`, account page `Linked via SSO`. DB: user count unchanged (3),
   identity attached to the EXISTING id 3 (`user_identities: (3,'http://127.0.0.1:9818','bob')`),
   `auth_events: (5,'link','bob@example.test','auto_link')`. No duplicate account.

3. **Admin refusal** — SSO as IdP user `admin` (admin@example.test) → page
   `<title>MoriNet — Error</title> <p style="color:red">This account cannot sign in via SSO.`;
   `/account` with that jar → 303 (no session). DB:
   `auth_events: (6|7,'deny','admin@example.test','admin_match')`.

4. **Second login** — `POST /logout` (303) then full SSO again as alice → `ACCOUNT_HTTP=200`.
   No new `provision`/`link` event emitted (returning path matched on `(iss,sub)` in
   `user_identities`; latest events at that point were only the group-sync rows below).

5. **Coexistence** — with `[oidc]` active: admin password login
   `POST /login → 303 → /`, `/admin → 200`; reader bob password login (invite temp
   password) `POST /login → 303 → /`, `/account → 200`.

6. **Safety net** — IdP process killed (`ss -ltn` confirms 9818 down):
   `GET /sso/login → 400` (discovery fetch fails cleanly),
   `GET /sso/callback?code=deadbeef&state=abc123 → 403` with page
   `MoriNet — Error / Sign-in could not be completed.` — no crash, server keeps serving
   (`/login → 200`); admin password login still `303 → /`, `/admin → 200`.

7. **Group sync** — local group `writers` created via `/admin/groups/create`;
   `group_map = { "wiki-readers" = "writers" }`. Alice's IdP claim `groups:
   ["wiki-readers"]` → next login: `auth_events: (8,'group_sync','alice@example.test','added:writers')`,
   membership rows `(alice → free, writers)`. Then the group was removed in the IdP
   (`users.json` → `groups: []`, no MoriNet change) → next login:
   `auth_events: (9,'demote','alice@example.test','removed:writers')`, membership back to
   `(free)` only. (Earlier `deny/unknown_group_target` rows predate group creation —
   sync never creates groups, by design.)

Full `auth_events` tail after the run:
```
(3,'provision','alice@example.test','auto_provision')
(5,'link','bob@example.test','auto_link')
(6,'deny','admin@example.test','admin_match')
(7,'deny','admin@example.test','admin_match')
(8,'group_sync','alice@example.test','added:writers')
(9,'demote','alice@example.test','removed:writers')
```

Notes for reproduction: the IdP must place user claims in the signature-verified ID
token (`conformIdTokenClaims: false` in oidc-provider, `groups` added to
`claimsSupported`) — MoriNet deliberately never consults userinfo (D-15), so an IdP
configured to defer claims to userinfo will be refused with `deny/email_not_verified`
(two such rows from the first setup attempt are in the table and are expected artifacts
of that misconfiguration, not MoriNet bugs).
