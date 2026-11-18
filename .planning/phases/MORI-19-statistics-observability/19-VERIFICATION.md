---
status: passed
phase: 19
score: "6/6 must-haves verified"
updated: 2026-11-18T17:55:08Z
---

# Phase 19 Verification — statistics-observability

**Goal under test:** "every vault read is counted on every surface, and the Statistics page answers 'what did people read, ask, and find?'"

**Verdict: PASSED.** All 6 ROADMAP success criteria verified against the live codebase and the test suite at HEAD (`cf3f2c9`). Full suite re-run here: **589 passed / 4 ignored** (baseline 554 → +35), `scripts/check.sh` green (fmt + clippy -D warnings + tests + release build).

## Requirement trace (plan frontmatter vs REQUIREMENTS.md)

| ID | Covered by plans | Verified |
|----|------------------|----------|
| STAT-01 | 19-01, 19-02, 19-03 | ✅ web/API/MCP all log; CHECK widened |
| STAT-02 | 19-01, 19-04 | ✅ Top pages, web source, date-range aware |
| STAT-03 | 19-01, 19-02, 19-04 | ✅ MCP activity two-column section |
| STAT-04 | 19-01, 19-02, 19-03, 19-04 | ✅ search_queries mcp+api, keywords section |
| STAT-05 | 19-01, 19-02, 19-03, 19-04 | ✅ best-effort spawn, 90-day prune, no full bodies |

Every STAT-01..05 ID appears in at least one plan's `requirements:` frontmatter and is backed by code + tests. No orphan IDs.

## Criterion 1 — Migration 0014 widens access_log.source CHECK ✅

`migrations/0014_stats_observability.sql`: create-new/copy/drop/rename rebuild. CHECK is now `('web', 'mcp', 'api')`; `INSERT INTO access_log_new (id, vault_slug, email, source, path, ts) SELECT ... FROM access_log` copies every pre-existing row including its `id`; both original indexes (`idx_access_log_slug_ts`, `idx_access_log_email`) recreated after the rename. sqlx wraps the file in a transaction (atomic).
- Test `access_log_rejects_bad_source` (src/store.rs:2493): `'ftp'` rejected, `'web'`/`'mcp'`/`'api'` all accepted.
- Test `access_log_indexes_survive_rebuild` (src/store.rs:3659): both index names present in `sqlite_master`.
- Boot on upgraded DB: `sqlx::migrate!("./migrations").run()` (src/store.rs:117) runs 0014 over any 0008-era DB; the whole 589-test suite boots through this path on every run.

## Criterion 2 — Every surface appends an access event; logging never fails the request ✅

- **Web:** `log_web_access` (src/server.rs:2809) — `tokio::spawn` + `let _ = store.log_access(..., "web", ...)`, error dropped.
- **REST API:** `log_api_access` (src/api.rs:605) and `log_api_search` (src/api.rs:620) — spawn-and-drop; called only on the ACL-passing success path (`api_read_note` src/api.rs:549, `api_search` src/api.rs:496). Denied reads return the 404-collapse BEFORE any logging (T-19-07).
- **MCP:** `log_access` (src/mcp.rs:214), `log_activity` (src/mcp.rs:229), `log_search_query` (src/mcp.rs:255) — all spawn-and-drop, wired only on ACL-passing arms.
- Tests: `api_read_note_logs_access`, `api_search_logs_access_and_query`, `api_denied_call_logs_nothing` (src/api.rs); `mcp_read_note_logs_activity`, `mcp_search_logs_query`, `mcp_denied_call_logs_no_activity` (src/mcp.rs). Denied calls write NOTHING to any new table (no probing trail).

## Criterion 3 — "Top pages" per vault, web source, ranked, date-range aware ✅

`stats_top_pages(slug, days)` (src/store.rs:1460): `WHERE vault_slug = ? AND source = 'web' AND ts >= datetime('now', ?) GROUP BY path ORDER BY COUNT(*) DESC LIMIT 10`. Rendered in the owner panel `<h2>Top pages</h2>` (src/server.rs:3126). `days` bound as a `datetime()` modifier (whitelisted integer, never concatenated).

## Criterion 4 — "MCP activity" two-column newest-first list ✅

`recent_mcp_activity(slug, days, 20)` (src/store.rs:1539) returns `ts DESC`. Rendered at src/server.rs:3002-3031: `<th>Request</th><th>Outcome</th>` — left = tool + args + actor email, right = status · latency ms · size B + excerpt. Every interpolated field html_escaped (args/excerpt originate from note content — T-19-10). `mcp_activity` schema has NO column that could hold a full response body — only capped `excerpt` + `resp_size` (STAT-05).
- Tests: `stats_shows_mcp_activity_section`, `stats_mcp_activity_escapes_untrusted`, `mcp_activity_roundtrip_newest_first`.

## Criterion 5 — "Top search keywords" aggregates MCP + REST queries ✅

`search_queries` table (source CHECK `IN ('mcp','api')`), one row per searched vault. Fed by MCP `search_notes` (src/mcp.rs:422/440) and REST `api_search` (src/api.rs:637). `top_search_queries` (src/store.rs:1590) aggregates across BOTH sources; `top_words` (src/server.rs:2835) tokenizes recent raw queries (lowercased, whitespace-split, len ≥ 3). Rendered as `<h2>Search keywords</h2>` + Top words (src/server.rs:3031-3064).
- Tests: `top_search_queries_aggregates_and_orders`, `top_words_*`, `stats_top_queries_and_words_section`.

## Criterion 6 — 90-day boot prune, check.sh green, suite green ✅

`prune_mcp_activity` (src/store.rs:1559) and `prune_search_queries` (src/store.rs:1631) follow the `prune_vault_changes` convention (90-day DELETE, return `rows_affected`), called at boot in src/main.rs:158-162 with the `prune_auth_events` eprintln-guard shape — a failed prune never aborts boot.
- Tests: `prune_mcp_activity_removes_only_old_rows`, `prune_search_queries_removes_only_old_rows`.
- `scripts/check.sh` → `check: OK` (fmt, clippy -D warnings, tests, release build).
- Full suite: **589 passed / 4 ignored** (baseline 554 → +35). Targeted re-runs: `--lib stats` 18 passed, `--lib mcp` 38 passed, `--lib api` 66 passed, `--lib store` 102 passed.

## Additional security checks (STAT-05 ACL semantics)

- Detailed sections (MCP activity + Search keywords) render ONLY inside the `entry.personal || is_admin` owner branch (src/server.rs ~2903); sharees get only their own-count line — `stats_sharee_no_detailed_sections` proves zero leak.
- `stats_days` strict whitelist (7/30/90, else 90) — `stats_days_whitelist` pins SQL-injection junk falls back to 90 (T-19-12).

## human_verification (optional visual confirmation)

All criteria are proven by automated tests; the following is an OPTIONAL visual smoke-test of the rendered page, not a gap:

1. `export PATH="$HOME/.cargo/bin:$PATH" && export MORINET_ADMIN_EMAIL=admin@example.com MORINET_ADMIN_PASSWORD=morinet-dev-2026 && ./target/release/morinet serve --config ./config.toml`
2. Log in as the demo owner → open `/vault/demo/stats?days=7`.
3. Confirm: chip row (7/30/90) under "Statistics"; "MCP activity" two-column table (request left / outcome right, newest first); "Search keywords" table + Top words; switching `?days=` shifts every section together.
4. As a shared reader, `/vault/demo/stats` shows only chips + own-count (no detailed sections).

## gaps

None. All 6 ROADMAP success criteria and all 5 STAT requirements verified against the actual codebase and passing tests.
