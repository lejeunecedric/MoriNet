# Phase 16: rest-api — Read-only REST API on /api/v1 - Context

**Gathered:** 2026-10-30
**Status:** Ready for planning

<domain>
## Phase Boundary

Expose the MCP read surface as a JSON REST API on `/api/v1`, authenticated with existing `/account` API tokens (Bearer only) — same ACL, same read helpers, four read endpoints (vaults, notes list, note read, search), cursor pagination, RFC 9457 problem+json errors, per-token rate limiting. `mcp.rs` tools become thin wrappers over extracted helpers so ACL stays single-sourced. Read-only: no write endpoints (API-F01 is future), no scoped tokens (API-F02), no webhooks (API-F03).

</domain>

<decisions>
## Implementation Decisions

### Locked by ROADMAP (constraints, not gray areas)
- **D-01:** Auth is Bearer API token only via `Store::user_for_api_token` (same pattern as `/mcp`, server.rs ~:278-297); session cookies are rejected 401 on `/api/v1` (b4: no cookie/Bearer mixing).
- **D-02:** A vault the user cannot see answers **404 (not 403)** on every endpoint including search and path reads (API-05, b1).
- **D-03:** All errors are RFC 9457 `application/problem+json`.
- **D-04:** New `src/api.rs` router branch; `mcp.rs` tools refactored into thin wrappers over the extracted shared helpers — ACL logic single-sourced (ROADMAP goal).
- **D-05:** No new dependencies (axum `Json` + serde DTOs; rate limiting in-process).
- **D-06:** Note reads serve the raw markdown **source** (never mdBook build output, b3) with the Phase 15 strong ETag header.
- **D-07:** ACL test matrix ported from MCP `tools_are_acl_scoped` must pass on all four endpoints.

### Response shapes & pagination (discussed 2026-10-30)
- **D-08:** Bare arrays/objects matching MCP field names — no `{"data":...}` envelope. `GET /api/v1/vaults` → `[{"slug","name","scope","role"}]` (same fields as MCP `list_vaults`); notes list reuses the MCP `list_md_files` shape; search reuses the MCP `search_md` hit shape.
- **D-09:** Notes list pagination: opaque base64 cursor over sorted paths; response carries `has_more` + `next_cursor`; `?limit=` default 50, capped at 200.
- **D-10:** Search: top 50 hits, no pagination in v1 (mirrors MCP `search_md` cap).
- **D-11:** `GET /vaults/{slug}/notes/{path}` body is raw markdown, `Content-Type: text/markdown`, plus the strong `ETag` header (quoted sha256 wire form from `webdav::etag_of`).

### Caching (discussed 2026-10-30)
- **D-12:** Note reads honor `If-None-Match` via the Phase 15 `webdav::if_none_match_of` parser → `304 Not Modified` on match; `Malformed` precondition → 400 fail-closed (same rule as WebDAV PUT, PITFALLS C2).

### Rate limiting (discussed 2026-10-30)
- **D-13:** Fixed-window limiter: 120 requests/minute per token, in-process counters (no new deps); every `/api/v1` endpoint counts equally; over-limit → `429` with `Retry-After` seconds.

### API discoverability (discussed 2026-10-30)
- **D-14:** `GET /api/v1` returns a small JSON index of endpoints + auth hint (behind Bearer like the rest of the surface).
- **D-15:** A human-readable `/api/v1/docs` page rendered in the existing `app_page()` shell (Phase 9b rule: no ad-hoc CSS), describing endpoints, auth, pagination, errors, and rate limit.

### Claude's Discretion
- Whether `/api/v1/docs` is public or Bearer-gated — recommend public static HTML (contains no vault data), but planner decides.
- Cursor encoding details (base64 of last path vs opaque token), rate-limiter data structure (e.g. `DashMap`/`Mutex<HashMap>` — must stay dependency-free), problem+json `type`/`code` string vocabulary, exact endpoint path parameter style for `{path}` (nested slashes).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase requirements
- `.planning/ROADMAP.md` §"Phase 16: rest-api" — goal, 7 success criteria, Avoids list (B1 ACL bypass, B2 token leakage, B3 serving build output, B4 cookie/Bearer mixing)
- `.planning/REQUIREMENTS.md` — API-01..API-06 (and API-F01..F03 = explicitly future/out of scope)

### Code to reuse / refactor (read before planning)
- `src/mcp.rs` — the four tools (`list_vaults`, `list_notes`, `read_note`, `search_notes`), helpers `list_md_files`/`search_md`/`source_dir`, and the `tools_are_acl_scoped` test matrix to port (criterion 7)
- `src/webdav.rs` — `etag_of` (~:31, quoted sha256 wire form), `if_none_match_of`/`if_match_of` + `Precondition` enum (fail-closed parsers), `safe_join`, `source_dir_of`
- `src/store.rs` — `user_for_api_token`, `accessible_vaults`, `list_api_tokens`
- `src/server.rs` ~:265-300 — existing `/mcp` Bearer auth middleware pattern to mirror; `app_page()` shell for the docs page

### Phase 15 contract (ETag form)
- `.planning/phases/MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05/15-01-SUMMARY.md` — ETag is the QUOTED wire form `"<sha256-hex>"`; compare quoted forms directly, never re-hash-and-quote separately

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `mcp.rs` tool bodies — nearly the whole read surface already exists (accessible_vaults → vaults JSON; safe_join + list_md_files → notes; read_to_string → note body; search_md(vaults, q, 50) → hits). Extract into shared helpers both MCP and REST call (D-04).
- `webdav::etag_of` — feeds the note-read ETag header and the If-None-Match compare (D-11/D-12).
- `webdav::if_none_match_of` — total fail-closed parser; reuse for 304 support (D-12).
- `/mcp` Bearer middleware (server.rs ~:278-297) — same `user_for_api_token` lookup + 401 shape to mirror on `/api/v1`.
- `app_page()` shell — docs page styling (D-15).

### Established Patterns
- 404-not-403 for invisible vaults is already MCP behavior (`source_dir` returns None → generic error) — REST must keep the same non-disclosing shape via problem+json.
- Best-effort access logging (`log_access`) exists on MCP tools — decide whether REST reuses it (planner discretion).
- No tracing dependency; errors via axum responses; store queries use bound params only.
- Rate limiting must be in-process (single binary) — same justification as phase 15's per-vault in-process locks.

### Integration Points
- New `src/api.rs` module: router branch merged in `server.rs` alongside `/mcp` and WebDAV routes.
- `mcp.rs` refactor: tools call the extracted helpers (keep `tools_are_acl_scoped` green while moving logic).
- `AppState` gains the rate-limiter state (per-token counters).

</code_context>

<specifics>
## Specific Ideas

- User explicitly chose the docs page (8b over 8a): a human-readable `/api/v1/docs` page in the app shell, not just a JSON index.
- Consistency with MCP field names was chosen over REST-convention renaming — clients should be able to move between MCP and REST without relearning shapes.

</specifics>

<deferred>
## Deferred Ideas

- API-F01 write endpoints (upload/PUT via REST) — future requirement, management stays on WebDAV + admin UI.
- API-F02 scoped tokens (`read:notes` vs `read:search`) — future.
- API-F03 webhooks on vault events — future.
- Issue #10 (web+MCP usage stats) — backlog, not folded; rate-limiter counters may later feed it.

</deferred>

---

*Phase: 16-rest-api-read-only-rest-api-on-api-v1-api-01-06*
*Context gathered: 2026-10-30*
