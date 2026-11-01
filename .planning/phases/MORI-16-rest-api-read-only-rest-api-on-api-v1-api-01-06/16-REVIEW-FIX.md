---
phase: 16
slug: rest-api-read-only-rest-api-on-api-v1-api-01-06
name: review-fix-report
status: complete
created: 2026-11-01
review: 16-REVIEW.md
findings_fixed: [M-01, M-02, L-01]
findings_accepted: [L-02, L-03, L-04]
---

# Phase 16 Review Fix Report

All fixes applied inline by the orchestrator (no CRITICAL/HIGH findings — MEDIUM/LOW-only
per protocol). TDD: one regression test per fix, observed RED, then GREEN.

## Fixed

### M-01 — JSON index advertised `&slug=` instead of `&vault=`
- **Fix:** `src/api.rs` index string now advertises `optional &vault=` (the param the code
  actually reads). Regression test `api_index_advertises_vault_param_not_slug`.
- RED: assertion failed at api.rs:1767 → GREEN.

### M-02 — method mismatch answered axum's bare 405, not problem+json (D-03 gap)
- **Fix:** all five API routes registered with `any()` and dispatched through
  `is_get_or_head` guards; mismatch → `problem(405, "method_not_allowed", ...)` with
  `Allow: GET, HEAD` per RFC 9110. Vocabulary extended with `method_not_allowed`
  (problem() doc comment + docs page updated). HEAD accepted, mirroring axum `get()`.
  Regression test `api_method_mismatch_is_problem_405`.
- RED: content-type assertion failed at api.rs:1790 (bare 405) → GREEN.

### L-01 — clock-rewind underflow panic in `api_rate_check`
- **Fix:** `now.saturating_sub(*start)` in the window-reset comparison and the
  retry-after computation (`.max(1)` keeps Retry-After ≥ 1).
  Regression test `rate_limiter_clock_rewind_safe`.
- RED: `panicked at src/api.rs:219` (u64 subtraction overflow) → GREEN.

## Accepted (with rationale)

- **L-02** no `Cache-Control: private`/`Vary` on authenticated responses — accepted:
  single-binary deployment behind no shared caches; revisit if a CDN is ever fronted.
- **L-03** MCP search wrapper double-queries `accessible_vault_entries` for logging —
  accepted: MCP-path-only perf nit, no correctness or security impact.
- **L-04** case-sensitive `Bearer ` prefix — accepted: deliberate consistency with the
  existing `/mcp` gate; changing one without the other would split behavior.
- INFO items (unreachable 400 arms, fixed-window boundary burst inherent to D-13,
  unthrottled 401 path documented in threat model, store-error→`200 []` inherited,
  intentional REST logging gap) — no action.

## Gate

- Full suite: **406 passed / 0 failed / 4 ignored** (was 403; +3 regression tests).
- `cargo clippy --all-targets` clean, `cargo fmt --check` clean.
