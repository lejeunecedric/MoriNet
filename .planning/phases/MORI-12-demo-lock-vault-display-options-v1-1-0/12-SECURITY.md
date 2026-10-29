---
phase: 12
slug: demo-lock-vault-display-options-v1-1-0
status: verified
threats_open: 0
asvs_level: 1
created: 2026-10-29
---

# Phase 12 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| unauthenticated/locked user → vault mutation routes | The demo account (or anyone using its session) crosses into load/delete/edit/share mutations | session claims → authorization decision |
| config/env → RuntimeConfig | Operator-supplied locked-account list crosses into an authorization decision | `MORINET_LOCKED_ACCOUNTS` / `[security]` file list |
| server-rendered HTML → browser JS | The locked flag and alert string cross into the page | static strings only |
| owner form POST → vaults row | Free-form theme/checkbox values cross into persisted per-vault state | theme token, keep-page-numbers bool |
| migration → existing DBs | ALTER TABLE runs against live SQLite data at boot | schema-only, additive |
| vault HTML on disk → served bytes | Attacker-influenced book content (GitHub-sourced vaults) passes through the rewrite | page bytes |
| stored theme value → HTML rewrite | A DB value is substituted into served page bytes | whitelist-bounded token |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-12-01 | Tampering | home popup (UX-only) | mitigate | Server-side refusal first statement in `load_personal_vault` (server.rs:2593) and `delete_my_vault` (:2007); 400 via `error_page`; test `locked_account_create_refused` proves scripted POST refused | closed |
| T-12-02 | Elevation of privilege | `is_locked` helper | mitigate | `eq_ignore_ascii_case` compare (server.rs:2057); admin exemption intentional (:2050-2052, D-02); demo seeded reader; test `locked_admin_email_not_locked` | closed |
| T-12-03 | Spoofing | env override | mitigate | Trim+lowercase at resolve (config.rs:148-152); set-but-blank env falls through to `[security]` file list (config.rs:189-197, fix f09d062); tests `resolve_locked_accounts_blank_env_falls_back_to_file_section`, `env_overrides_file_section` | closed |
| T-12-04 | Information disclosure | error_page refusal body | mitigate | Fixed static `LOCK_MSG` html-escaped by `error_page` (server.rs:2852-2862); no vault data, no enumeration | closed |
| T-12-05 | Denial of service | `load_personal_vault` gate | mitigate | `is_locked` check precedes quota (:2613) and GitSource sync/build (:2655) — refused load never touches network/disk | closed |
| T-12-06 | Tampering | theme form value | mitigate | Server-side whitelist `""|light|navy|rust|coal|ayu` with REJECT 400, not coercion (server.rs:2164-2166); test `edit_submit_rejects_bad_theme` | closed |
| T-12-07 | Tampering | edit form CSRF/impersonation | mitigate | Persistence strictly after `can_edit_vault` (:2156) AND `is_locked` (:2159) gates; first write at :2172 | closed |
| T-12-08 | Denial of service | migration 0009 | mitigate | Exactly two additive `ADD COLUMN ... NOT NULL DEFAULT` statements (0009_vault_display_options.sql); test `display_options_migration_shape` | closed |
| T-12-09 | Information disclosure | theme value in HTML | accept | Owner's own choice, whitelist-bounded at write AND serve time — see Accepted Risks | closed |
| T-12-10 | Tampering / XSS | serve-time theme rewrite | mitigate | Fixed-literal substitution only (server.rs:642-648); whitelist re-checked at serve time `THEME_WHITELIST` (:639, f09d062 defense-in-depth); test `display_options_non_whitelist_theme_is_ignored` proves injection payload inert | closed |
| T-12-11 | Tampering | visitor page content | mitigate | Exact full-assignment-string match (:620-621); test `display_options_theme_rewrite_inert_when_absent` — page without literals byte-identical | closed |
| T-12-12 | XSS | injected `<style>` snippet | mitigate | Static raw-string const `PAGE_NUMBERS_OVERRIDE` (:616-617), zero interpolation, inserted verbatim | closed |
| T-12-13 | Denial of service | rewrite on large pages | mitigate | `str::contains` guards before any replace (:640, :645); non-UTF-8 returned untouched (:664-666); no regex | closed |
| T-12-14 | Elevation of privilege | option pass scope | accept | Serve authorization unchanged (`access_for`/`can_read`, server.rs:692-699) — see Accepted Risks | closed |
| T-12-SC | Tampering | dependencies | accept | No new dependencies in phase-12 range (`git diff bec1d7f..HEAD -- Cargo.toml Cargo.lock` empty) | closed |

*Status: open · closed*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| A-12-01 | T-12-09 | Theme is the owner's own choice rendered only on their vault's pages; whitelist bounds it to six fixed tokens at write time and again at serve time — residual disclosure surface nil | plan-time register (all 3 plans) | 2026-10-28 |
| A-12-02 | T-12-14 | Display options apply only to pages served from vaults the visitor already passes `access_for`/`can_read` on; authorization call sites unchanged (verified via diff bec1d7f..HEAD) | plan-time register (12-03) | 2026-10-28 |
| A-12-03 | T-12-SC | No new dependencies introduced in any phase-12 plan (verified: Cargo.toml/Cargo.lock untouched in range) | plan-time register (all 3 plans) | 2026-10-28 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-29 | 15 | 15 | 0 | gsd-security-auditor (verify-only, ASVS L1, block-on-high) vs HEAD 1051cf9 |

Audit method: each mitigation verified mechanically against current code (file:line evidence in register above), not against plan/summary claims. Targeted test runs on the audited tree: `locked_` 13 passed, `display_options` 9 passed, `theme` 9 passed, 0 failures. Post-review hardening commit `f09d062` (blank-env fail-open fix + serve-time theme whitelist) included in scope.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-29
