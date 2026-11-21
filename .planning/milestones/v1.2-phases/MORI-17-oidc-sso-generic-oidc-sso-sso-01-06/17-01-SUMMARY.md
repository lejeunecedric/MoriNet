# 17-01 SUMMARY — oidc-config-migration-and-password-regression-baseline

**Status:** COMPLETE — all 3 tasks shipped, all final gates green.
**Branch:** main (not pushed, per orchestrator rule)
**Date:** 2026-11-05

## What shipped

### Task 1 — password-login regression baseline (SSO-06, D-08)
Nested module `mod sso_password_regression` inside `mod tests` in `src/server.rs`
(uses `super::*` — reuses `test_router()`, `req()`, `login_headers()` verbatim).
Four tests locking CURRENT password-login behavior before any OIDC code existed:

- `sso_reg_password_login_mints_exact_session_cookie` — exact `session=uid:ts.sig` shape + 302 `/`
- `sso_reg_password_login_wrong_password_401` — 401, no session cookie
- `sso_reg_bootstrap_admin_always_available` — bootstrap admin password login works with `[oidc]` absent
- `sso_reg_password_login_works_for_reader` — reader role login path intact

Green at introduction **by design** (baseline lock, not RED-first — its purpose is
to prove later waves did not disturb login, per D-08 intra-phase ordering).

### Task 2 — `[oidc]` config section (SSO-05, D-06) — `src/config.rs`
- Four env constants: `ENV_OIDC_ISSUER` / `ENV_OIDC_CLIENT_ID` / `ENV_OIDC_CLIENT_SECRET` /
  `ENV_OIDC_ALLOWED_DOMAINS` (`MORINET_OIDC_*`).
- `OidcSection` (all-Option + `serde(default)`, `Default` derive; fields: issuer,
  client_id, client_secret, sso_label, allowed_email_domains, groups_claim, group_map).
- `OidcRuntime` (resolved, all-or-nothing): issuer, client_id, client_secret,
  allowed_email_domains, groups_claim (default `"groups"`), group_map, sso_label
  (default `"SSO"`).
- `resolve_oidc()`: env-wins per field via `env_nonblank()` (set-but-blank env =
  UNSET — Phase 12 lesson); SSO enables only when issuer + client_id +
  client_secret ALL resolve; partial set ⇒ `None` + one startup warning (never a
  hard boot error, never half-enabled); allowed domains normalized through
  `normalize_locked_list` (trim, lowercase, drop empties).
- `RuntimeConfig.oidc: Option<OidcRuntime>`; test literals updated with
  `oidc: None` (server.rs `test_config()` — covers `email_cfg()` via spread — and
  api.rs `router_cfg()`).
- Six tests: `oidc_absent_section_disables`, `oidc_full_section_enables`,
  `oidc_env_wins_over_file`, `oidc_blank_env_does_not_shadow_file`,
  `oidc_partial_config_disables_not_half_enabled`, `oidc_allowed_domains_env_normalized`
  (all under `ENV_LOCK`, env save/restore helpers).

### Task 3 — migration 0012 + store helpers (D-11, D-05)
- `migrations/0012_user_identities.sql` (additive CREATE TABLE only; `users` untouched):
  - `user_identities(id, provider, iss, sub, user_id REFERENCES users ON DELETE CASCADE,
    linked_at)` with `UNIQUE(provider, iss, sub)` (T-17-02)
  - `auth_events(id, ts, kind, actor_email, iss, sub, detail)` with
    `CHECK (kind IN ('link','provision','deny','demote','group_sync'))` and **no
    token/code column at all** (T-17-04); `idx_auth_events_ts`.
- Store helpers (`src/store.rs`): `link_identity` (INSERT OR IGNORE → bool),
  `identity_for_user`, `user_by_provider_iss_sub` (JOIN users → id/email/role),
  `log_auth_event`, `prune_auth_events` (90-day window, mirrors `prune_access_log`).
- `src/main.rs`: `prune_auth_events()` called at boot beside the other prunes,
  best-effort (warn on error, never abort boot).
- Six tests in `mod identity_tests`: `user_identities_migration_shape` (PRAGMA
  column-set lock + unique-index check + no-token-column assertion),
  `auth_events_check_rejects_unknown_kind`, `identity_link_roundtrip`,
  `identity_unique_conflict_is_ignored`, `identity_for_user_none_when_unlinked`,
  `log_auth_event_and_prune_window`.

## Commits (atomic, per plan commit protocol)

| Hash | Message |
|---|---|
| `8c40826` | `test(sso): lock password-login regression baseline before OIDC work (SSO-06, D-08)` |
| `0b2d0e4` | `feat(config): add opt-in [oidc] section with env-wins, disabled by default (SSO-05, D-06)` |
| `f4893cd` | `feat(store): add user_identities + auth_events migration and helpers (D-11, D-05)` |

## TDD evidence

- **Task 2 RED:** `cargo test --lib oidc_` → compile failure, 16 `error[...]`
  lines (OidcSection/OidcRuntime/ENV_OIDC_* absent). **GREEN:** 6 oidc_* tests
  pass; full crate green at that point (416 passed, 0 failed).
- **Task 3 RED:** `cargo test --lib identity_` → compile failure, 11
  `error[...]` lines (store helpers absent). **GREEN:** 6 identity tests pass
  (identity_ 6, auth_events 1, user_identities 1, log_auth 1 — same module).
- **Task 1:** green at introduction by design (baseline lock; 4/4 via
  `cargo test --lib sso_reg_`), re-verified green after tasks 2–3.

## Final gates (all verified)

- `cargo test --lib` → **422 passed, 0 failed, 4 ignored** (was 410 + 4 sso_reg + 6 oidc + 6 identity = 426 total incl. 4 ignored)
- `cargo clippy --all-targets -- -D warnings` → clean
- `cargo fmt --check` → clean
- `test -f migrations/0012_user_identities.sql` ✓; grep `UNIQUE(provider, iss, sub)` ✓;
  grep kind CHECK vocab ✓; grep `prune_auth_events` in store.rs + main.rs ✓
- `grep -rn "oidc: None" src/` → 3 occurrences (≥3 required) ✓
- `git diff --exit-code Cargo.toml Cargo.lock` → clean (no new dependencies)
- `wc -l src/config.rs` → 803 (< 1000) ✓
- Working tree clean; nothing pushed.

## Deviations from plan

1. **`email_cfg()` also gained `oidc: None` explicitly** — the plan said "all three
   test literals"; `email_cfg()` spreads `..test_config()` so it needed no change,
   but the explicit field documents SSO-off intent and keeps the grep gate at 3.
   No behavioral difference.
2. **rustfmt fixes to task-2 code were folded into the task-3 commit** (final
   `cargo fmt` reflowed two spots in config.rs/server.rs after task 2 was
   committed; noted in the task-3 commit body).
3. Test-module placement: task-1 tests live in a **nested** `mod sso_password_regression`
   inside `mod tests` (needed for `super::*` access to test helpers); task-3 tests
   are a top-level `mod identity_tests` per the store.rs pattern. Both match the
   plan's anchor intent.

## For wave 2 (oidc-core) — integration surface now available

- `state.config.oidc: Option<OidcRuntime>` — `None` ⇔ SSO disabled; every route
  must gate on it.
- `store.user_by_provider_iss_sub("oidc", iss, sub)` — authoritative returning-user
  lookup (D-03); `store.link_identity` for first-link; `store.log_auth_event`
  best-effort for the five D-05 vocab events.
- Password-login behavior is locked by `sso_reg_*` — any wave that breaks them
  broke SSO-06.
