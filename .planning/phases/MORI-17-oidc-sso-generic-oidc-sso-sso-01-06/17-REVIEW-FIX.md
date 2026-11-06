---
phase: 17
findings_fixed: [REV-17-01, REV-17-03, REV-17-04, REV-17-05, REV-17-06]
findings_accepted: [REV-17-02, REV-17-07..11 (INFO)]
fix_commit: 108ebf5
created: 2026-11-06
---

# Phase 17 Review-Fix Report

## Fixed

- **REV-17-01 (MEDIUM) — pre-parse deny flood.** `/sso/callback` no longer writes `auth_events` rows before the state cookie parses: `idp_error` and missing/forged/expired-state refusals answer 403 + clear the state cookie with **zero DB writes**. Post-parse denies (state mismatch, token/nonce/iss/aud failures, resolution refusals) still audit — they require a valid signed cookie or a real IdP session, so they cannot be flooded anonymously. Regression tests: `matrix_a2_deny_flood_not_audited` (90 anonymous callbacks → 0 rows); `matrix_a2_missing_state` / `matrix_a2_forged_state` / `matrix_a2_expired_state` updated to assert 0 rows.
- **REV-17-03 (LOW) — control-char email.** After D-12 normalization, emails containing control chars or embedded whitespace are refused (`deny/invalid_email`, audited post-exchange) before any lookup/provision — no more silently-broken provisioned accounts. Test: `sso_email_control_chars_refused`.
- **REV-17-04 (LOW) — last-admin guard race.** `sync_groups` now runs under a process-wide `SYNC_GROUPS_LOCK` (tokio Mutex), serializing the check-then-act admin-removal guard across concurrent SSO logins. Single-binary deployment makes process serialization sufficient.
- **REV-17-05 (LOW) — link audit honesty.** `link_identity` result is matched explicitly: losing the race re-reads and proceeds as returning user **without** logging a `link` event; a DB error fails closed (`deny/link_error`). Existing `sso_link_race_falls_back_to_returning` still green.
- **REV-17-06 (LOW) — mid-diff audit abort.** `log_auth_event` failures inside `sync_groups` are best-effort (`let _ =`), matching the `log_access` precedent; applied membership changes are no longer left half-diffed by an audit insert failure.

## Accepted (documented, not fixed)

- **REV-17-02 (LOW) — IdP discovery/JWKS uncached.** Every `/sso/login` and callback fetches metadata + JWKS. Outbound relay vector is bounded by the same unauthenticated-request surface as `/login` itself; caching + refresh policy is a real feature (stale-JWKS handling) deferred, not a 5-line fix. Accepted for v1.
- **INFO ×5** — `identity_for_user` LIMIT-1 without ORDER (one row per user by design), missing `user_identities.user_id` index (table is tiny at v1 scale), 400/404 wording on IdP-unreachable, `base_url` trailing-slash double slash, `groups_claim` collision with a registered claim. All recorded for a future hardening pass.

## Gates after fix

`cargo test --lib` **493 passed / 0 failed / 4 ignored**; clippy `-D warnings` clean; `fmt --check` clean.
