---
phase: 16
slug: rest-api-read-only-rest-api-on-api-v1-api-01-06
status: verified
threats_open: 0
asvs_level: 1
created: 2026-11-01
---

# Phase 16 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Verified against HEAD `e3e00aa` (code state `4c5f4b8` — includes review fixes:
> problem+json 405 method guards, saturating rate-limiter math, `&vault=` index).

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Client → `/api/v1` | Untrusted HTTP client authenticating with Bearer API token | Vault note content (confidential), token plaintext |
| `/api/v1` → Store | SQL access via bound params only | User/vault/token records |
| `/api/v1` → filesystem | Note reads restricted to mdBook **source** dirs via `safe_join` | Raw markdown bytes |

## Threat Register (22 threats, from plan threat_model blocks)

| ID | Threat | STRIDE | Sev | Status | Evidence |
|----|--------|--------|-----|--------|----------|
| T-16-01 | ACL bypass via helper drift | Spoofing/Info Disclosure | high | CLOSED | Helpers sole path `api.rs:75–146`; mcp.rs thin wrappers; `api_tools_are_acl_scoped` + `tools_are_acl_scoped` green |
| T-16-02 | Existence leak via error differentiation | Info Disclosure | med | CLOSED | Single NotFound for unknown slug AND no-access; `helper_source_dir_invisible_vault_is_none` |
| T-16-03 | Traversal via path/sub params | Tampering | med | CLOSED | `safe_join` only (component-whitelist refusal — equal-or-stronger than plan wording); `api_read_note_refuses_traversal_even_with_access` |
| T-16-04 | Serving build output | Info Disclosure | low | CLOSED | Dir only via `webdav::source_dir_of`; `api_note_read_serves_raw_source` |
| T-16-05 | Cookie/Bearer mixing | Spoofing | high | CLOSED | api router sibling merge outside `protected`; middleware reads only AUTHORIZATION; `api_ignores_session_cookie` |
| T-16-06 | Token leakage in logs/errors | Info Disclosure | high | CLOSED | Zero logging calls in api.rs (grep gate); fixed 401 detail; `api_requires_bearer` |
| T-16-07 | Error-shape divergence (403-vs-404) | Info Disclosure | med | CLOSED | One `problem()` ctor for 401/404/405/429; `api_unknown_path_is_problem_not_found`, `api_invisible_vault_404_not_403`, `api_method_mismatch_is_problem_405` |
| T-16-08 | Unauthenticated 401 flood | DoS | low | ACCEPTED | One indexed lookup on UNIQUE token_hash; no map insert pre-auth; documented |
| T-16-09 | Wildcard `{*path}` traversal | Tampering | high | CLOSED | Route → `read_note` → `safe_join`; `api_note_read_refuses_traversal` (`../` + `%2e%2e%2f`) |
| T-16-10 | Build output via note read | Info Disclosure | high | CLOSED | Decoy test plants `book/note.md` leak marker; body == source bytes |
| T-16-11 | Status differentiation on path reads | Info Disclosure | med | CLOSED | NotFound/NoSuchNote/InvalidPath/NoSuchFolder → one 404 shape |
| T-16-12 | DoS via limit/cursor | DoS | med | CLOSED | Clamp 200, search cap 50, `partition_point` skip; `api_notes_limit_default_and_cap` |
| T-16-13 | If-None-Match cache poisoning | Tampering | med | CLOSED | Phase-15 `if_none_match_of` reused verbatim; Malformed → 400 before body (fail-closed) |
| T-16-14 | Unbounded rate-limiter map | DoS | high | CLOSED | Insert only post-auth; retain-prune each check; `rate_limiter_prunes_expired_windows` |
| T-16-15 | Token as map key / in logs | Info Disclosure | high | CLOSED | Key = SHA-256 hex, plaintext dropped; `rate_limiter_key_is_token_hash_not_plaintext` |
| T-16-16 | Limiter lock held across await | Tampering | med | CLOSED | `api_rate_check` fully sync, lock scoped; called before `next.run().await` |
| T-16-17 | Rate-limit evasion via token rotation | DoS | low | ACCEPTED | Per-token budget is the locked D-13 requirement; users mint their own tokens by design |
| T-16-18 | Clock source / skew | Tampering | low | ACCEPTED (hardened) | Same `unix_now()` as session expiry; post-plan saturating_sub (clock-rewind safe) — `rate_limiter_clock_rewind_safe` |
| T-16-19 | ACL matrix port fidelity | Spoofing | high | CLOSED | 6 ports green, each asserting 404-not-403 + problem+json; 3 logging ports documented-skipped |
| T-16-20 | Docs page data leak / enumeration | Info Disclosure | med | CLOSED | Static raw string, zero interpolation; `api_docs_page_no_vault_data` byte-identical |
| T-16-21 | Public docs flood | DoS | low | ACCEPTED | Static string, no I/O/DB |
| T-16-22 | XSS via docs page | Tampering | low | CLOSED | No user-controlled interpolation; app_page chrome only |

## Accepted Risks (review, 2026-11-01)

- No `Cache-Control: private` on authenticated responses — single-binary deployment, no shared caches (revisit if a CDN is fronted).
- MCP search double-queries `accessible_vault_entries` for logging — perf only.
- Case-sensitive `Bearer ` prefix — deliberate consistency with `/mcp`.
- Fixed-window boundary burst — inherent to locked D-13.
- REST access logging skipped in v1 — `access_log.source` CHECK constraint; no requirement depends on it.

## Audit Trail

- 2026-11-01 — Plans 16-01..05 carried threat_model blocks at plan time (security_enforcement on, ASVS L1, block-on-high).
- 2026-11-01 — Code review `16-REVIEW.md`: 0 CRITICAL / 0 HIGH; M-01/M-02/L-01 fixed in `4c5f4b8` (TDD), L-02/L-03/L-04 accepted (`16-REVIEW-FIX.md`).
- 2026-11-01 — Verify-only security audit against HEAD `e3e00aa`: 22/22 verified (18 CLOSED, 4 ACCEPTED, 0 OPEN). Evidence: `cargo test --lib api::` 44/44, `mcp::` 12/12, grep spot-checks B1–B4 all pass.

**Gate: threats_open: 0 — PASS**
