---
phase: 21
slug: ui-polish
status: verified
threats_open: 0
asvs_level: 1
created: 2026-11-22
---

# Phase 21 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Vault source → served HTML | Vault filenames/paths (attacker-controllable in a shared/loaded vault) rendered into home-page badge HTML | Untrusted filenames → trusted UI |
| Browser client → mdBook runtime | `localStorage['mdbook-theme']` read by mdBook's `get_theme()` at page load | Client-side cosmetic preference |
| Reader POST → Vault Edit handler | Theme value submitted on `/vault/{slug}/edit` | Owner-controlled form input → persisted vault setting |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-21-01 | Stored XSS (V05) | Non-conformant paths in badge HTML | mitigate | `html_escape` on every path/version (`src/server.rs:553,587`); test `okf_badge_html_escapes_nonconformant_paths` (`:10605`) | closed |
| T-21-02 | DoS / unbounded storage (V06) | `okf_json` blob growth | mitigate | `MAX_MISSES=200` + overflow counter (`src/okf.rs:22,104`) + char-safe `MAX_MISS_PATH_CHARS=200` `clip_miss_path` (`:28-33`, applied `:103`; added post-review at `a38a445`); tests `:751-796` | closed |
| T-21-03 | Parser crash (V05) | Malformed/hostile `okf_json` rows | mitigate | `serde(default)` on new fields (`src/okf.rs:53,56`); parse-fail → empty badge (`src/server.rs:541`); API embeds only parseable JSON (`src/api.rs:331-334`) | closed |
| T-21-04 | n/a | localStorage prefs (plan 01) | not-applicable | No server surface; zoom.js localStorage is client-only | closed |
| T-21-05 | AuthN/AuthZ | Badge data source | not-applicable | Badge renders only inside the `accessible_vaults` loop from stored per-slug `vault_okf` (`src/server.rs:691-721`); no new routes in phase diff | closed |
| T-21-06 | Supply chain (V14) | New Cargo deps (plan 01) | not-applicable | `git diff 989629e..HEAD -- Cargo.toml Cargo.lock` empty | closed |
| T-21-07 | Client tampering (V14) | `localStorage['mdbook-theme']` | mitigate (by design) | Cosmetic, per-reader, same-origin; mdBook falls back on unknown values; handler stops propagation before the write and reloads only on success (`assets/zoom.js:101-108`, fix `10d6e0f`) | closed |
| T-21-08 | XSS via innerHTML (V05) | Looks section DOM injection | mitigate | Hard-coded palette literals only (`assets/zoom.js:88-94`); DOM built via `createElement`/`textContent`; `innerHTML` only with fixed entities | closed |
| T-21-09 | Broken access control (V01) | Server-side per-reader prefs | not-applicable | No handler persists localStorage prefs (D-05: zero server state); theme remains an owner-set vault setting | closed |
| T-21-10 | n/a | Cross-vault preference bleed | not-applicable | localStorage is origin-scoped; shared `mdbook-theme` key is mdBook's own built-in behavior | closed |
| T-21-11 | Supply chain (V14) | New deps (plan 02) | not-applicable | Cargo.toml/Cargo.lock untouched across phase | closed |
| T-21-12 | AuthN/AuthZ | Plan 02 routes | not-applicable | Route-table diff `989629e..HEAD` shows no new routes | closed |
| T-21-13 | Input tampering (V05) | Vault Edit POST theme value | mitigate | `THEMES` whitelist reject-not-coerce (`src/server.rs:2614-2616`), 400 via `error_page` which escapes the echoed value (`:4253`); test `edit_submit_rejects_bad_theme` (`:5005`) | closed |
| T-21-14 | XSS (V05) | Rendered edit page | not-applicable | Markup-only change (`d564f0c` style attr on hard-coded option block, `:2562-2572`); slug/name/url still `html_escape`d (`:2588-2590`) | closed |
| T-21-15 | n/a | localStorage (plan 03) | not-applicable | Docs/markup-only plan; no client state added | closed |
| T-21-16 | Content injection (V05) | Docs vault prose | mitigate | Static committed prose (`docs/morinet-docs/src/sharing-and-roles.md`, commit `987c7fa`) | closed |
| T-21-17 | Supply chain (V14) | New deps (plan 03) | not-applicable | Cargo.toml/Cargo.lock untouched across phase | closed |
| T-21-18 | AuthN/AuthZ | Owner-only edit route | not-applicable | Route pre-existed; `require_session` layer (`src/server.rs:262-265`) + `can_edit_vault` guard (`:2477-2489`, enforced `:2533` GET / `:2606` POST) unchanged | closed |

*Status: open · closed*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|

No accepted risks.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-22 | 18 | 18 | 0 | gsd-security-auditor (verify-only, register authored at plan time) — HEAD `c0c3b2a`, post-plan commits `10d6e0f`/`a38a445`/`d564f0c`/`a57b99e` re-verified against current code |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-22
