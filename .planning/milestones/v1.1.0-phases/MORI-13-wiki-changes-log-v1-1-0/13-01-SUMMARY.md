# 13-01 Summary — Wiki-changes capture layer (REQ-CHG-001)

**Status:** COMPLETE. Commits: `6ab986e` (Task 1) + `70089e5` (Task 2). Full suite green (316 passed, 0 failed), `cargo clippy --all-targets -D warnings` clean, `cargo fmt --check` clean.

## What was built

### Task 1 — migration + store trio (commit `6ab986e`)

- `migrations/0010_vault_changes.sql`: `vault_changes(id, vault_slug, event_id, kind TEXT CHECK IN ('added','modified','removed'), path, ts DEFAULT datetime('now'))` + `idx_vault_changes_slug_ts ON (vault_slug, ts)`, Phase 13 header comment, 0008-style. Auto-applied by `sqlx::migrate!` — no registry edit (D-03).
- `src/store.rs` (after `prune_access_log`), access_log-shaped, bind params only:
  - `log_vault_change(slug, event_id, kind, path)` — insert, doc-commented best-effort (D-04).
  - `recent_vault_changes(slug, max_events, max_rows) -> Vec<(event_id, ts, kind, path)>` — newest-first, one query with the `event_id IN (SELECT … GROUP BY event_id ORDER BY MAX(ts) DESC, MAX(id) DESC LIMIT ?)` event cap, second LIMIT caps rows (D-06 caps enforced by caller).
  - `prune_vault_changes() -> u64` — `DELETE … WHERE ts < datetime('now', '-90 days')`, returns `rows_affected()` (D-05).

### Task 2 — capture call sites (commit `70089e5`)

- `src/server.rs` free fns next to `refresh_my_vault`:
  - `git_head_oid(repo_dir) -> Option<git2::Oid>` — None on missing repo / unborn HEAD.
  - `diff_commit_trees(repo_dir, old, new) -> Vec<(&'static str, String)>` — `diff_tree_to_tree`, no `find_similar`; `Added→"added"`, `Modified→"modified"`, `Deleted→"removed"`, other statuses skipped; repo-relative forward-slash paths (D-01, D-08).
  - `record_vault_changes(store, slug, repo_dir, old, new)` — one `uuid::Uuid::new_v4()` event_id per refresh, one row per changed path.
- `refresh_my_vault`: `old_head = git_head_oid(&repo_dir)` captured BEFORE `src.sync()` (server.rs:2648 vs sync at :2650); after `seed_vault` succeeds, the diff block runs only when HEAD actually moved, wrapped in `if let Err(e) → eprintln!("vault-changes: …")` — the `Redirect::to` response is byte-identical either way (D-04).
- `webdav_entry` PUT branch: `existed = metadata(&full).is_ok()` captured immediately before `tokio::fs::write`; after the successful write, a `tokio::spawn` fire-and-forget (log_web_access style) records one event for the single written `rel` path — `modified` if it existed, else `added` (D-02 single-path recording; no full-tree hash diff). Comment records that WebDAV exposes no DELETE verb, so no `removed` rows come from this path. Spawn is placed before `rebuild_book` per the plan's key_links.
- `src/main.rs`: `store.prune_vault_changes().await?;` immediately after `store.prune_access_log().await?` (:148/:150), same 90-day window, REQ-CHG-001 comment (D-05).

## Tests added (all passing)

Task 1 (`src/store.rs`, `mod vault_changes_tests`):
- `vault_changes_migration_shape`
- `vault_changes_rejects_bad_kind` — proves SQLite CHECK rejects `kind='renamed'` (D-08 DB-enforced)
- `vault_changes_roundtrip_and_grouping`
- `prune_vault_changes_removes_only_old_rows`

Task 2 (`src/server.rs`, `mod tests`):
- `diff_commit_trees_maps_kinds` — unit, git2 fixture: added/modified/removed map exactly
- `refresh_records_vault_changes` — e2e: HEAD moved → rows with expected kinds, ALL sharing one event_id, response still SEE_OTHER (D-04)
- `refresh_noop_records_nothing` — unchanged HEAD → zero rows
- `webdav_put_records_change` — PUT new path → 201 + `added` row; PUT again → `modified`, own event_id, 201 unchanged (D-02, D-04)

Test helpers added: `commit_all` (git2 stage-all commit), `load_git_vault` (login + create-form load).

## Deviations

- **Recovery execution:** Task 2 landed as verified working-tree WIP from a SIGTERM-killed executor, not fresh TDD. The WIP was inspected line-by-line against the plan's must_haves and found complete; the only fix applied was moving the WebDAV record block to before `rebuild_book` (plan key_links ordering) and `cargo fmt` on two spots. All four Task-2 tests were then run and pass.
- **`diff_commit_trees` uses `diff.deltas()`** instead of `diff.diffs()` (plan prose said `diffs()`); `deltas()` is the file-level delta iterator — `diffs()` yields per-hunk records and would double-count files. Same mapping, correct semantics.
- **`refresh_records_vault_changes` asserts `contains` for the two expected kinds** rather than exact row equality (the refresh may legitimately touch other fixture paths); the shared-event_id assertion is exact.

## Decisions

- event_id = uuid v4 string (planner discretion, uuid already a dependency).
- No `find_similar`: renames = one `removed` + one `added` row (keeps D-08 vocabulary exact).
- WebDAV PUT = single-path recording, not full-tree hash diff (D-02 call-site decision).
- Best-effort logging via `eprintln!("vault-changes: …")` — tracing is not a dependency; matches the repo's `eprintln!("webdav: …")` style.

## Lessons

- `git2` `Diff::deltas()` (file-level) vs `Diff::diffs()` (hunk-level): use `deltas()` for a changed-file list or multi-hunk files produce duplicate rows.
- Fire-and-forget DB writes in handlers need a short poll loop in tests (the spawned task may land after the response); 50×20 ms poll on the store query is reliable and fast.
- The patch tool's single-file Rust lint runs edition-2015 rustc and floods false `async fn` errors on this crate — trust `cargo clippy`/`cargo test` instead.
- Recovery pattern that worked: `git diff` the WIP against the plan's must_haves as a checklist, run the plan's targeted tests first, then the full suite with the awk-summed one-liner before committing.
