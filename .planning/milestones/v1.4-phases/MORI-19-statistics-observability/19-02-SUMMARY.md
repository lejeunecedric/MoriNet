---
phase: MORI-19-statistics-observability
plan: 19-02
one_liner: "MCP surface instrumented: truncate_chars (UTF-8-safe 200-char cap) + spawn-based log_activity/log_search_query wired into the three vault-scoped tool wrappers — one mcp_activity row per ACL-passing call (tool, truncated args, status, latency_ms, full resp_size, 200-char excerpt) and one raw search_queries row per searched vault (source='mcp'); denied calls write nothing."
requirements_completed: [STAT-01, STAT-03, STAT-04, STAT-05]
subsystem: mcp
tags: mcp, activity-logging, search-queries, utf8-truncation, best-effort, spawn

requires:
  - phase: 19-01
    provides: mcp_activity + search_queries tables, Store::log_mcp_activity / log_search_query accessors
provides:
  - truncate_chars helper (pub(crate), reusable by 19-03 for REST excerpts)
  - MoriMcp::log_activity + MoriMcp::log_search_query spawn helpers
  - mcp_activity rows for list_notes / read_note / search_notes (D-02 vocabulary)
  - search_queries rows with raw query text, per searched vault (D-04 MCP half)
affects: [19-03-rest-api-logging, 19-04-statistics-page]

key-files:
  created: []
  modified: [src/mcp.rs]

key-decisions:
  - "args capped INSIDE log_activity (truncate_chars(args, 200)) so every call site is UTF-8-safe at the write; excerpt arrives pre-truncated from the call site per plan"
  - "status='error' only for the post-ACL non-UTF-8 read_note decode failure (resp_size 0, excerpt ''); every pre-ACL Err arm writes nothing — no probing trail (T-19-05)"
  - "#[rustfmt::skip] on the log_mcp_activity spawn line to keep the one-line shape pinned by the acceptance grep (rustfmt splits the 9-arg call at max-width 100)"

requirements-completed: [STAT-01, STAT-03, STAT-04, STAT-05]

duration: ~30min
completed: 2026-11-18
---

# Phase 19 Plan 19-02: MCP detailed activity + search-query logging Summary

**MCP surface instrumented: `truncate_chars` (UTF-8-safe 200-char cap) plus spawn-based `log_activity`/`log_search_query` wired into the three vault-scoped tool wrappers — one `mcp_activity` row per ACL-passing call and one raw `search_queries` row per searched vault; denied calls write nothing.**

## Performance

- **Duration:** ~30 min
- **Tasks:** 2 (both TDD)
- **Files modified:** 1 (src/mcp.rs)

## What was built

**Task 1 (`beaef49`)** — `pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> String` above `MoriMcp` (chars().take, never a byte slice; `max_chars 0` → `"…"`), and `fn log_activity(&self, slug, tool, args, status, latency_ms, resp_size, excerpt)` mirroring `log_access`'s spawn-and-drop contract (STAT-05). Wired:
- `read_note`: `t0` before the api call; Ok arm logs tool `read_note`, args `json!({"slug","path"})`, status `ok`, `resp_size = content.len()` (FULL bytes), `excerpt = truncate_chars(&content, 200)`; the post-ACL non-UTF-8 Err branch logs status `error`, size 0, empty excerpt. Pre-ACL Err arms untouched.
- `list_notes`: `t0` before the api call; Ok arm serializes the final `files` ONCE into `text` (reused for the CallToolResult), logs args `json!({"slug","path","type","tags"})`, `resp_size = text.len()`, `excerpt = truncate_chars(&text, 200)`.

**Task 2 (`0e5627b`)** — `fn log_search_query(&self, slug, query)` spawning `store.log_search_query(email, slug, query, "mcp")`. `search_notes`: `t0` before the api call; Ok arm serializes `hits` once into `text`; the existing per-vault fan-out match now calls, per searched vault (Some(slug) → that slug; None → every accessible vault with a source_dir), BOTH the unchanged coarse `log_access(slug, "-")` AND `log_activity(slug, "search_notes", args, "ok", latency, text.len(), truncate_chars(&text, 200))` AND `log_search_query(slug, &p.query)` (raw query, one row per vault — D-04). Err arms untouched.

Existing coarse `log_access` calls stay exactly as they were; `list_vaults` untouched (keeps only its `slug='-'` access_log row — D-02).

## New tests (8, all in `mcp::tests`)

`truncate_chars_short_string_unchanged`, `truncate_chars_ascii_caps_at_max_plus_ellipsis`, `truncate_chars_utf8_boundary_safe` (300×'é' — byte slice would panic), `truncate_chars_zero_max_is_just_ellipsis_or_empty`, `mcp_read_note_logs_activity` (pins tool/status/excerpt-prefix/resp_size=full bytes/latency≥0), `mcp_search_logs_query` (one search_queries row, raw query, source='mcp' + search_notes activity row whose excerpt contains a hit path), `mcp_search_multi_vault_logs_per_vault` (slug=None over 2 vaults → 2 rows), `mcp_denied_call_logs_no_activity` (denied read_note + search_notes → COUNT(*)=0 in BOTH new tables).

## TDD evidence

- **Task 1 RED:** `cargo test --lib truncate_chars` → compile failure, `error[E0425]: cannot find function truncate_chars` ×6. **GREEN:** 8 targeted tests pass (incl. untouched pins `mcp_read_note_logs_access`, `mcp_list_and_search_log`, `mcp_denied_call_logs_nothing`).
- **Task 2 RED:** `mcp_search_logs_query` + `mcp_search_multi_vault_logs_per_vault` FAILED (assertion: no search_queries rows). `mcp_denied_call_logs_no_activity` **passed on write — honest coverage lock** (nothing wrote to the new tables yet; the test pins the no-probing-trail rule that the wiring must preserve). **GREEN:** `cargo test --lib mcp` → 36 passed, 0 failed.

## Verification (plan acceptance criteria + verification block)

- `grep "pub(crate) fn truncate_chars"` ✓ :147; `grep "chars().take"` ✓; `grep "let _ = store.log_mcp_activity"` ✓ :249 (one line); `grep -c truncate_chars` = 14 (≥4).
- `grep log_search_query` → helper + both search call sites; `'"mcp"'` literal at the store call ✓.
- `grep log_activity` → 5 call lines covering exactly the 3 tools (read_note ok+error arms, list_notes, search fan-out ×2 branches); `list_vaults` body: 0 matches ✓.
- `cargo test --lib mcp` 36/36; full `cargo test --lib` → **572 passed / 1 failed / 4 ignored** (564 baseline + 8 new). The single failure was `server::tests::export_waits_for_semaphore_permit` — known flake under parallel load; both export tests pass isolated (2 passed). Not chased per baseline.
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean. No `--no-verify`; nothing pushed; only src/mcp.rs committed.

## Deviations from plan

1. **[Rule 1 - Tooling friction] `#[rustfmt::skip]` on the spawn line** — Found during: Task 1 acceptance gate | The acceptance grep `let _ = store.log_mcp_activity` requires one line, but rustfmt splits the 9-arg call at max-width 100 | Fix: statement-level `#[rustfmt::skip]` with a comment explaining why | File: src/mcp.rs | Verified by `cargo fmt --check` + the grep.
2. **[Rule 3 - Plan-text interpretation] "exactly 3 tool call sites"** — the grep shows 5 `self.log_activity(` lines because read_note has an ok AND an error arm and search_notes has both fan-out branches; the plan's intent (3 tools covered, nothing in list_vaults) holds and is grep-verified.
3. **[Rule 2 - sqlx typing] COUNT queries need `(i64,)` tuples** — bare `i64` doesn't implement `FromRow` (observed as RED-phase compile errors, fixed in the test before GREEN).
4. **Pre-existing flake**: `export_waits_for_semaphore_permit` failed once in the full parallel run, passes isolated — recorded per baseline, not debugged.
5. **STATE.md / ROADMAP.md / REQUIREMENTS.md not touched** — executor instructions forbid staging them; the orchestrator advances plan counters and requirement marks centrally.

## Decisions

- args truncation applied INSIDE `log_activity` (not at each call site) so every present and future call site is capped at the write site (19-01's verbatim requirement).
- `truncate_chars` made `pub(crate)` so 19-03 (REST logging) can reuse the exact same UTF-8-safe cap.
- Search excerpt/resp_size are identical across the fan-out rows (same `text` serialized once) — matches "one insert on the path that already knows the result", no second pass.

## Lessons

- A long best-effort call pinned by an acceptance grep fights rustfmt; `#[rustfmt::skip]` on the single statement is the minimal reconciliation — prefer it over renaming variables into unreadable shapes.
- `sqlx::query_as` single-column results decode as 1-tuples (`(i64,)`, `(String,)`), never bare scalars.
- Poll-loop shape (40×25ms) copied from `poll_log` keeps spawned-row tests race-free without sleeps-per-guess.

## Notes for next executors (verbatim seams)

**src/mcp.rs seams:**

```rust
// above MoriMcp (~:147) — reuse for ANY 200-char cap (19-03 REST excerpts):
pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> String
// ≤max_chars chars → unchanged; else max_chars CHARS + '…' (so a truncated
// excerpt is ≤201 chars); max_chars 0 → "…". Never byte-slices.

// MoriMcp methods (~:229, ~:255) — both spawn + `let _ =` (STAT-05):
fn log_activity(&self, slug: &str, tool: &str, args: &str, status: &str,
    latency_ms: i64, resp_size: i64, excerpt: &str)   // args capped HERE (200)
fn log_search_query(&self, slug: &str, query: &str)   // store.log_search_query(email, slug, query, "mcp")
```

**Row value shapes (what 19-04 will render):**
- `tool` ∈ `list_notes | read_note | search_notes` (schema CHECK).
- `status`: `'ok'` on every ACL-passing Ok arm; `'error'` ONLY for post-ACL non-UTF-8 read_note decode (then `resp_size=0`, `excerpt=""`). Pre-ACL failures write NO row.
- `args`: `serde_json::json!({...}).to_string()` truncated to 200 chars — read_note `{slug,path}`; list_notes `{slug,path,type,tags}`; search_notes `{query,slug,type,tags}`.
- `excerpt`: first 200 chars of the response text + `'…'` if truncated (read_note = note body; list_notes = JSON file list; search_notes = JSON hits, so excerpts contain hit paths/snippets). `resp_size` = FULL byte length before truncation.
- `search_queries.query` = raw user query text (untruncated), `source='mcp'`; one row per searched vault (slug-scoped → 1; global → per accessible vault with a source_dir).

**Wiring points:** list_notes Ok arm (~:314), read_note Ok arm (~:342) + decode-Err arm (~:356), search_notes fan-out match (~:413 Some arm, ~:431 None arm). 19-03: mirror `log_search_query` with source `"api"` in src/api.rs; access_log CHECK already accepts `'api'` (19-01).
