# 13-02 Summary — 'Wiki changes' section on the vault stats page (REQ-CHG-002)

**Status:** COMPLETE. Commit: `08e2336` (feat). Full suite green (320 passed, 0 failed = 316 baseline + 4 new), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, `git diff --stat assets/` empty (D-07: global CSS untouched).

## What was built

Single file touched: `src/server.rs`, all inside `vault_stats`'s owner/admin branch (after `top_rows`, before the panel `format!`) plus four tests in `mod tests`.

- **Query**: `state.store.recent_vault_changes(&slug, 5, 30).await.unwrap_or_default()` — best-effort like `stats_per_day`/`stats_top_pages`; a read error degrades to the empty state, never breaks the page (D-06 caps 5 events / 30 rows enforced by the 13-01 query).
- **Grouping in Rust**: one pass over the newest-first rows; a new group starts when `event_id` differs from the previous row's; group carries its first `ts` as header timestamp and its row count. Order preserved exactly as returned.
- **Markup** (planner-discretion markup, verbatim from the plan): per event `<h3>{ts} — {n} change(s)</h3>` + `<table><thead><tr><th>Change</th><th>Path</th></tr></thead><tbody>…</tbody></table>`, rows `<tr><td>{Label}</td><td>{path}</td></tr>`. Empty state: `<p>No changes recorded yet.</p>` (exact D-06 text). Interpolated into the existing panel `format!` as `{changes_html}` under `<h2>Wiki changes</h2>`, after the Top pages table — raw-string HTML, no new CSS classes, no new route, no new ACL logic (D-07).
- **Kind label** (D-08): first letter upper-cased via `label.get_mut(0..1).make_ascii_uppercase()` (boundary-safe) → "Added"/"Modified"/"Removed"; stored vocabulary untouched.
- **Escaping** (T-13-08): `html_escape` on every interpolated value — `ts`, label, path. Comment cross-references T-10.1-09.
- **Sharee branch and 403 path**: byte-identical (untouched; verified by test).

## Tests added (`src/server.rs`, `mod tests`, all passing)

- `stats_shows_wiki_changes_section` — two seeded events (A: added+removed, B: modified); asserts section heading, all three paths, `>Added<`/`>Modified<`/`>Removed<` labels, `2 change(s)` + `1 change(s)` headers, and event B's row positioned before event A's (newest-first).
- `stats_wiki_changes_empty_state` — no rows → exact `No changes recorded yet.` text, still 200.
- `stats_wiki_changes_escapes_paths` — path `<script>alert(1)</script>` → body contains `&lt;script&gt;`, NOT raw `<script>alert` (T-13-08).
- `stats_sharee_branch_unchanged` — sharee (hidden=0 direct share via `seed_owner_and_shared_share`) sees own-count line, no `Wiki changes` heading, no change-path leak even with rows seeded (D-07 / T-13-07).

TDD followed: tests written first; the three positive tests failed (red), implementation made them pass (green). `cargo test stats_` → 9 passed (4 new + 5 existing stats tests).

## Deviations

- None functional. The `stats_sharee_branch_unchanged` test passed already in the red phase (it is a negative assertion against untouched code); the other three failed as required.
- `cargo fmt` reflowed two multi-line `assert!` macros in the new tests after the initial write — formatting only.

## Decisions

- Grouping via `Vec<(event_id, ts, Vec<row_html>)>` + `last_mut()` match — simplest single-pass shape preserving query order; no HashMap (order must be preserved, keys may repeat non-adjacently in theory but never do given `ORDER BY ts DESC, id DESC` per event).
- Capitalization done with `get_mut(0..1)` + `make_ascii_uppercase` instead of `chars()`/`to_uppercase` formatting — avoids multi-byte panic and extra allocation; kinds are CHECK-constrained ASCII anyway.
- Section header is `<h2>Wiki changes</h2>` (page-level heading level matching "Activity by day"/"Top pages"), event headers `<h3>` per plan markup.

## Lessons

- The patch tool's edition-2015 rustc lint noise on this crate fires on every `server.rs` edit (known from 13-01); `cargo clippy` + `cargo test` are the real gates.
- A negative-assertion test (sharee branch unchanged) passes before the feature exists — fine as a regression guard, but the red phase must be judged on the positive tests only.
- Seeding `vault_changes` rows directly via `store.log_vault_change` makes display tests trivial — no git fixtures needed (rows sharing one second still order correctly because the query tie-breaks on `id DESC`).
