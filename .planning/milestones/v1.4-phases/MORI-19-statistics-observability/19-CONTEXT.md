# Phase 19: statistics-observability - Context

**Gathered:** 2026-11-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Every read of a vault is counted on every surface, and the per-vault Statistics page answers "what did people read, ask, and find?":

1. **STAT-01** — access events logged across ALL surfaces: web (exists), MCP (exists, coarse), REST API (missing today); `access_log.source` CHECK widened to include `api` (migration 0014).
2. **STAT-02** — top viewed pages per vault (web source), date-range aware.
3. **STAT-03** — MCP activity: request (tool, args, actor) ↔ outcome (status, latency, size + excerpt), newest first.
4. **STAT-04** — top search keywords aggregated from MCP `search_notes` + REST `search`.
5. **STAT-05** — best-effort bounded logging: never fails the request, 90-day boot prune, full response bodies never stored.

NOT in this phase: WebDAV access analytics, mdBook client-side search capture (impossible server-side), OKF conformance trend, admin cross-vault dashboard.

</domain>

<decisions>
## Implementation Decisions

### MCP activity content (discussed 2026-11-18)
- **D-01:** Each activity row stores BOTH sides: request = tool name + raw args truncated (slug, path, query — acceptable exposure: the viewer can already read the vault per D-05); response = status, latency, response size, and a ~200-character excerpt (UTF-8-boundary-safe truncation). Full bodies NEVER stored (STAT-05 locked).
- **D-02:** Detailed activity covers the 3 vault-scoped tools (`list_notes`, `read_note`, `search_notes`). `list_vaults` keeps today's coarse `access_log` row (`slug="-"`); no detailed row for it.
- **D-03:** Display = new "MCP activity" section on the existing per-vault Statistics page, same shape as the Phase 13 "Wiki changes" section (grouped list, newest first). Project rule: NO new CSS without an Impeccable pass — reuse existing section/table/pill classes only.

### Search keywords (discussed 2026-11-18)
- **D-04:** Store the raw query string per search (MCP `search_notes` + REST `/api/v1/.../search`); display BOTH top queries and top words (words = lowercased whitespace-split tokens, length ≥ 3). Web/mdBook search is client-side → out of scope, no fake capture.

### Visibility (discussed 2026-11-18)
- **D-05:** The detailed sections (MCP activity + search keywords/queries) are visible to the vault **owner + admins only**; existing aggregate sections (activity by day, top paths) keep their current audience. Actor identity shown as email — consistent with the existing per-user counts.

### Date range (discussed 2026-11-18)
- **D-06:** Fixed chips 7 / 30 / 90 days applied to ALL Statistics sections (90 = retention ceiling). No custom from/to picker.

### API surface logging (locked by STAT-01)
- **D-07:** REST note reads and searches append access events with `source='api'`. Widening the SQLite CHECK requires a table rebuild in migration 0014 (create-new/copy/drop/rename) — planner MUST reconcile this with the "additive-only migrations" convention and preserve existing history; the pin test `access_log_rejects_bad_source` (src/store.rs:2332) must be updated to the new vocabulary in the same change.

### Claude's Discretion
- Storage shape for detailed activity (new `mcp_activity` table vs columns on `access_log`) — planner picks, favoring the simplest query story for the Stats page.
- Exact truncation/excerpt mechanics, tokenization details (stopwords or not), chip UI mechanics, and whether log writes are awaited or spawned best-effort (must never fail the request path either way).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase requirements
- `.planning/ROADMAP.md` §"Phase 19: statistics-observability" — goal + 6 success criteria
- `.planning/REQUIREMENTS.md` §"Statistics & observability (issue #10)" — STAT-01..05

### Existing access-log surface (extend, don't reinvent)
- `migrations/0008_shares_stats.sql` — `access_log` shape + `CHECK (source IN ('web','mcp'))` (the constraint 0014 must widen)
- `src/store.rs` — `log_access` (:1423), stats queries activity-by-day/top-paths/per-user (:1444-1467), `prune_access_log` 90-day (:1477), pin test `access_log_rejects_bad_source` (:2332)
- `src/main.rs` — boot prune sequence (:148-153) — new prunes join here, failure must not abort boot

### Surfaces to instrument
- `src/mcp.rs` — `log_access` helper (:201) + the 4 `#[tool]` wrappers (:212-330): where latency/status/excerpt capture goes
- `src/api.rs` — `api_read_note` (:515), `api_search` (:478) handlers; :1676 comment about the CHECK constraint
- `src/server.rs` — web view logging (:2815), Statistics page(s) (:2852, :2987) — sections to extend

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Store::log_access(email, slug, source, path)` — the established best-effort insert; extend or mirror for the new fields.
- Stats queries already in `store.rs` (top paths, activity by day, per-user counts) — top-pages section is mostly wiring.
- "Wiki changes" section (Phase 13) = the UI precedent for a grouped, newest-first list on the Stats page.
- Boot prune convention (`prune_access_log` / `prune_vault_changes` / `prune_auth_events` in main.rs) — new tables follow the same 90-day pattern.

### Established Patterns
- Best-effort side-logging: `let _ = store.log_access(...).await` — failures logged, request unaffected (mcp.rs:207, server.rs:2815).
- ACL: 404-not-403 on all API/MCP surfaces; stats must not leak inaccessible vaults.
- Migrations historically additive-only — the CHECK widening is the first rebuild; pin tests must move with it.
- TDD per plan; `scripts/check.sh` gate; baseline 554 passed.

### Integration Points
- Migration `migrations/0014_*.sql` (CHECK widen + detailed-activity storage).
- `src/api.rs` read/search handlers (new `source='api'` events).
- `src/mcp.rs` tool wrappers (detailed activity capture: latency, status, excerpt).
- `src/server.rs` Statistics page (top pages + MCP activity + keywords sections, owner/admin gating, date-range chips).
- `src/main.rs` boot prune.

</code_context>

<specifics>
## Specific Ideas

- Issue #10 wording is the UX spec: "MCP requests on left and answer on right" → two-column row layout; "statistics of main keywords" → the top-words display.
- Excerpt truncation must be char-boundary safe (UTF-8 vault content is expected — notes are prose).
- Keep the detailed-activity insert on the same code path that already knows tool + args + result (mcp.rs wrappers) — one insert, not a second pass.

</specifics>

<deferred>
## Deferred Ideas

- OKF conformance trend in Statistics (carried from Phase 18 deferral — pairs with this data story, future phase).
- Admin cross-vault analytics dashboard (per-instance overview) — future phase.
- WebDAV PROPFIND/GET analytics as a source — not requested in issue #10; keep out.
- Custom from/to date picker — chips 7/30/90 locked for v1.

</deferred>

---

*Phase: 19-statistics-observability*
*Context gathered: 2026-11-18*
