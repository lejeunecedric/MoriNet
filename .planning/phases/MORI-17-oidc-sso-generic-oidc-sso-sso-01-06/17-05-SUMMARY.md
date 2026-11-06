---
id: "17-05"
phase: 17
name: security-matrix-and-config-docs
completed: 2026-11-06
commits: [a296562, 8a0b335, 7e49c5b]
requirements: [SSO-05, SSO-06]
---

# 17-05 Summary: end-to-end security matrix + operator docs

## What was built

**Task 1 — security matrix (`8a0b335`, `src/oidc.rs` tests mod)**
18 `matrix_*` tests driving the REAL router + mock IdP, one per ROADMAP
"Avoids" entry. Every refusal asserts BOTH the HTTP status AND its
`auth_events` row (kind + detail) via `count_auth_events` — D-05 made
mechanically checkable. No matrix test asserts only "non-200".

- **A1 email-takeover (3):** `matrix_a1_admin_never_linked` (403 + deny
  `admin_match` + no session cookie + admin gained no identity row);
  `matrix_a1_relinked_identity_ignores_email` (sub-A pinned to victim despite
  attacker email claim, 0 `auto_link` rows, attacker email gains no account);
  `matrix_a1_email_change_at_idp_does_not_move_identity` (same sub → same
  user, no re-provision, no re-link audit).
- **A2 state/nonce/PKCE/replay (6):** `matrix_a2_missing_state`,
  `matrix_a2_forged_state` (foreign-secret codec), `matrix_a2_expired_state`
  (signed with `now - TTL - 60`) — all 403 + deny `state_invalid`;
  `matrix_a2_nonce_mismatch` (403 + deny `nonce_mismatch`);
  `matrix_a2_code_replay` (same callback URL twice under IdP single-use
  enforcement: first 303, replay 403 + deny `token_invalid`, no session on
  replay); `matrix_a2_pkce_verifier_required` (challenge swapped post-hoc →
  403 + deny `token_invalid`, verifier proven to have reached the IdP via
  `last_verifier`).
- **A5 groups escalation (3):** `matrix_a5_no_admin_claim_without_mapping`
  (claim `["admins","admin"]` with only `dev→writers` mapped: admin group
  stays empty, 0 `added:admin` rows, and the signed-in reader is 403 at
  `/admin` — T-17-16); `matrix_a5_access_token_claim_ignored` (groups in the
  access token never consulted); `matrix_a5_forged_key_rejected` (ID token
  signed with foreign key → 403 + deny `token_invalid`).
- **A6 session fixation (2):** `matrix_a6_fresh_cookie_on_sso_login` (hostile
  pre-set `morinet_session` replaced by the fresh mint; hostile token alone
  still redirects to `/login`); `matrix_a6_no_preauth_identifier` (the state
  token from `/sso/login` must never verify as a session — see deviation).
- **A7 gate bypass (3):** `matrix_a7_provisioned_reader_blocked_from_admin`
  (provision `auto_provision` row, `/` 200, `/admin` 403);
  `matrix_a7_unmatched_email_gets_no_cookie` (403, no `morinet_session=`
  anywhere in Set-Cookie, deny `domain_not_allowed`);
  `matrix_a7_sso_disabled_routes_404` (no `[oidc]`: both routes 404 AND
  `/login` body byte-identical to `LOGIN_HTML` — SSO-05 removability).
- **Coexistence (1):** `matrix_coexist_password_and_sso_side_by_side`
  (bootstrap admin + password reader + SSO reader all sign in, identical
  gates, admin login still works after SSO activity — SSO-06).

Mock IdP addition (test-only, opt-in): `enforce_code_single_use()` +
`used_codes` set — `/token` refuses a code it already exchanged, mirroring
real IdP single-use authorization codes. Off by default; existing tests
untouched.

**Task 2 — A6 gap fix (`a296562`, `src/session.rs` + `src/server.rs`)**
`matrix_a6_no_preauth_identifier` went RED on first run: the SSO state trio
is signed with the SAME codec as sessions, so any anonymous visitor could
GET `/sso/login`, take the state cookie, and present it as
`morinet_session=…` — `require_session` accepted it as a phantom session
(payload lands in `Claims.email`). Fixed in the owning module:
`SessionCodec::verify_session` = `verify` + reject payloads containing
`\n` (the state trio payload is `state\nnonce\nverifier`; session emails
never contain newlines, so no legitimate token is affected). Both
`require_session` and `require_admin` gates switched to it; plain `verify`
kept for state validation. Unit regression
`verify_session_rejects_multiline_payloads` in `src/session.rs`.

**Task 3 — docs + init template (`7e49c5b`)**
- `docs/morinet-docs/src/config-reference.md`: new `## [oidc]` section
  mirroring the `[smtp]`/`[security]` voice — intro "Optional. An absent or
  incomplete section disables SSO entirely…", key table (issuer, client_id,
  client_secret, allowed_email_domains, groups_claim default "groups",
  group_map, sso_label default "SSO"), env-override table (the four
  `MORINET_OIDC_*` vars, env wins, blank = unset), and the fail-closed rules
  list verbatim: partial config ⇒ disabled/404; exact-domain +
  `email_verified: true`; admin never auto-linked; linked email inert;
  D-12 lowercase-normalization divergence from password login;
  replace-on-login semantics (absent = keep, present-empty = remove managed
  set only, adds/removals audited, last-admin guard); 90-day `auth_events`
  retention; no self-service unlink.
- `src/init.rs`: commented-out `[oidc]` example block appended to the
  generated template (every line `#`-prefixed; generated config stays
  functionally identical and disabled).

## TDD evidence

- RED: `cargo test --lib matrix_` → 17/18 pass,
  `matrix_a6_no_preauth_identifier` fails (state token authenticated as a
  session — real gap, not a test bug).
- GREEN: after `verify_session` fix, full `cargo test --lib` →
  **491 passed, 0 failed, 4 ignored** (472 baseline + 18 matrix + 1 session
  regression). `sso_reg_*` baseline tests UNEDITED.
- `cargo clippy --lib -- -D warnings` clean; `cargo fmt --check` clean.
- Docs grep gates: `## [oidc]` present; `MORINET_OIDC_` ×4; `[oidc]` in
  init template (all block lines `#`-commented); rules list contains
  "absent"/"keep", "email_verified", admin refusal, lowercase note.

## Deviations

1. **Production-code change beyond the init template** (plan explicitly
   allows: "if a matrix test exposes a real gap, fix it in the owning module
   and note it in SUMMARY"): `session.rs` gained `verify_session`;
   `server.rs` gates (`require_session`, `require_admin`) call it — two
   one-word call-site edits. The A6 hole was real and pre-existing since
   17-02 shipped the state cookie.
2. Mock IdP grew the opt-in single-use-code gate (test infrastructure only,
   off by default).
3. `cargo fmt` reformatted some of the new matrix test lines (cosmetic).

## Coverage map (Avoids → tests)

| Avoids | Tests |
| --- | --- |
| A1 email-takeover | matrix_a1_admin_never_linked, matrix_a1_relinked_identity_ignores_email, matrix_a1_email_change_at_idp_does_not_move_identity |
| A2 state/nonce/PKCE/replay | matrix_a2_missing_state, matrix_a2_forged_state, matrix_a2_expired_state, matrix_a2_nonce_mismatch, matrix_a2_code_replay, matrix_a2_pkce_verifier_required |
| A5 groups escalation | matrix_a5_no_admin_claim_without_mapping, matrix_a5_access_token_claim_ignored, matrix_a5_forged_key_rejected |
| A6 session fixation | matrix_a6_fresh_cookie_on_sso_login, matrix_a6_no_preauth_identifier (+ session.rs unit test) |
| A7 gate bypass | matrix_a7_provisioned_reader_blocked_from_admin, matrix_a7_unmatched_email_gets_no_cookie, matrix_a7_sso_disabled_routes_404 |
| Coexistence SSO-06 | matrix_coexist_password_and_sso_side_by_side |
