1|## v1.4 Statistics & Observability (Shipped: 2026-11-20)
2|
3|**Phases completed:** 2 phases, 10 plans, 21 tasks
4|
5|**Key accomplishments:**
6|
7|- Migration 0014 rebuilds access_log with CHECK (source IN ('web','mcp','api')) preserving every row and both indexes, adds mcp_activity + search_queries tables, and ships the seven store accessors plus 90-day boot prunes every later Phase-19 plan builds on.
8|- MCP surface instrumented: `truncate_chars` (UTF-8-safe 200-char cap) plus spawn-based `log_activity`/`log_search_query` wired into the three vault-scoped tool wrappers — one `mcp_activity` row per ACL-passing call and one raw `search_queries` row per searched vault; denied calls write nothing.
9|- REST API instrumented: `log_api_access` (spawn best-effort, source='api') fires on every successful `api_read_note` (raw AND `?meta=1`), and `log_api_search` fans out one `access_log` row (path `-`) + one raw `search_queries` row per searched vault — closing the deliberate Phase 16 gap and retiring its stale comment.
10|- The Statistics page now answers "what did people read, ask, and find?": 7/30/90-day chips (default 90 = retention ceiling) drive every section, and two new owner/admin-only sections — "MCP activity" (two-column request↔outcome, newest-first) and "Search keywords" (top queries + top words) — render from the 19-01 store accessors with zero new CSS.
11|- Six built-in docs pages now carry OKF v0.2 frontmatter (type/title/description/generated, no trust fields), a first-party `morinet mdbook-okf-preprocessor` subcommand strips it at render time on both mdbook build paths, and the docs-seed marker is versioned to "2" so every existing install re-seeds once at next boot.
12|- The built-in docs vault passes both official OKF validators at the D-04 bar — okflint 0.5.1 exit 0 and okforest 0.14.0 JSON with 0 errors / 0 warnings across 6 concepts — and the clean runs are durably recorded (versions, exact commands, full outputs, validated commit SHA) in 20-EVIDENCE.md.
13|- The local quality gate now runs the official OKF validators (okflint 0.5.1, okforest 0.14.0) against `docs/morinet-docs` whenever the operator has them in PATH — failing on any error or warning (D-04) and printing an explicit skip note when a validator is absent — via a standalone `scripts/check-okf.sh` wired into `scripts/check.sh`, with no GitHub CI and no auto-install path.
14|- The official OKF static viewer (template + viz.css + viz.js) is vendored from `open-knowledge-format/open-knowledge-format` at pinned commit `527e81aff55992be21507e97004286789126f88c`, together with the exact CDN libraries it references (cytoscape@3.28.1, marked@12.0.0), under `assets/okf-viz/` with Apache-2.0 headers intact and a PROVENANCE.md license + sha256 integrity record — the complete offline-render-capable asset set plan 20-05 needs.
15|- Every exported OKF bundle now contains a root `viz.html` — the official OKF graph viewer, fully self-contained (cytoscape@3.28.1 + marked@12.0.0 inlined, bundle graph injected as `window.BUNDLE`), synthesized in-memory only when the vault ships no viz.html of its own, with T-18-10 caps counting its bytes.
16|- README publishes a static OKF v0.2 conformance badge with a scope-honest claim backed by 20-EVIDENCE.md — docs vault certified clean (0/0), demo-vault validation marked in-progress per the user's checkpoint decision.

**Stats:** 87 commits (`v1.4.0`→HEAD at archive), 78 files, +7,905/−138 LOC, timeline 2026-11-06 → 2026-11-20. Test suite 554 → 611 green. Milestone audit PASSED (7/7 requirements, 11/11 integration, 3/3 E2E flows) — milestones/v1.4-MILESTONE-AUDIT.md. Both v1.2-deferred human UATs (rclone WebDAV, real-IdP SSO) resolved with real clients 2026-11-20.
17|
18|---
19|
20|1|# Milestones
21|2|
22|3|## v1.2 Team Access (Shipped: 2026-11-06, released as v1.2.0 → v1.3.0)
23|4|
24|5|**Phases completed:** 3 phases, 15 plans, 17/17 requirements (CONF-01..05, API-01..06, SSO-01..06)
25|6|
26|7|**Key accomplishments:**
27|8|
28|9|- Conflict-safe co-editing: strong sha256 ETags + fail-closed `If-Match`/`If-None-Match` on WebDAV, atomic writes under per-vault locks, upload conflict list (409) with audited `force=1` overwrite, "Load local" two-phase UX (Phase 15, migration 0011)
29|10|- Read-only REST API on `/api/v1`: Bearer API tokens (session cookies rejected), full MCP ACL parity via shared helpers, RFC 9457 problem+json, cursor pagination, search, raw-markdown reads with ETag/304, per-token rate limiting, public `/api/v1/docs` (Phase 16, no new deps)
30|11|- Generic OIDC SSO: auth-code + PKCE S256, JWKS validation, `[oidc]` config env-wins disabled-by-default, `user_identities` + `auth_events` (migration 0012), verified-email auto-link, domain-gated reader provisioning, replace-on-login group sync with last-admin guard, byte-identical session mint with password login (Phase 17, only new dep `openidconnect` rustls-only)
31|12|- Security: 67/67 threat blocks closed across the three phase audits; real session-confusion gap (A6) caught RED-first and fixed via `verify_session` multiline rejection
32|13|
33|14|**Stats:** 134 commits (`v1.1.0`→`v1.3.0`), 16 source files, +10,013/−201 LOC, timeline 2026-10-29 → 2026-11-06. Test suite 334 → 493 green. Releases v1.2.0/v1.2.1/v1.2.2/v1.3.0 all deployed via GitHub Actions.
34|15|
35|16|**Known deferred items at close:** 2 human operator UATs (rclone/Obsidian WebDAV smoke, real-IdP SSO smoke), G1-G3 composed E2E test gaps, accepted LOW debt (JWKS uncached, no Secure cookie attr, no Cache-Control on API) — see milestones/v1.2-MILESTONE-AUDIT.md `tech_debt`; Nyquist VALIDATION.md absent for phases 15-17 (goal-backward verification used instead).
36|17|
37|18|---
38|19|
39|20|## v1.1.0 demo hardening + vault options (Shipped: 2026-10-29)
40|21|
41|22|**Phases completed:** 3 phases, 6 plans, 10 tasks
42|23|
43|24|**Key accomplishments:**
44|25|
45|26|- Locked-account guard: server-side refusal of load/upload/delete/edit/share for demo accounts + home-page popup (REQ-DEMO-001/002)
46|27|- Per-vault display options: keep-page-numbers toggle + mdBook theme select, applied at serve time with no rebuild (REQ-VOPT-001/002)
47|28|- Wiki-changes log: `vault_changes` table, git tree-diff capture at Refresh + WebDAV PUT capture, best-effort, 90-day prune (REQ-CHG-001)
48|29|- Statistics "Wiki changes" section: newest-first, grouped per refresh event (REQ-CHG-002)
49|30|- Local vault upload: browser directory picker → sanitized multipart endpoint, quota/lock gates, shared convert/build/register pipeline (REQ-LOC-001/002)
50|31|- Issue #7 (duplicate sidebar chapter numbers) closed with the 1.1.0 deploy
51|32|
52|33|**Stats:** 43 commits (`f5711bd`→`3b1e962`), 9 source files, +2,577/−220 LOC (Rust), timeline 2026-10-28 → 2026-10-29. Full suite 334 green; `scripts/check.sh` clean. Tag `v1.1.0` created by `scripts/release.sh` (GitHub release + live deploy on holldemo).
53|34|
54|35|**Known deferred items at close:** 8 tech-debt items — 6 LOW review findings + 2 live-browser human checks (see milestones/v1.1.0-MILESTONE-AUDIT.md `tech_debt`); Nyquist VALIDATION.md absent for phases 12–14 (goal-backward verification used instead).
55|36|
56|37|---