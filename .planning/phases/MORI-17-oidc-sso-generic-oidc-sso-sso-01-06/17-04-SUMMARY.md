---
id: "17-04"
phase: 17
name: group-sync-replace-on-login
completed: 2026-11-06
commits: [ff6ce46, 7447d6a]
requirements: [SSO-04]
---

# 17-04 Summary: group sync, replace-on-login (D-09/D-10/D-15)

## What was built

**Task 1 — store membership queries (`ff6ce46`, `src/store.rs`)**
- `group_memberships_for_user(user_id) -> Vec<(i64, String)>` — the reversed join
  PATTERNS §5 flagged missing (`SELECT g.id, g.name FROM groups g JOIN
  group_members m ... ORDER BY g.name`).
- `group_id_for_name(name) -> Option<i64>` — exact-name resolve; sync never
  auto-creates groups.
- 3 tests (`group_sync_store_tests`): roundtrip+sort, empty-for-new-user,
  known/unknown resolve. TDD RED→GREEN.

**Task 2 — OIDC sync (`7447d6a`, `src/oidc.rs`)**
- Claim source (D-15): custom `OidcExtraClaims` (named-field `#[serde(flatten)]`
  map) on a Core-crypto `OidcClient` (only the additional-claims/token type
  params differ from Core). `exchange_and_verify` now returns
  `(VerifiedIdentity, HashMap<String, serde_json::Value>)` — the unregistered
  claims captured during the SAME signature-verified ID-token parse. Access
  token and userinfo are never parsed; `group_sync_reads_only_id_token` proves
  a groups claim smuggled into the access-token payload is ignored.
- `sync_groups(state, user_email, user_id, iss, sub, extra)` implements the
  locked diff: claim absent → keep (early return, no audit rows); present-empty
  → remove all managed; managed set = `group_map` values resolved via
  `group_id_for_name` (unknown target → skip + deny `unknown_group_target`);
  desired = mapped targets whose claim value is present; adds logged
  `group_sync`/`added:{group}`, removals `demote`/`removed:{group}`;
  wrong-shape claim → skip + deny `groups_claim_invalid` (fail-closed);
  non-string array entries ignored; last-admin removal refused with deny
  `last_admin_kept`. Claim name read via `extra.get(&oidc.groups_claim)` —
  never a hardcoded literal.
- Placement: best-effort call on the single session-mint success tail (all three
  identity paths — returning, auto-link, provision — converge there per 17-03),
  after cookie construction, before response; errors `eprintln!`'d, never
  propagated (D-04 precedent).
- MockIdp gained `set_access_token` (served verbatim at /token) for the D-15
  smuggling test.
- 11 `group_sync_*` tests, all through the full HTTP callback against the mock
  IdP. TDD RED→GREEN.

## Verification (all gates)

- `cargo test --lib`: **472 passed, 0 failed** (458 baseline + 14 new);
  `sso_reg_*` and all 17-02/17-03 `sso_*` tests pass UNEDITED (empty default
  `group_map` → sync is a no-op for them).
- `cargo clippy --all-targets -- -D warnings`: clean (one `manual_contains`
  nit fixed).
- `cargo fmt --check`: clean.
- `git diff --exit-code Cargo.toml Cargo.lock`: clean — zero new dependencies.
- Grep gates: `async fn sync_groups` in oidc.rs ✓, `group_memberships_for_user`
  in store.rs ✓, claim name configurable via `oidc.groups_claim` ✓.
- Process: no `--no-verify`, nothing pushed, `.planning/STATE.md` /
  `.planning/ROADMAP.md` untouched.

## Deviations / notes

- Plan sketched claim extraction from `CoreIdTokenClaims.extra()`; the Core type
  carries `EmptyStruct` extras, so a custom claims type was required — the
  flatten map lives on `OidcExtraClaims` and `exchange_and_verify` returns it
  alongside the identity. Semantics identical to the plan's intent.
- Plan listed three call sites (returning / auto-link / provision); 17-03
  converged all success paths on one session-mint tail, so one call site covers
  all three.
- `sync_groups` takes the extra-claims map instead of `&CoreIdTokenClaims`
  (same data, decoupled from the openidconnect type).
- Threats T-17-16..19 each have their designated test, all green.
