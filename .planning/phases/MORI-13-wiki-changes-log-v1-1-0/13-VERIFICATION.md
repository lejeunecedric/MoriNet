---
phase: MORI-13-wiki-changes-log-v1-1-0
verified: 2026-10-29
status: passed
score: "10/10"
plans: [13-01, 13-02]
---

# Phase 13 Verification — Wiki-changes log

Goal-backward verification against the actual tree at `6997d70` (feature
commits `6ab986e`, `70089e5`, `08e2336`). Every ROADMAP criterion was checked
against the implementation itself (file:line inspection), not the SUMMARYs,
and each phase test was re-executed live:

- `cargo test --lib vault_changes` → **5 passed, 0 failed**
  (`vault_changes_migration_shape`, `vault_changes_rejects_bad_kind`,
  `vault_changes_roundtrip_and_grouping`, `prune_vault_changes_removes_only_old_rows`,
  `refresh_records_vault_changes`)
- `cargo test --lib refresh_records` → **1 passed, 0 failed**
- `cargo test --lib webdav_put_records_change` → **1 passed, 0 failed**
- `cargo test --lib stats_` → **9 passed, 0 failed** (includes
  `stats_shows_wiki_changes_section`, `stats_wiki_changes_empty_state`,
  `stats_wiki_changes_escapes_paths`, plus the 6 pre-existing stats tests —
  no regression on the shared panel)

## ROADMAP criteria

| Criterion | Verdict | Evidence |
|---|---|---|
| New `vault_changes` table (vault, timestamp, kind added/modified/removed, path); populated at refresh (git: diff old→new commit; local: hash-diff) — REQ-CHG-001 | **PASS** (with sanctioned D-02 deviation, see below) | `migrations/0010_vault_changes.sql:9-16`: `vault_changes(id PK, vault_slug, event_id, kind CHECK IN ('added','modified','removed'), path, ts DEFAULT datetime('now'))` + `(vault_slug, ts)` index. Store trio `log_vault_change` / `recent_vault_changes` / `prune_vault_changes` at `src/store.rs:1437-1493`. Git capture: `git_head_oid` + `diff_commit_trees` (git2 `diff_tree_to_tree`, Added/Modified/Deleted → added/modified/removed, `src/server.rs:2608-2643`) called from `refresh_my_vault` only when HEAD moved, one uuid-v4 `event_id` per refresh (`record_vault_changes`, `src/server.rs:2645-2665`, hook at `2705-2715`). Tests: `refresh_records_vault_changes` (real git repo, HEAD moves → added `new-page.md` + modified `index.md`, single shared `event_id`, response still SEE_OTHER), `refresh_noop_records_nothing` (HEAD unmoved → zero rows), `vault_changes_rejects_bad_kind` (CHECK lock). |
| Statistics page gains a "Wiki changes" section: latest entries, newest first, grouped per refresh event — REQ-CHG-002 | **PASS** | `vault_stats` (`src/server.rs:2536-2580`): `recent_vault_changes(&slug, 5, 30)` (5 events / 30 rows caps enforced in SQL, `src/store.rs:1462-1483`, newest-first via `ORDER BY MAX(ts) DESC … , ts DESC, id DESC`), consecutive rows grouped per `event_id` into `<h3>{ts} — {n} change(s)</h3>` + table, rendered under `<h2>Wiki changes</h2>` (`src/server.rs:2596`). Empty state `No changes recorded yet.`; ts/kind/path all `html_escape`d (T-13-08). Tests: `stats_shows_wiki_changes_section` (two events, asserts newest-event-first by position, per-event counts, prettified Added/Modified/Removed labels), `stats_wiki_changes_empty_state`, `stats_wiki_changes_escapes_paths` (`<script>` payload → `&lt;script&gt;`, raw form absent). |
| Best-effort: a logging failure never fails the refresh (same rule as access_log) | **PASS** | Refresh: capture is inside the `Ok(())` arm of `seed_vault`, wrapped in `if let Err(e) = record_vault_changes(…) { eprintln!(…) }` (`src/server.rs:2705-2715`) — the redirect is returned regardless. WebDAV PUT: recording is a fire-and-forget `tokio::spawn` with `eprintln!` swallow (`src/server.rs:895-910`); the 201 never awaits it. Boot prune is a plain `?`-propagating call like its `prune_access_log` sibling (`src/main.rs:148-150`), matching the established access_log rule. `refresh_records_vault_changes` asserts the refresh response is unchanged (SEE_OTHER) alongside recording. |

## REQ status

- **REQ-CHG-001** — satisfied (table + refresh capture + best-effort + 90-day
  boot prune). Note: `.planning/REQUIREMENTS.md:203` still shows `- [ ]`; the
  checkbox flip is a docs bookkeeping item outside this verification's
  write scope (verification file only).
- **REQ-CHG-002** — satisfied (section, newest-first, grouped per event).

## Documented deviation (not a failure)

The ROADMAP's "local: hash-diff source tree before/after" was consciously
scoped by **13-CONTEXT.md D-02**: "if a full-tree hash is impractical at a
call site, recording the known single-path change (WebDAV PUT = one
modified/added path) is acceptable — the planner decides per call site."
Implemented exactly as sanctioned: the WebDAV PUT handler records one row per
write — `added` on first write, `modified` on rewrite (`existed` check,
`src/server.rs:890-908`); no `removed` rows since WebDAV exposes no DELETE
verb at that call site (comment at `src/server.rs:893-894`). Test
`webdav_put_records_change` proves added→modified progression and that each
PUT gets its own `event_id`. Git-source refresh uses the full tree diff, so
the deviation is limited to the no-git call site, as D-02 permits.

## Overall

**Status: passed — 10/10.** All three ROADMAP criteria have concrete code and
green-test evidence re-run live; the single scope deviation is a sanctioned
planner decision (D-02), implemented and tested as documented. No source
changes required.
