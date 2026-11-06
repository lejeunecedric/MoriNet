---
phase: 17
name: oidc-sso
verified: 2026-11-06
status: passed
verdict: Phase 17 meets all 7 ROADMAP success criteria and SSO-01..06; all D-01..D-15 decisions honored.
---

# Phase 17 Verification — Generic OIDC SSO

**Verdict: PASSED.** All gates re-run fresh on the current tree (HEAD `bd3d673`), every named test located in `src/` and executed, all decisions spot-checked against code.

## Gate results (re-run 2026-11-06)

| Gate | Result |
|---|---|
| `cargo test --lib` | **491 passed, 0 failed, 4 ignored** (matches 17-05 final gate) |
| `./scripts/check.sh` | **check: OK** (fmt, clippy `-D warnings`, tests, release build) |
| Targeted filters (below) | 13/13 passed |
| Working tree | clean; nothing pushed |

Targeted run: `cargo test --lib -- sso_reg_ sso_callback_happy sso_auto_links sso_admin_match_refused sso_auto_provisions group_sync_claim_absent_keeps group_sync_claim_present_empty matrix_a1_admin_never_linked matrix_a6_no_preauth_identifier matrix_coexist` → **13 passed, 0 failed**.

## ROADMAP success criteria

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | SSO button when `[oidc]` configured; auth-code + PKCE S256 with `state` + `nonce`; discovery + JWKS-validated ID token (SSO-01) | ✅ | `login_page_shows_sso_button_when_oidc_configured` (oidc.rs:1598); PKCE `S256` + state + nonce in `sso_login` (oidc.rs:85,115); JWKS validation proven by `matrix_a5_forged_key_rejected` (oidc.rs:3011, foreign key → 403 + deny `token_invalid`); mock IdP serves real discovery/JWKS (`MockIdp`) |
| 2 | `email_verified` match auto-links + signs in; admin never auto-linked; every link logged; `(iss, sub)` authoritative (SSO-02) | ✅ | `sso_auto_links_verified_email_match` (oidc.rs:1728); `sso_admin_match_refused` (1877) + `matrix_a1_admin_never_linked` (2631, 403 + deny `admin_match`, no identity row); link logged via `log_auth_event("link", …, "auto_link")` (oidc.rs:419); `user_by_provider_iss_sub` (store.rs:1512) is the returning-user path; `matrix_a1_relinked_identity_ignores_email` (2672) + `matrix_a1_email_change_at_idp_does_not_move_identity` |
| 3 | First-time allowed-domain user auto-provisioned `reader`; unmatched fails closed (SSO-03) | ✅ | `sso_auto_provisions_reader_for_allowed_domain` (oidc.rs:1916); role hard-pinned reader — `create_user_without_password` rejects non-reader (store.rs:3947); `matrix_a7_unmatched_email_gets_no_cookie` (deny `domain_not_allowed`, no session cookie) |
| 4 | Configurable groups claim syncs at each login, ID-token-only; admin mapping opt-in fail-closed; demotions logged (SSO-04) | ✅ | `sync_groups` (oidc.rs:534) reads `extra.get(&oidc.groups_claim)` — never hardcoded; claim name default `groups` (config.rs); `group_sync_claim_absent_keeps_memberships` (2293) / `group_sync_claim_present_empty_removes_managed` (2332); removals logged `demote`/`removed:{group}`; `group_sync_reads_only_id_token` (2564) + `matrix_a5_access_token_claim_ignored` + `matrix_a5_no_admin_claim_without_mapping` |
| 5 | `[oidc]` config with env-wins; disabled by default; removable without affecting password login (SSO-05) | ✅ | `oidc_absent_section_disables`, `oidc_env_wins_over_file`, `oidc_blank_env_does_not_shadow_file`, `oidc_partial_config_disables_not_half_enabled` (config.rs:698,769,…); `matrix_a7_sso_disabled_routes_404` (oidc.rs:3171) asserts both `/sso/*` routes 404 **and** `/login` byte-identical to pre-Phase-17 `LOGIN_HTML`; docs `## [oidc]` in config-reference.md + commented init template |
| 6 | Password login works for every account incl. SSO users; bootstrap admin always available (SSO-06) | ✅ | `sso_reg_*` baseline suite (server.rs:9549–9658) green and UNEDITED all phase; `sso_reg_bootstrap_admin_always_available` (9621); `matrix_coexist_password_and_sso_side_by_side` (3199): admin + password reader + SSO reader side by side |
| 7 | Migration applies cleanly on existing DBs, no table rebuild; password regression suite green before and after | ✅* | Migration is **0012** `user_identities` + `auth_events` — pure additive `CREATE TABLE IF NOT EXISTS`, `users` untouched, no ALTER/rebuild (D-11 explicitly supersedes the ROADMAP's "0011 `users.provider`" sketch: 0011 was consumed by Phase 15 `0011_vault_changes_forced_actor.sql` — confirmed in `migrations/`). `sqlx::migrate!` runs the full 0001→0012 chain on every `Store::open`/`connect_memory` (store.rs:117,127) — 491 tests exercise it; `user_identities_migration_shape` (store.rs:3664) locks column set + unique index. Provisioned users get bcrypt over a random 2×uuid placeholder (`create_user_without_password`, store.rs:1633; guess test at 3965). `sso_reg_*` green before (17-01 baseline commit `8c40826` first) and after. *ROADMAP text says "0011"; CONTEXT D-11 documents the supersession — deviation is sanctioned, not a gap. |

## SSO-01..06 requirement mapping

| Req | Delivered by | Key tests |
|---|---|---|
| SSO-01 | `src/oidc.rs` `sso_login`/`sso_callback`, login button | `sso_callback_happy_path_mints_session_cookie`, `login_page_*` (3), `mock_idp` harness test |
| SSO-02 | identity-resolution policy (17-03) | `sso_auto_links_verified_email_match`, `sso_admin_match_refused`, `matrix_a1_*` (3) |
| SSO-03 | `create_user_without_password` + domain allowlist | `sso_auto_provisions_reader_for_allowed_domain`, `matrix_a7_provisioned_reader_blocked_from_admin` |
| SSO-04 | `sync_groups` replace-on-login (17-04) | 11 `group_sync_*` + `matrix_a5_*` (3) |
| SSO-05 | `OidcSection`/`resolve_oidc` env-wins (17-01) | 6 `oidc_*` config tests, `matrix_a7_sso_disabled_routes_404` |
| SSO-06 | `sso_reg_*` baseline + coexistence matrix | 4 `sso_reg_*`, `matrix_coexist_password_and_sso_side_by_side` |

## D-01..D-15 decision spot-checks

- **D-01** ✅ `openidconnect = "4.0.1"` (rustls-tls only; `rsa` dev-dep only; no wiremock/mockito — dep audit in 17-02, rustls=1, native-tls=0). PKCE S256 + state + nonce + JWKS validation verified by matrix A2 (6 tests) + forged-key test.
- **D-02** ✅ `sso_callback_cookie_byte_identical_to_password_login` (oidc.rs:1354) asserts the minted cookie against the same `SessionCodec`; single `DEFAULT_TTL_SECS` mint tail.
- **D-03** ✅ `(iss, sub)` lookup first (`user_by_provider_iss_sub`); email never a join key for returning users (`matrix_a1_relinked_identity_ignores_email`).
- **D-04** ✅ reader-only provisioning, fail-closed `domain_not_allowed` deny.
- **D-05** ✅ admin match refused (403 + deny); `auth_events` CHECK-locked vocab (`link/provision/deny/demote/group_sync`), every matrix refusal asserts its audit row via `count_auth_events`.
- **D-06** ✅ disabled by default (`oidc_absent_section_disables`, partial ⇒ disabled not half-enabled); **login page byte-identical test exists**: `login_page_no_sso_ui_when_oidc_absent` (oidc.rs:1588) + `matrix_a7_sso_disabled_routes_404` body equality vs `LOGIN_HTML`.
- **D-07** ✅ bootstrap admin password login test with `[oidc]` absent: `sso_reg_bootstrap_admin_always_available`; coexistence matrix re-checks admin login after SSO activity.
- **D-08** ✅ git log order: baseline `test(sso)` commit `8c40826` precedes all OIDC feature code; group sync (`7447d6a`) shipped last before the matrix.
- **D-09** ✅ replace-on-login: absent claim keeps memberships; present-empty removes managed set; removals logged `demote`.
- **D-10** ✅ managed set = `group_map` values only (`group_id_for_name`, unknown target → deny `unknown_group_target`, never auto-creates groups); unmapped groups untouched.
- **D-11** ✅ migration **0012** `user_identities(id, provider, iss, sub, user_id→users ON DELETE CASCADE, linked_at)` with `UNIQUE(provider, iss, sub)`; `users` untouched.
- **D-12** ✅ strict `email_verified == Some(true)` (oidc.rs:371) + `LOWER(email)` normalized match (SSO path only; password login still exact-match); admin match → refuse.
- **D-13** ✅ "Linked via SSO ({iss})" indicator (server.rs:1262); no unlink route anywhere in server.rs/oidc.rs (grep clean).
- **D-14** ✅ button above password form with "ou" divider (server.rs:99), `sso_label` configurable + HTML-escaped (`login_page_sso_label_configurable_and_escaped`).
- **D-15** ✅ `groups_claim` configurable (default `groups`), read only from signature-verified ID token (`group_sync_reads_only_id_token` smuggle test).

## No plaintext tokens/codes stored

- `auth_events` insert columns: `(kind, actor_email, iss, sub, detail)` only (store.rs:1552) — no token/code/secret column exists in the schema at all (T-17-04).
- `user_identities` schema: no credential columns.
- SSO state trio (`state\nnonce\nverifier`) lives only in a short-lived (600 s) signed `morinet_sso_state` cookie — never persisted server-side.
- Bonus hardening found in-phase: `verify_session` rejects multiline payloads (a296562) so the state token can never authenticate as a session (`verify_session_rejects_multiline_payloads`, session.rs:118) — closes a real A6 gap the matrix caught RED-first.

## Notes / non-blocking

- ROADMAP criterion 7 text says "migration 0011 (`users.provider`)" — superseded by CONTEXT D-11 (0011 consumed by Phase 15); implementation follows the sanctioned 0012 design. ROADMAP text is stale, not the code.
- 17-03 executor crashed mid-run (context limit); orchestrator verified patches green and committed `da873f3` — independently re-confirmed here by the full fresh suite.
- `matrix_a6_no_preauth_identifier` initially exposed a real session-confusion gap; fixed in `session.rs` with unit regression — good evidence the matrix has teeth.
