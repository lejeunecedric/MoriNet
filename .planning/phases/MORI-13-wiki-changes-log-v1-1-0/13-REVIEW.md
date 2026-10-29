---
phase: MORI-13-wiki-changes-log-v1-1-0
review: "13-REVIEW"
date: 2026-10-29
commits_reviewed: [6ab986e, 70089e5, 08e2336]
specs: [13-01-PLAN.md, 13-02-PLAN.md, 13-CONTEXT.md (D-01..D-08)]
status: passed
---

# Phase 13 code review — wiki-changes log

**Status: passed.** No HIGH or MEDIUM findings. The three commits implement
D-01..D-08 as locked; every hard requirement (best-effort D-04, bind-params
only, html_escape at render, exact kind vocabulary, 90-day prune) verified
against the live tree, not the SUMMARYs. Baseline re-verified on this tree:
`cargo test --lib` → **320 passed, 0 failed**; `cargo clippy --all-targets --
-D warnings` → clean. Findings below are LOW (minor correctness edges) and
INFO (plan-locked behaviors worth recording).

## Verified against the code (what the hunt checked)

- **git2 diff kind mapping (D-01/D-08)** — `src/server.rs:2628-2637`:
  `Added→"added"`, `Modified→"modified"`, `Deleted→"removed"`, every other
  status skipped; path from `old_file()` for Deleted, `new_file()` otherwise.
  Correct. No `find_similar` (rename → removed+added), per the locked planner
  decision. Unit test `diff_commit_trees_maps_kinds` (server.rs:4118) pins
  all three kinds.
- **Pre-sync HEAD capture (D-01)** — `src/server.rs:2697-2699`: `repo_dir`
  built and `git_head_oid(&repo_dir)` read BEFORE `src.sync()` at :2700;
  post-sync HEAD read only in the `Ok(())` arm of `seed_vault` (:2708), so
  failed syncs/seedings log nothing. Line-order acceptance criterion holds.
- **Event grouping** — one uuid v4 `event_id` per refresh shared by all rows
  (`record_vault_changes`, server.rs:2647-2663; empty diff → zero rows);
  one event per WebDAV PUT with its own event_id (server.rs:900-909). Stats
  grouping (server.rs:2546-2561) groups consecutive rows sharing event_id
  from the already-newest-first query — correct for sequential writers.
- **SQL injection** — all three store methods (store.rs:1439-1492) use `?`
  bind params only; no `format!`-built SQL anywhere in the touched code.
  The `recent_vault_changes` subquery is slug-scoped on both levels, so a
  colliding event_id can never leak another vault's rows.
- **HTML escaping (T-13-08)** — `ts`, kind label, and path all pass through
  `html_escape` (server.rs:2552-2557, 2570-2573); `n` is a `usize`.
  `html_escape` (server.rs:975-980) covers `& < > "`. Test
  `stats_wiki_changes_escapes_paths` proves the `<script>` payload renders
  escaped.
- **Best-effort (D-04)** — refresh: whole block inside `if let Err(e)` +
  `eprintln!("vault-changes: …")`, `Redirect::to` returned unchanged in every
  branch (server.rs:2705-2717). WebDAV PUT: `tokio::spawn` + swallow inside
  the task, `StatusCode::CREATED` returned unconditionally (server.rs:896-914).
  Stats read: `.unwrap_or_default()` degrades to the empty state
  (server.rs:2539-2543). No path exists where a vault_changes error alters a
  response.
- **Prune window (D-05)** — `DELETE … ts < datetime('now', '-90 days')`
  (store.rs:1487) is byte-identical in shape/window to `prune_access_log`
  (store.rs:1427); boot call `store.prune_vault_changes().await?` sits
  immediately after `prune_access_log` (main.rs:149-150), same `?` discipline.
- **Migration (D-03)** — `migrations/0010_vault_changes.sql` matches the
  locked DDL exactly (event_id, CHECK `('added','modified','removed')`,
  ts default, `idx_vault_changes_slug_ts`); auto-applied via `sqlx::migrate!`.
- **Regressions** — sharee stats branch returns early at server.rs:2479,
  before the section code at :2536+; `stats_sharee_branch_unchanged` proves
  sharees see neither the heading nor the paths. Refresh and PUT responses
  unchanged (e2e tests assert SEE_OTHER / 201). `git show 08e2336 --stat
  -- assets/` → zero CSS changes (D-07 / Phase 9b rule).

## Findings

### LOW-01 — Path normalization corrupts legitimate Linux filenames
`src/server.rs:2639` — `p.to_string_lossy().replace('\\', "/")`. On Linux a
backslash is a valid filename character: a file named `a\b.md` is recorded as
`a/b.md` (wrong path, can collide with a real `a/b.md`). `to_string_lossy`
also silently mangles non-UTF-8 filenames (valid on ext4) into U+FFFD.
Git already emits forward slashes on Unix, so the replace only ever harms.
*Fix:* drop the `.replace('\\\\', "/")` (or gate it under
`#[cfg(windows)]`), and consider `p.to_string_lossy()` alone as acceptable
lossy-but-honest. Display-only impact (path is escaped at render), hence LOW.

### LOW-02 — WebDAV added/modified classification is a non-atomic TOCTOU guess
`src/server.rs:888` — `let existed = tokio::fs::metadata(&full).await.is_ok();`
Two failure modes: (a) a transient metadata error (permissions, EINTR-style
races) on an existing file records `added` instead of `modified`; (b) two
concurrent PUTs to the same new path both see `existed=false` → two `added`
rows. The plan locked the pre-write existence check as the design, so this is
an inherent imprecision of the chosen approach, not a deviation.
*Fix (optional):* `let existed = tokio::fs::try_exists(&full).await.unwrap_or(true);`
— defaulting to `modified` on error biases toward the less-misleading kind.

### LOW-03 — Concurrent same-vault refreshes can split one event into two groups
The stats section groups *consecutive* rows sharing event_id
(`src/server.rs:2546-2561`), and the query orders by `ts DESC, id DESC`
(store.rs:1470-1478). Two refreshes of the same vault running concurrently
interleave their inserts (each row gets `datetime('now')` at insert time), so
one event's rows can become non-contiguous and render as two headers with the
same event_id, and one event's rows can be counted under the other's header.
Requires simultaneous refreshes of one vault — rare, cosmetic, self-heals.
*Fix (optional):* group by event_id with a HashMap preserving first-seen
order instead of consecutive-run grouping.

### INFO-01 — First-ever clone / re-clone records nothing
`git_head_oid` returns `None` when the clone dir is missing or unborn
(server.rs:2608-2613), so the initial import (done at `/vaults/create`) and a
refresh that re-clones a deleted `repos/<slug>` write zero rows — the owner
sees no "added" rows for a whole-tree import. This is exactly the plan's
locked `if let (Some(old), Some(new))` shape (13-01-PLAN Task 2); recorded
here so it is a known, intentional gap, not an oversight.

### INFO-02 — Non-markdown files are recorded
The git diff records every changed path (images, `.gitignore`, `README.md`,
build-adjacent files), while D-01's parenthetical says "markdown pages".
The plan's must_haves ("one row per changed path") locked no filter, so the
implementation follows the plan; flag for the planner that the section may
show non-page noise. *Fix if desired:* filter `ends_with(".md")` in
`record_vault_changes` — one line, but it changes locked behavior.

### INFO-03 — Fire-and-forget ordering and shutdown loss
The WebDAV record (server.rs:900-909) is spawned: its `ts`/`id` are assigned
at insert time, so under scheduling delay two rapid PUTs can land out of PUT
order ("newest first" briefly shows the older event first), and a process
shutdown between response and insert drops the row. Both are inherent to the
plan-mandated `log_web_access`-style fire-and-forget and within D-04's
best-effort contract. No action needed.

### INFO-04 — Refresh response latency grows with diff size
`record_vault_changes` is awaited inline before the redirect
(server.rs:2710-2711): a HEAD move touching N files does N sequential INSERTs
before the response. Response bytes are identical (D-04 holds) and the plan
specified the inline shape; if latency ever matters, spawn it like the WebDAV
hook.

### INFO-05 — Rows of a deleted vault linger until prune
`delete_vault` (store.rs:527) removes only `vaults` rows; `vault_changes`
rows for the dead slug survive up to 90 days. Identical to the existing
`access_log` behavior — consistent, no action.

### INFO-06 — max_rows cap can truncate an event mid-group
The 30-row flat cap (store.rs:1478) can cut a large event's rows, so its
header count shows the truncated count. Within D-06's "~5 events / 30 rows"
discretion. No action.

## Edge cases explicitly checked and found safe

- **Blank paths:** WebDAV rejects empty `rel` before the record
  (server.rs:872-874); git deltas never yield empty paths; DB is
  `path TEXT NOT NULL` regardless.
- **Unborn/missing repo:** `git_head_oid` returns `None` on every failure
  arm (open, head, peel) — no panic path in refresh.
- **Old commit gc'd between refreshes:** `find_commit` fails → `Err` →
  eprintln + swallow; response untouched.
- **Empty tree diff on moved HEAD** (empty commit / message-only change):
  `changes.is_empty()` early-return writes nothing (server.rs:2653-2655).
- **`Delta::Typechange`:** skipped per the plan's "every other status
  skipped" instruction — a file→symlink change logs nothing; accepted.
- **Kind label capitalization:** `label.get_mut(0..1)` (server.rs:2550) is
  total for the CHECK-constrained non-empty kinds.

## Verdict

**status: passed** — ship as-is. LOW-01..LOW-03 are cheap polish for a
follow-up touch; none blocks REQ-CHG-001/002 or violates a locked decision.
