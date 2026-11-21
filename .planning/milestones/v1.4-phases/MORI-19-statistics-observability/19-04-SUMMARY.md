---
phase: MORI-19-statistics-observability
plan: 19-04
one_liner: "Statistics page now answers 'what did people read, ask, and find?': fixed 7/30/90-day chips (?days=N, default 90) drive EVERY section via the 19-01 days-parameterized queries, plus two new owner/admin-only sections — 'MCP activity' two-column request↔outcome table (STAT-03) and 'Search keywords' top-queries + top-words (STAT-04) — all fields html_escaped, zero new CSS."
requirements_completed: [STAT-02, STAT-03, STAT-04, STAT-05]
subsystem: server
tags: statistics-page, date-range-chips, mcp-activity, search-keywords, xss-escaping, owner-admin-gate

requires:
  - phase: 19-01
    provides: recent_mcp_activity / top_search_queries / recent_search_queries accessors + days:i64 on stats_per_day/stats_top_pages/stats_own_count/recent_vault_changes
provides:
  - stats_days strict whitelist (7/30/90, 90 fallback) + StatsQuery DTO on vault_stats
  - top_words tokenizer (D-04 locked wording)
  - chip strip (menu-btn links + aria-current) in BOTH owner and sharee panels
  - MCP activity + Search keywords owner-panel sections
affects: [phase-19-verifier]

key-files:
  created: []
  modified: [src/server.rs]

key-decisions:
  - "chips built once before the owner/sharee branch — sharee mini-panel gets the strip too (D-06 says ALL sections; the sharee own-count honors days)"
  - "MCP row format omits ts (plan-locked markup: request cell = tool — args + email small; outcome cell = status · latency · size + excerpt); newest-first ordering comes from the store query"
  - "section order in owner body: Activity by day → Top pages → MCP activity → Search keywords → Wiki changes (plan context)"

requirements-completed: [STAT-02, STAT-03, STAT-04, STAT-05]

duration: ~35min
completed: 2026-11-18
---

# Phase 19 / Plan 19-04: Statistics page chips + MCP activity + Search keywords Summary

**The Statistics page now answers "what did people read, ask, and find?": 7/30/90-day chips (default 90 = retention ceiling) drive every section, and two new owner/admin-only sections — "MCP activity" (two-column request↔outcome, newest-first) and "Search keywords" (top queries + top words) — render from the 19-01 store accessors with zero new CSS.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 2 (Task 1 TDD, Task 2 standard-with-tests per plan)
- **Files modified:** 1 (src/server.rs only — `git diff --name-only assets/` empty, D-03)

## What was built

**Task 1 `date-range-chips-and-top-words-pure` (`6320ede` + fmt `style` commit)**
- `fn stats_days(raw: Option<&str>) -> i64` — strict whitelist `Some("7") => 7, Some("30") => 30, Some("90") => 90, _ => 90` (T-19-12: only three bound integers ever reach SQL).
- `fn top_words(queries: &[String], top: usize) -> Vec<(String, usize)>` — D-04 locked wording: `to_lowercase().split_whitespace()`, keep `chars().count() >= 3`, HashMap count, sort (count DESC, word ASC), truncate. No stopwords, no punctuation stripping.
- `#[derive(serde::Deserialize)] struct StatsQuery { days: Option<String> }` + `Query(q): Query<StatsQuery>` on `vault_stats`; `let days = stats_days(q.days.as_deref());` replaces every literal `90` at the four 19-01 call sites (stats_own_count, stats_per_day, stats_top_pages, recent_vault_changes) — STAT-02: Top pages is now date-range aware.
- Chip strip: three `<a class="menu-btn" href="/vault/{slug}/stats?days=N">` links (slug html_escaped), active one carries `aria-current="true"`; interpolated right after `<h2>Statistics</h2>` in BOTH the owner panel and the sharee mini-panel. No CSS added.

**Task 2 `mcp-activity-and-search-keywords-sections` (`b2148b3`)**
- `mcp_html`: `recent_mcp_activity(&slug, days, 20)` → two-column table (`Request` = `{tool} — {args}` + `<small>{email}</small>`; `Outcome` = `{status} · {latency} ms · {size} B` + excerpt), empty state `<p>No MCP activity recorded yet.</p>`. Best-effort (`.await` error → empty state, never 500). html_escape on tool/args/email/status/excerpt (T-19-10).
- `search_html`: `top_search_queries(&slug, days, 10)` table + `top_words(recent_search_queries(&slug, days, 200), 10)` list under `<h3>Top words</h3>`; empty state `<p>No searches recorded yet.</p>`; html_escape on query + word.
- Both interpolated between the Top pages table and `<h2>Wiki changes</h2>` — strictly inside the existing `entry.personal || is_admin` owner branch (D-05; sharee and 403 paths untouched apart from the chip strip).

## New tests (10)

`server::tests`: `stats_days_whitelist`, `top_words_lowercases_and_filters_short_tokens`, `top_words_orders_by_count_then_alpha`, `top_words_caps_at_top`, `top_words_empty_input` (Task 1); `stats_shows_mcp_activity_section`, `stats_mcp_activity_escapes_untrusted`, `stats_top_queries_and_words_section`, `stats_sharee_no_detailed_sections`, `stats_days_chip_filters_rows` (Task 2).

## TDD evidence

- **Task 1 RED:** `cargo test --lib -- stats_days_whitelist top_words` → compile failure `error[E0425]: cannot find function stats_days` (×7) + same for `top_words`. **GREEN:** 16 passed (5 new + 11 pre-existing stats tests).
- **Task 2 RED:** `stats_shows_mcp_activity_section`, `stats_mcp_activity_escapes_untrusted`, `stats_top_queries_and_words_section` FAILED (section heading absent). `stats_sharee_no_detailed_sections` and `stats_days_chip_filters_rows` **passed on write — honest coverage locks** (the first trivially holds while the sections don't exist and pins the D-05 no-leak rule going forward; the second's filtering was already delivered by Task 1's days wiring). **GREEN:** `cargo test --lib -- stats_` → 17 passed.

## Verification

- `cargo test --lib` → **587 passed / 2 failed / 4 ignored** (579 baseline + 10 new). The 2 failures are exactly the known-flaky `export_waits_for_semaphore_permit` + `export_zip_streams_bundle`; isolated re-run → 2 passed. Recorded as pre-existing flakes per baseline instructions, not debugged.
- `cargo fmt` (one style commit) + `cargo clippy --all-targets -- -D warnings` clean.
- Acceptance greps: `fn stats_days` / `fn top_words` / `Some("7") => 7` / `StatsQuery` / `stats_days(q.days.as_deref())` / `aria-current` all match; `grep -c "days=7\|days=30\|days=90"` = 3; `git diff --name-only assets/` empty (D-03 zero new CSS).
- Pre-existing stats tests untouched and green (stats_owner_sees_full_panel, stats_sharee_sees_only_own_count, stats_unrelated_user_forbidden, stats_shows_wiki_changes_section, stats_sharee_branch_unchanged, export_link_in_stats_panel).

## Deviations from plan

1. **`top_words` test expectation completed:** the plan's example `["Foo BAR foo", "ab baz"] → foo:2, baz:1` omits `bar:1` — but "bar" is 3 chars and the locked rule keeps it. Test pins the full rule output `[foo:2, bar:1, baz:1]` (rule text is authoritative over the abbreviated example).
2. **Chip strip also in the sharee panel** — explicitly sanctioned by the plan ("interpolate it … AND in the sharee mini-panel body"); noted here because it changes sharee HTML (the `stats_sharee_branch_unchanged` / `stats_sharee_sees_only_own_count` negative assertions still pass — they check owner-panel strings, not the chips).
3. **fmt-only style commit** after each feature commit (rustfmt re-wrapped the chips `if` arms and the `match` header) — same file, no behavior change.
4. **Pre-existing flakes:** the two export tests failed in the full parallel run, green isolated — recorded per baseline.

## Decisions

- Chips built once before the owner/sharee branch so both panels share one construction; `days` computed before the gate so the sharee own-count honors the window too (D-06 "EVERY section").
- MCP row markup follows the plan-locked format exactly (no ts column) — newest-first ordering is inherent to the store query (`ts DESC, id DESC`), matching the Wiki-changes grouping precedent's shape.
- `search_html` empty-state condition is `top_queries.is_empty() && words.is_empty()` (both accessors read the same table, so they agree; the AND is defensive).

## Lessons

- The patch tool's post-edit lint runs rustc at edition 2015 on this crate and floods false E0670/E0425 errors — trust `cargo test`/`cargo clippy` instead; don't chase patch-lint output on this repo.
- `req()` in the server test module takes `Router` by value — a test issuing two GETs needs `app.clone()` on the first.
- axum implicit named-format-args capture lets a body `format!` pick up new locals (`{mcp_html}`) without touching the explicit arg list.

## Notes for next executor / verifier

- **Manual UAT:** log in as the demo owner → `/vault/demo/stats` shows the chip row under "Statistics"; `?days=7`, `?days=30`, `?days=90` (and any junk value → 90) shift Activity by day, Top pages, MCP activity, Search keywords and Wiki changes together. Seed rows to exercise the sections: `store.log_mcp_activity("admin@example.com","demo","read_note","{\"path\":\"x.md\"}","ok",12,3456,"excerpt text")` and `store.log_search_query("admin@example.com","demo","rust macros","mcp")` — or just hit the MCP `read_note`/`search_notes` tools and the REST search once. A sharee (`/vault/demo/stats` as a shared reader) sees only chips + own-count.
- `stats_days_chip_filters_rows` backdates via `sqlx::query(...).execute(&state.store.pool)` — `Store::pool` is pub, usable from any test.
- Phase verifier criteria touched: ROADMAP 3 (STAT-02 date-range Top pages), 4 (STAT-03 MCP activity), 5 (STAT-04 search keywords); D-05/D-06 display halves.
