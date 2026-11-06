# Milestones

## v1.2 Team Access (Shipped: 2026-11-06, released as v1.2.0 → v1.3.0)

**Phases completed:** 3 phases, 15 plans, 17/17 requirements (CONF-01..05, API-01..06, SSO-01..06)

**Key accomplishments:**

- Conflict-safe co-editing: strong sha256 ETags + fail-closed `If-Match`/`If-None-Match` on WebDAV, atomic writes under per-vault locks, upload conflict list (409) with audited `force=1` overwrite, "Load local" two-phase UX (Phase 15, migration 0011)
- Read-only REST API on `/api/v1`: Bearer API tokens (session cookies rejected), full MCP ACL parity via shared helpers, RFC 9457 problem+json, cursor pagination, search, raw-markdown reads with ETag/304, per-token rate limiting, public `/api/v1/docs` (Phase 16, no new deps)
- Generic OIDC SSO: auth-code + PKCE S256, JWKS validation, `[oidc]` config env-wins disabled-by-default, `user_identities` + `auth_events` (migration 0012), verified-email auto-link, domain-gated reader provisioning, replace-on-login group sync with last-admin guard, byte-identical session mint with password login (Phase 17, only new dep `openidconnect` rustls-only)
- Security: 67/67 threat blocks closed across the three phase audits; real session-confusion gap (A6) caught RED-first and fixed via `verify_session` multiline rejection

**Stats:** 134 commits (`v1.1.0`→`v1.3.0`), 16 source files, +10,013/−201 LOC, timeline 2026-10-29 → 2026-11-06. Test suite 334 → 493 green. Releases v1.2.0/v1.2.1/v1.2.2/v1.3.0 all deployed via GitHub Actions.

**Known deferred items at close:** 2 human operator UATs (rclone/Obsidian WebDAV smoke, real-IdP SSO smoke), G1-G3 composed E2E test gaps, accepted LOW debt (JWKS uncached, no Secure cookie attr, no Cache-Control on API) — see milestones/v1.2-MILESTONE-AUDIT.md `tech_debt`; Nyquist VALIDATION.md absent for phases 15-17 (goal-backward verification used instead).

---

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
