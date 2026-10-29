# Milestones

## v1.1.0 demo hardening + vault options (Shipped: 2026-10-29)

**Phases completed:** 3 phases, 6 plans, 10 tasks

**Key accomplishments:**

- Locked-account guard: server-side refusal of load/upload/delete/edit/share for demo accounts + home-page popup (REQ-DEMO-001/002)
- Per-vault display options: keep-page-numbers toggle + mdBook theme select, applied at serve time with no rebuild (REQ-VOPT-001/002)
- Wiki-changes log: `vault_changes` table, git tree-diff capture at Refresh + WebDAV PUT capture, best-effort, 90-day prune (REQ-CHG-001)
- Statistics "Wiki changes" section: newest-first, grouped per refresh event (REQ-CHG-002)
- Local vault upload: browser directory picker → sanitized multipart endpoint, quota/lock gates, shared convert/build/register pipeline (REQ-LOC-001/002)
- Issue #7 (duplicate sidebar chapter numbers) closed with the 1.1.0 deploy

**Stats:** 43 commits (`f5711bd`→`3b1e962`), 9 source files, +2,577/−220 LOC (Rust), timeline 2026-10-28 → 2026-10-29. Full suite 334 green; `scripts/check.sh` clean. Tag `v1.1.0` created by `scripts/release.sh` (GitHub release + live deploy on holldemo).

**Known deferred items at close:** 8 tech-debt items — 6 LOW review findings + 2 live-browser human checks (see milestones/v1.1.0-MILESTONE-AUDIT.md `tech_debt`); Nyquist VALIDATION.md absent for phases 12–14 (goal-backward verification used instead).

---
