---
plan: 18-06
phase: 18
name: mcp-type-tags-filters
one_liner: "MCP list_notes/search_notes gain optional type/tags filters applied strictly post-ACL (D-08, OKF-04)"
requirements_completed: [OKF-04]
status: complete
commit: 5f601de
date: 2026-11-06
---

# 18-06 Summary — MCP type/tags filters (post-ACL) + milestone closeout

## What was built (src/mcp.rs only)

- **Param structs** (`ListNotesParams`, `SearchParams`): each gained
  `#[serde(default)] pub r#type: Option<String>` and
  `#[serde(default)] pub tags: Option<Vec<String>>` with the plan's
  doc-comments, which schemars 1 publishes as schema descriptions via the
  existing `#[tool]` path — no router changes needed.
- **`async fn note_matches(store, email, slug, path, type_f, tags) -> bool`**
  (module-level helper, doc-comment documents T-18-15/16/17): no filter ⇒
  true; `crate::api::read_note` Err ⇒ false; `crate::okf::frontmatter_of`
  None ⇒ false; else `type` exact match against `fm.page_type()` AND `tags`
  ANY-match against `fm.list("tags")`. No unwrap in the filter path.
- **Filter placement**: inside the `Ok(files)` / `Ok(hits)` arms of the
  `list_notes` / `search_notes` wrappers, AFTER the ACL-gated
  `crate::api::list_notes` / `crate::api::search_notes` call and before
  serialization; plain for-loop collecting matches (futures-free). Search
  hits use each hit's own `slug`. Error arms and `log_access` semantics
  untouched (log the search, not each filtered file).

## TDD evidence

- **RED**: tests written first — `cargo test --lib mcp` failed to compile
  with 20 × E0560 ("struct `mcp::ListNotesParams` has no field named
  `r#type`" / same for `tags` in both structs).
- **GREEN**: `cargo test --lib mcp` → **24 passed / 0 failed** (17
  pre-existing MCP tests + 7 new: `list_notes_filter_by_type`,
  `list_notes_filter_by_tags_any`, `list_notes_filter_type_and_tags_and`,
  `list_notes_no_filters_unchanged`, `search_notes_filter_by_type`,
  `mcp_filters_never_resurrect_inaccessible_vault`,
  `mcp_filter_params_schema_exposes_type_and_tags`).
- ACL-first proof (T-18-15): bob (no grant) calling list/search on a foreign
  vault with filters gets the byte-identical `tool_error` as without
  filters — asserted by string equality, no existence oracle.

## Closeout results (task 2)

- `scripts/check.sh` end-to-end: **check: OK** (fmt, clippy -D warnings,
  full `cargo test`, release build).
- Full suite: `cargo test --lib` → **538 passed / 0 failed** (baseline 531
  + 7 new; ≥ 493 requirement met).
- D-09 pin: the assertion `"frontmatter must not survive into src"` lives
  inside test `convert_builds_a_complete_mdbook_tree` (convert.rs:329-331)
  — that test is green; `git diff --exit-code src/obsidian/convert.rs`
  clean (converter contract untouched).
- Dependency audit **snapshot at commit 5f601de**: Cargo.toml has **zero**
  new runtime deps from 18-01..18-06 (`grep -c '^zip' Cargo.toml` = 0).
  The zip dependency + comment rescope (D-06) is owned by plan 18-04, which
  runs AFTER this plan per the orchestrator — the "one new dep (zip)" final
  audit and the `git log` zip-commit check are deferred to the orchestrator's
  final gate after 18-04.
- Planning-gate sanity: `grep -ho "D-0[1-9]\|D-10" *-PLAN.md | sort -u`
  covers all of D-01..D-10.
- Process: no `--no-verify`, nothing pushed, commit contains only src/mcp.rs.

## Deviations

1. **Pre-existing test bodies edited (minimal, forced).** The plan's
   acceptance said "no edits inside pre-existing test bodies", but adding
   fields to `ListNotesParams`/`SearchParams` breaks the 4 existing struct
   literals (`tools_are_acl_scoped`, `mcp_list_and_search_log`,
   `mcp_sharee_sees_shared_vault`, `mcp_search_finds_notes_in_shared_vault`).
   Each got exactly `r#type: None, tags: None` — no assertion or semantics
   changed; all pass unmodified in behavior.
2. **`map_or(true, …)` → `is_none_or`.** Clippy 1.99 `unnecessary_map_or`
   (under `-D warnings`) rejected the plan's literal `map_or` snippet;
   replaced with the suggested `is_none_or` — same semantics.
3. **Zip-related closeout items deferred** (see closeout): 18-04 lands zip
   after this plan; documented rather than pre-empted.
4. **D-09 "pin test name"** is an assert message, not a test name; verified
   via its host test `convert_builds_a_complete_mdbook_tree`.

## Decisions

- `note_matches` as a free module-level `async fn` (plan's signature), not a
  `MoriMcp` method — keeps the wrappers thin and testable.
- Filter short-circuit: when both filters are `None` the wrapper skips the
  per-file read entirely (zero cost on the no-filter path — regression test
  `list_notes_no_filters_unchanged`).
- Schema test asserts property presence + string/array typing loosely
  (serialized-substring check) so schemars 1's `["string","null"]` union
  encoding doesn't make the test brittle.

## Lessons

- The editor lint hook runs bare `rustc` at edition 2015 and floods
  E0670/`async` errors on every patch of this crate — always noise; trust
  `cargo` instead.
- MCP filter fixtures work by writing frontmatter files into the tmpdir
  `src` **before** `seed_vault(slug, name, &src.join("book"))` (source dir
  is referenced, not copied) — matches the 18-05 fixture note.
- Adding fields to schemars param structs is source-breaking for struct
  literals in tests; budget those two-line edits into "unmodified tests"
  acceptance criteria in future plans.

## notes_for_next_executor (18-04: zip export route)

- `crate::mcp::list_md_files` is `pub(crate)` and `crate::okf::frontmatter_of`
  is available if the export needs per-note metadata; both proven stable here.
- 18-04 WILL add the `zip` dep (default-features=false, deflate) — the final
  dependency-budget audit (exactly one new runtime dep + rescoped Cargo.toml
  comment, D-06) belongs to the orchestrator's gate AFTER 18-04.
- The MCP tool schemas now include `type`/`tags` on list/search — if the
  export route mirrors MCP filters, reuse the same semantics (exact type
  match, ANY tags match, no-frontmatter never matches).
- Full-suite baseline after this plan: **538 passed / 0 failed / 4 ignored**.
