---
phase: 17
status: verified
threats_open: 0
asvs_level: 1
audited_head: 9fab783
created: 2026-11-06
---

# Phase 17 Security Verification — SECURED

Verify-only audit of threat register T-17-01..T-17-24 (plan `<threat_model>` blocks) against final code at `9fab783`, including post-plan hardening (`a296562` verify_session discriminator, `108ebf5` review fixes REV-17-01/03/04/05/06).

## Result

- **24/24 threats CLOSED** (T-17-10 and T-17-24 closed with plan-sanctioned accepted residuals).
- **0 OPEN.** No new threats at MEDIUM or above.
- Evidence tests: `cargo test --lib 'oidc::'` 62 passed / 0 failed; `cargo test --lib 'sso_reg_'` 4 passed / 0 failed; full suite 493/0.

## Key adversarial checks (all clean)

- **Session/state codec split** — `verify_session` rejects newline-bearing payloads (state trio always contains `\n`); all three production `codec.sign` sites swept; forging requires the HMAC secret. No bypass found.
- **Anonymous flood** — pre-parse callback refusals write zero DB rows (`matrix_a2_deny_flood_not_audited`: 90 → 0).
- **Callback exits** — 7 total; all 6 post-issuance exits clear the state cookie.
- **Provisioning** — role hard-pinned reader, bcrypt(2×uuid-v4) placeholder, exact-domain allowlist, empty allowlist refuses all.
- **Group sync** — claims only from JWKS-verified ID token; managed-set = mapping values; last-admin guard under `SYNC_GROUPS_LOCK`; every mutation audited.
- **Supply chain** — openidconnect rustls-only (single rustls 0.23.46, no native-tls); migration 0012 purely additive.

## Accepted residuals (documented)

- No `Secure` cookie attribute (pre-existing Phase 12 format; D-02 byte-identity forbids drift; deployment behind TLS termination).
- IdP discovery/JWKS uncached per request (REV-17-02).
- Unauthenticated `/sso/login` costs one URL build (T-17-10, mirrors Phase 16 acceptance style).
- Provision-then-link ignores `link_identity` errors; worst case re-link next login (INFO).

Full per-threat table with file:line + test evidence: auditor transcript `deleg_4516c98f/task-0.log`.
