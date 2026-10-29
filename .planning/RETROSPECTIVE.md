# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.1.0 — demo hardening + vault options

**Shipped:** 2026-10-29
**Phases:** 3 | **Plans:** 6 | **Sessions:** ~4 (discuss/plan/exec/audit+close)

### What Was Built
- Locked-account guard (config + env, server-side refusal on all vault mutations, home popup) — demo instance hardened
- Per-vault display options (page-numbers toggle, theme select) applied at serve time — no rebuild needed
- Wiki-changes log (`vault_changes` table, git tree-diff at Refresh + WebDAV PUT) + Statistics section
- Local vault upload (browser directory picker → sanitized multipart endpoint, shared build pipeline)
- Issue #7 (duplicate sidebar numbers) closed

### What Worked
- Goal-backward verification with live test re-runs caught nothing to fix in any of the 3 phases — plans carried exact file:line acceptance evidence
- Code-review → fix loop found 2 real bugs the green test suite missed (upload slug TOCTOU, admin delete path guard) — the review gate earns its cost
- SUMMARY.md chaining: each executor read the prior plan's SUMMARY for signatures instead of re-reading server.rs

### What Was Inefficient
- SUMMARY files carry no frontmatter (`one_liner`, `requirements_completed`) — audit and milestone tooling extracted "Status:" noise and requirements had to be cross-checked manually
- REQUIREMENTS.md checkboxes needed manual flipping (`phase.complete` doesn't update them on this install)
- Release docs (README/docs vault) were left uncommitted after release and only caught at close

### Patterns Established
- Serve-time HTML passes (`inject_vault_chrome` two-pass) as the extension point for per-vault rendering options
- Best-effort side-channel logging (never fail the user's request) — access_log → vault_changes
- Gate order before any disk write: lock → validation → quota → collision pre-check

### Key Lessons
1. Executors must write SUMMARY frontmatter (`one_liner`, `requirements_completed`) or downstream tooling degrades silently
2. Post-review fix commits must be fed to secure-phase auditors — they verify against current HEAD, not plan-time behavior
3. Commit release docs in the release flow, not after

### Cost Observations
- Not tracked per-milestone on this install (Hermes devo profile); notable: full milestone ran in ~2 days wall-clock with parallel review/verify gates

---

## Cross-Milestone Trends

### Process Evolution

- v1.0: pre-GSD + GSD adoption (phases 00–11 mostly pre-GSD)
- v1.1.0: first milestone run end-to-end through the full GSD loop (discuss → plan → execute → review → secure → verify → audit → close)
