# Phase 13: wiki-changes log (v1.1.0) - Context

**Gathered:** 2026-10-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Owners see what a refresh actually changed (ROADMAP Phase 13, REQ-CHG-001..002):

1. New `vault_changes` table (vault, timestamp, kind added/modified/removed,
   path) populated when Refresh rebuilds a vault — git source: diff old→new
   commit; local source: hash-diff of the source tree before/after.
2. Statistics page gains a "Wiki changes" section: latest entries, newest
   first, grouped per refresh event.
3. Best-effort: a logging failure NEVER fails the refresh (same rule as
   `access_log`).

Out of scope: local vault upload (Phase 14), diff content (only paths +
kind, not line diffs), per-file history views, notifications/emails on
changes, admin-side change dashboards.
</domain>

<decisions>
## Implementation Decisions

### Change capture (REQ-CHG-001)
- **D-01** Git-sourced vaults (the `refresh_my_vault` path, server.rs): read
  the clone's HEAD oid BEFORE `src.sync()`, the new HEAD after; if they
  differ, diff the two commit trees with git2 (`Tree::diff` +
  `diff_iter_with_options`, `find_similar` optional) and map git status →
  kind: added / modified / removed. Paths are repo-relative source paths
  (markdown pages), not built-book paths.
- **D-02** Local/no-git changes: where a rebuild happens without a git
  transition (e.g. WebDAV PUT path), hash-diff the source tree before/after
  (file path + content hash) and record added/modified/removed. Best-effort
  like D-04; if a full-tree hash is impractical at a call site, recording the
  known single-path change (WebDAV PUT = one modified/added path) is
  acceptable — the planner decides per call site.
- **D-03** Storage: migration `migrations/0010_vault_changes.sql` —
  `vault_changes(id INTEGER PK, vault_slug TEXT NOT NULL, event_id TEXT NOT
  NULL, kind TEXT NOT NULL CHECK (kind IN ('added','modified','removed')),
  path TEXT NOT NULL, ts TEXT NOT NULL DEFAULT (datetime('now')))`. All rows
  of one refresh share one `event_id` (uuid or ts-based opaque id). Index on
  `(vault_slug, ts)`. Follow the 0008 migration + store.rs access_log
  conventions (plain sqlx queries, no migrations table beyond the existing
  mechanism).
- **D-04** Best-effort rule (hard requirement): every write to `vault_changes`
  is wrapped so errors are logged (`eprintln!("vault-changes: …")` — tracing
  is NOT a dependency of this project; do not add it) and swallowed — the refresh
  response must not change if logging fails. Same discipline as the
  `access_log` insert (store.rs).
- **D-05** Retention: prune `vault_changes` older than 90 days alongside
  `prune_access_log` (main.rs startup prune). Same window, same call site.

### Statistics display (REQ-CHG-002)
- **D-06** `vault_stats` page gains a "Wiki changes" section: latest events
  (cap ~5 events / 30 rows), newest first, each event rendered as a group
  (header = timestamp + count, body = kind badge + path per row). Empty
  state: "No changes recorded yet."
- **D-07** Access: identical ACL to the existing stats page (owner, admin,
  granted users) — no new gating logic; the section renders inside the
  existing `app_page()` shell with the design-system styling (do not touch
  global CSS ad hoc — Phase 9b rule).
- **D-08** Kind vocabulary is exactly `added` / `modified` / `removed` (the
  ROADMAP wording); display labels may be prettified ("Added", "Modified",
  "Removed").

### Claude's Discretion
- Exact event_id format, store method names, HTML markup details, whether to
  use git2 `find_similar` (rename detection) — planner/executor decide.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/ROADMAP.md` §"Phase 13: wiki-changes log" — criteria REQ-CHG-001..002
- `.planning/REQUIREMENTS.md` — REQ-CHG-001 (line ~203), REQ-CHG-002
- `src/server.rs` `refresh_my_vault` (~2540) — the refresh pipeline where the
  git diff hooks in; `vault_stats` (~2424) — the page gaining the section
- `src/sources.rs` `GitSource::fetch_and_reset` (~125) — where HEAD moves on
  sync; git2 already a dependency (vendored-openssl)
- `src/store.rs` access_log block (~1379-1430) — insert/aggregation/prune
  pattern to mirror
- `migrations/0008_shares_stats.sql`, `migrations/0009_vault_display_options.sql`
  — migration style
- `.planning/phases/MORI-12-demo-lock-vault-display-options-v1-1-0/12-CONTEXT.md`
  — serve-time vs build-time decisions still in force

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `access_log` stack (insert + aggregation queries + `prune_access_log` +
  startup prune in main.rs:148): clone the shape for `vault_changes`.
- git2 is already vendored and used in `sources.rs` (clone/fetch/reset) — the
  tree-diff needs no new dependency.
- `vault_stats` already aggregates access_log per vault — the new section is
  one more query + one more HTML block via `app_page()`.

### Established Patterns
- Best-effort telemetry: access_log failures never fail the request path.
- Migrations are numbered SQL files in `migrations/` applied at startup.
- Neutral test credentials only (`test-pass-9f3a`); tests live next to code
  in `mod tests`.

### Integration Points
- `refresh_my_vault` (server.rs) — capture old HEAD, sync, diff, insert rows.
- WebDAV PUT rebuild path (webdav.rs) — optional local hash-diff hook (D-02).
- `vault_stats` (server.rs) — render section.
- `main.rs` startup — extend prune call.

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches.

</specifics>

<deferred>
## Deferred Ideas

- Line-level diffs / "view diff" links per change — future phase.
- Change notifications (email/webhook) — out of scope, no requirement.

</deferred>

---

*Phase: 13-wiki-changes-log-v1-1-0*
*Context gathered: 2026-10-29*
