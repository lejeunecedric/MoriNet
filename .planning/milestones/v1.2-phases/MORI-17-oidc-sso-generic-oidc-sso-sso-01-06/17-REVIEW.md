---
phase: 17
name: oidc-sso-generic-oidc-sso
review: source diff bb66e2d..bd3d673 (waves 1–5 + fixes)
date: 2026-11-06
verdict: APPROVE — no CRITICAL/HIGH findings; 1 MEDIUM, 5 LOW, 5 INFO
counts:
  critical: 0
  high: 0
  medium: 1
  low: 5
  info: 5
verification:
  - "cargo test --lib: 491 passed / 0 failed / 4 ignored (re-run at review time)"
  - "openidconnect 4.1.0 crate source audited: iss_required defaults true (verification/mod.rs:156), iss/aud/exp/nonce enforced inside id_token.claims()"
---

# Phase 17 code review — generic OIDC SSO

Scope: `git diff bb66e2d..bd3d673 -- src migrations Cargo.toml` (waves 1–5 + fixes).
Reviewed against 17-CONTEXT.md D-01..D-15.

## Verdict

**APPROVE.** The security-critical spine is sound: fail-closed ordering in the
callback, constant-time state compare, `(iss,sub)` authority, admin never
auto-linked, exact-domain provisioning, managed-set group sync, all-or-nothing
config, additive migration, escaped login-page injection point, and byte-identical
disabled behavior — all verified in code and backed by the A1/A2/A5/A6/A7 matrix.
No CRITICAL or HIGH findings. One MEDIUM (unauthenticated audit-DB write path),
five LOW hardening items, five INFO notes. None block shipping.

## What was verified clean (focus areas)

1. **`verify_session` discriminator (a296562) — airtight.** The state trio
   payload `state\nnonce\nverifier` (`src/oidc.rs:155`) always contains two
   `\n` separators; the only production `codec.sign` call sites are the trio
   (`oidc.rs:158`), the SSO mint (`oidc.rs:257`) and `login_submit`
   (`server.rs:469`) — no path can mint a state token without `\n`. Session
   emails are b64url-encoded inside the token, so a `\n` in an email never
   reaches cookie-header bytes (no header injection, no `parse().unwrap()`
   panic). Both session gates (`require_session` `server.rs:495`,
   `require_admin` `server.rs:414`) switched to `verify_session`; the only
   remaining plain `codec.verify` on cookie input is the intentional state
   validation at `oidc.rs:202` (a session token fed there fails the 3-part
   split). WebDAV (`require_dav_auth`) and `/api/v1`/MCP never read the
   session cookie. `matrix_a6_no_preauth_identifier` (oidc.rs:3078) drives
   exactly this attack against `/` and `/admin`.
2. **Callback fail-closed ordering.** State cookie verify + constant-time
   state compare (`oidc.rs:196–234`, `subtle::ct_eq` at :224) run BEFORE the
   token exchange (:238); nonce/iss/aud/exp are enforced by the library inside
   `id_token.claims()` (crate `verification/mod.rs`: `iss_required: true`
   default :156, iss match :319, aud match :340) BEFORE any DB write
   (`resolve_identity` :250). `morinet_sso_state` is cleared on every path —
   idp_error :193, state-invalid :216/:231, exchange failure :243, resolution
   refusal :288, success :267. `auth_events` rows carry only kind/actor/iss/
   sub/short-detail (CHECK-locked vocab); no token/code column exists and no
   call site passes one.
3. **Group sync.** Managed set = `group_map` VALUES only (`oidc.rs:563`),
   never auto-created (:571 unknown target → skip + deny); claim absent →
   early return (:549); present-empty → remove all managed; wrong shape →
   skip + deny `groups_claim_invalid` (:551–558); claim read only from the
   flatten map of the signature-verified ID token (`OidcExtraClaims`,
   :38–43, :513) — access-token smuggling test exists
   (`group_sync_reads_only_id_token`, `matrix_a5_access_token_claim_ignored`).
   Last-admin guard present (:617) but see REV-17-04.
4. **config `[oidc]`.** All-or-nothing resolve (`config.rs` `resolve_oidc`):
   partial set → `None` + one warning naming only the MISSING KEYS (never
   values — secret never logged); env-wins per field via `env_nonblank`
   (blank env = unset, Phase 12 lesson); domains normalized like
   `locked_accounts`. Six tests cover each branch.
5. **Migration 0012.** Pure additive `CREATE TABLE IF NOT EXISTS` ×2 + one
   index; `UNIQUE(provider, iss, sub)` and `CHECK(kind IN (...))` match D-11/
   D-05; FK `ON DELETE CASCADE` to users; no destructive ops.
6. **Login page.** `login_html` returns `LOGIN_HTML.to_string()` byte-for-byte
   when disabled (test `matrix_a7_sso_disabled_routes_404` asserts equality);
   `sso_label` passes through `html_escape` (`server.rs:1139`, escapes
   `& < > "`) inside element text — test `login_page_sso_label_configurable_and_escaped`.
   Account-page `iss` is escaped too (`server.rs:1262`).
7. **Mint parity (D-02).** Callback mint (`oidc.rs:255–260`) is the same
   `codec.sign(email, now, DEFAULT_TTL_SECS)` + identical cookie attribute
   string + `Redirect::to("/")` as `login_submit`; test
   `sso_callback_cookie_byte_identical_to_password_login` (oidc.rs:1354)
   compares the cookie shape; `DEFAULT_TTL_SECS` appears exactly once in
   oidc.rs (single mint site).

## Findings

### REV-17-01 — MEDIUM — unauthenticated writes to `auth_events` from `/sso/callback`

**Where:** `src/oidc.rs:190–195` (`idp_error` deny logged before ANY state
check), `src/oidc.rs:211–234` (`state_invalid` deny on every cookie-less hit),
`src/store.rs:1543` (`log_auth_event` INSERT).

**Why it matters:** an unauthenticated attacker can hammer
`GET /sso/callback?error=x` (or any request without a state cookie) and force
one SQLite INSERT per request. `prune_auth_events` runs only at boot
(`main.rs:153`), so between restarts the table grows unbounded — a disk/DB
bloat vector and an audit-pollution channel that can bury genuine deny events
(D-05's audit is the phase's own safety property). Password login's failure
path writes nothing comparable.

**Suggested fix:** log the deny only after the state cookie has at least
parsed/verified (drop the pre-state `idp_error`/`state_invalid` inserts, or
rate-limit them — e.g. one row per IP per minute via the existing
`api_rate`-style map); optionally also prune `auth_events` periodically, not
only at boot.

### REV-17-02 — LOW — IdP discovery (metadata + JWKS fetch) on every SSO request, uncached and unthrottled

**Where:** `src/oidc.rs:126` (`sso_login`) and `src/oidc.rs:480`
(`exchange_and_verify`) — each performs `discover_async` (metadata + JWKS
round-trips) per request.

**Why it matters:** `/sso/login` is public and unauthenticated; each hit
triggers outbound HTTPS to the IdP. An attacker can use MoriNet as a relay to
hammer the corporate IdP (and exhaust local sockets/latency), and legitimate
logins pay two discovery round-trips.

**Suggested fix:** cache `CoreProviderMetadata` (and the built client) keyed
by issuer with a modest TTL (e.g. 1 h, refresh on `token_invalid`), and/or
rate-limit `/sso/login` per IP.

### REV-17-03 — LOW — email claim not checked for control characters; a `\n`-bearing claim provisions an unusable account

**Where:** `src/oidc.rs:374–379` (normalize = trim + lowercase only),
`src/oidc.rs:329–339` (`domain_allowed` accepts `a\nb@corp.example` —
`trim()` only strips edges), `src/oidc.rs:431–456` (provision + mint).

**Why it matters:** a permissive IdP that lets users edit their email can
deliver `a\nb@corp.example`; the domain check passes, the account is
provisioned, and the session minted for it is then rejected by
`verify_session` (`session.rs:56`) — a silently broken account plus a junk
row in `users` (and repeated failed logins re-auditing). Fail-closed (no
security bypass — the token is b64url-encoded so no header injection), but
confusing and self-DoS-ing. Related: `user_by_email_normalized`
(`store.rs:1596`) uses `fetch_optional`, which errors on case-duplicate
accounts and is swallowed as "no match" by `.ok().flatten()`.

**Suggested fix:** in `resolve_identity`, refuse any email containing control
characters (`c.is_control()`) or non-ASCII-without-IDN rules before link/
provision; deny `no_email`-style.

### REV-17-04 — LOW — last-admin group guard is a check-then-act race (no transaction)

**Where:** `src/oidc.rs:617` — `group_members(*id).await?.len() <= 1` then
`remove_group_member` at :624 are separate statements on separate pool
connections.

**Why it matters:** two concurrent SSO logins whose claims both drop admin
membership can each observe `len == 2` and jointly empty the `admin` group —
exactly what the guard exists to prevent. Blast radius is limited (the plan
group, not `users.role`; boot-time `ensure_fallback_admin` remains), hence
LOW.

**Suggested fix:** perform count+delete in one statement/transaction, e.g.
`DELETE FROM group_members WHERE group_id=? AND user_id=? AND
(SELECT COUNT(*) FROM group_members WHERE group_id=?) > 1` and treat
`rows_affected()==0` as `last_admin_kept`.

### REV-17-05 — LOW — `link_identity` store error is conflated with losing the link race

**Where:** `src/oidc.rs:401–416` — `.unwrap_or(false)` maps a DB error to
"lost the race"; if the follow-up re-read also fails (`.ok().flatten()` →
`None`), the code falls through to logging a `link`/`auto_link` audit row and
signing in although no identity row was persisted.

**Why it matters:** the audit trail (D-05) can claim a link that does not
exist, and the operator's `user_identities` table silently lacks the binding
the UI later implies. Sign-in itself is still gated on verified email +
non-admin, so this is audit integrity, not auth bypass.

**Suggested fix:** distinguish `Err` from `Ok(false)`: on `Err`, refuse
(`provision_failed`-style deny) instead of proceeding; only the true
`Ok(false)` race should re-read.

### REV-17-06 — LOW — `sync_groups` aborts mid-diff when an audit insert fails

**Where:** `src/oidc.rs:606–628` — membership mutations and their
`log_auth_event(...).await?` share the `?`; a logging failure after some
`add_group_member` calls leaves applied-but-unaudited changes and skips the
remaining removals.

**Why it matters:** contradicts the best-effort-audit convention used
everywhere else in the phase (`let _ = log_auth_event(...)`), and a partial
sync is the one state D-09's replace-on-login is meant to avoid.

**Suggested fix:** swallow audit errors (`let _ =`) inside the diff loop and
log a warning, keeping the diff itself atomic w.r.t. membership operations.

### REV-17-07 — INFO — `identity_for_user` `LIMIT 1` without ORDER BY

**Where:** `src/store.rs:1490–1495`. Multiple `(iss,sub)` rows can legitimately
link to one user (same verified email from different subs); the D-13 indicator
then shows an arbitrary row. Harmless in single-IdP v1; add `ORDER BY linked_at`
when multi-IdP lands.

### REV-17-08 — INFO — missing secondary indexes

**Where:** `migrations/0012_user_identities.sql` — no index on
`user_identities.user_id` (every account-page render scans via
`identity_for_user`); `auth_events` has only the `ts` index (`count_auth_events`
by kind+detail is test-only, fine). Add `CREATE INDEX ... ON user_identities(user_id)`
in a future additive migration if the account page gets hot.

### REV-17-09 — INFO — status-code semantics on SSO setup errors

**Where:** `src/oidc.rs:124` (invalid configured issuer → 404 "SSO is not
configured.") and `src/oidc.rs:128` (IdP unreachable → 400 via `error_page`).
A misconfigured issuer is indistinguishable from disabled for operators, and
"provider unreachable" is semantically 502/503. Cosmetic; the fail-closed
behavior itself is correct.

### REV-17-10 — INFO — `base_url` trailing slash yields `//sso/callback` redirect_uri

**Where:** `src/oidc.rs:136` — `format!("{}/sso/callback", base_url)`. A
trailing-slash `base_url` produces a redirect URI that will not match the
IdP-registered callback. Normalize (trim trailing '/') or validate at resolve
time.

### REV-17-11 — INFO — `groups_claim` set to a registered claim name silently disables sync

**Where:** `src/oidc.rs:548` reads from the unregistered-claims flatten map
only. If an operator sets `groups_claim = "email"` (or `"sub"`), the library
consumes that claim and sync sees "absent" → keep, with no warning. Consider
warning at config resolve when `groups_claim` collides with a registered OIDC
claim name.

## Notes (non-findings)

- Auto-link on IdP-asserted `email_verified=true` (D-12) is a deliberate trust
  decision — the IdP is the trust boundary; admin accounts are excluded and
  every link is audited. The A1 matrix covers the email-change and
  attacker-email variants.
- Committed `src/testdata/sso_test_key.pem` / `sso_wrong_key.pem` are declared
  neutral fixtures; if a secret scanner runs in CI, allowlist them explicitly
  so future scans keep meaning something.
- Nonce comparison inside the library is a plain equality, not constant-time —
  acceptable: the nonce is checked only against a signature-verified token,
  so there is no attacker-iterable oracle.
- Review-time gate re-run: `cargo test --lib` → **491 passed, 0 failed,
  4 ignored**.
