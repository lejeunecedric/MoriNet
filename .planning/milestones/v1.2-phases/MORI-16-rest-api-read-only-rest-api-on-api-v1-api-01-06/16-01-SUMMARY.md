---
phase: 16
plan: 16-01
name: shared-read-helpers
status: complete
one_liner: Extracted MCP read bodies into shared pub(crate) helpers in src/api.rs with typed ApiReadError; MCP tools are now thin wrappers.
requirements_completed: [API-02, API-03, API-04, API-05]
commits: [3a8d7fd, ddc89b7]
completed: 2026-11-01
---

# 16-01: shared-read-helpers — SUMMARY

## What was built

- **`src/api.rs` (new, declared `pub mod api;` first in `src/lib.rs`)** — the single-sourced ACL read surface (D-04):
  - `pub(crate) enum ApiReadError { NotFound, InvalidPath, NoSuchFolder, NoSuchNote, Unreadable }` (`Debug, Clone, Copy`) + `pub(crate) fn mcp_message(&self) -> &'static str` returning the EXACT legacy MCP strings.
  - `pub(crate) async fn accessible_vault_entries(store: &Store, email: &str) -> Vec<AccessibleVault>`
  - `pub(crate) fn vault_json(a: &AccessibleVault) -> serde_json::Value` — exactly `slug`/`name`/`scope`/`role`, no envelope (D-08).
  - `pub(crate) async fn source_dir(store: &Store, email: &str, slug: &str) -> Option<PathBuf>` — via `crate::webdav::source_dir_of` (source tree only, D-06); `None` = invisible vault (D-02, 404-not-403 groundwork).
  - `pub(crate) async fn list_notes(store, email, slug, sub: Option<&str>) -> Result<Vec<String>, ApiReadError>` — sorted output (wave-3 cursor pages over this order, D-09).
  - `pub(crate) async fn read_note(store, email, slug, path) -> Result<Vec<u8>, ApiReadError>` — raw bytes for wave-3 `webdav::etag_of`.
  - `pub(crate) async fn search_notes(store, email, query, slug: Option<&str>) -> Result<Vec<crate::mcp::Hit>, ApiReadError>` — cap 50 (D-10).
  - Helpers call `crate::mcp::{safe_join, list_md_files, search_md}` and `crate::mcp::Hit` — those stayed in mcp.rs per plan.
- **`src/mcp.rs` refactored to thin wrappers** — the four `#[tool]` methods now only call their `crate::api::` helper, map `Err(e)` via `tool_error(e.mcp_message())`, and keep `log_access` exactly where it was (best-effort spawn, source `"mcp"`, one row per searched vault on global search). `MoriMcp::source_dir` deleted. Zero test-body edits (`git diff` of the `mod tests` block shows no assertion changes).

## Tests added (src/api.rs `mod tests`)

- `helper_source_dir_invisible_vault_is_none`
- `helper_list_notes_not_found_vs_invalid_path`
- `helper_read_note_maps_reasons`
- `helper_search_scoped_to_accessible`
- `helper_vault_json_field_names`

## RED/GREEN evidence

- **Task 1 RED** (tests written first, helpers absent): `cargo test --lib helper_` → `error[E0433]: cannot find type 'ApiReadError' in this scope` … `error: could not compile 'morinet' (lib test) due to 23 previous errors`.
- **Task 1 GREEN**: `cargo test --lib helper_` → `test result: ok. 5 passed; 0 failed`.
- **Task 2 baseline (before refactor)**: `cargo test --lib mcp` → `test result: ok. 16 passed; 0 failed`.
- **Task 2 GREEN (after refactor, same command set)**: targeted 6 ACL tests `ok. 6 passed; 0 failed`; `cargo test --lib mcp` → `16 passed; 0 failed` — no test bodies touched.
- **Full suite**: `cargo test --lib` → `367 passed; 0 failed; 4 ignored` (baseline 366 + 5 new = 371 total; the 4 ignored are pre-existing mdbook-corpus oracles).
- `cargo clippy --all-targets`: zero warnings. `cargo fmt --check`: clean. `git diff Cargo.toml Cargo.lock`: empty (D-05).

## Deviations

None. All plan acceptance criteria met as written.

## Decisions

- **REST access logging skipped in v1** (planner decision inherited from plan `<context>`): `migrations/0008_*.sql:27` CHECK-constrains `access_log.source` to `('web','mcp')`; a `'api'` value needs a table-rebuild migration for a feature no API-0x requirement asks for, and reusing `'mcp'`/`'web'` would misattribute REST traffic. MCP's own `log_access` untouched.
- **`Unreadable` string** = `"could not read the note"` — the verbatim string from the legacy `read_note` read-failure branch (mcp.rs:253). The helper returns bytes; the MCP wrapper maps non-UTF-8 `String::from_utf8` failure through `ApiReadError::Unreadable.mcp_message()` so the string is single-sourced.
- **Global-search access logging** in the wrapper re-lists accessible entries and filters by `source_dir_of` to reproduce the legacy "one row per searched vault" behavior exactly (logging only — no ACL resolution duplicated).

## Lessons (for wave 2/3 executors)

- Error enum: `crate::api::ApiReadError::{NotFound, InvalidPath, NoSuchFolder, NoSuchNote, Unreadable}`; `mcp_message()` gives the MCP strings; REST must map `NotFound` → 404 problem+json (D-02), `InvalidPath` → 400/404, `NoSuchFolder`/`NoSuchNote` → 404.
- Helper signatures (all `pub(crate) async` in `src/api.rs`, first two args always `(&Store, &str email)`): `source_dir(store, email, slug) -> Option<PathBuf>`, `list_notes(store, email, slug, Option<&str>) -> Result<Vec<String>, ApiReadError>`, `read_note(store, email, slug, path) -> Result<Vec<u8>, ApiReadError>`, `search_notes(store, email, query, Option<&str>) -> Result<Vec<crate::mcp::Hit>, ApiReadError>`, `accessible_vault_entries(store, email) -> Vec<AccessibleVault>`, `vault_json(&AccessibleVault) -> serde_json::Value`.
- `read_note` returns **bytes** — feed straight into `webdav::etag_of`; do not re-read the file in REST handlers.
- `list_notes` output is sorted — the D-09 cursor (`skip_while(|p| p <= cursor)`) depends on it.
- Do NOT re-implement vault resolution in REST handlers; call these helpers (threat model B1).
