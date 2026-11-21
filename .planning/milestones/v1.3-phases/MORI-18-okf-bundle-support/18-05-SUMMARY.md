---
one_liner: "?meta=1 opt-in frontmatter metadata on /api/v1 note list/read + additive okf field on the vaults endpoint"
requirements_completed: [OKF-04]
status: complete
commit: a6a3965
---

# 18-05 Summary — api-v1 meta opt-in + vault okf fields

## What was built

All changes in `src/api.rs` only (guard `git diff --exit-code Cargo.toml Cargo.lock src/mcp.rs src/server.rs` clean).

**Query param parsing**
- `ListQuery` gained `pub(crate) meta: Option<String>` (raw string, same discipline as `limit` — no extractor rejection).
- New `pub(crate) struct ReadQuery { pub(crate) meta: Option<String> }`; `api_read_note` and `read_note_route` gained a `Query(q): Query<ReadQuery>` extractor.
- Truthy test is strictly `q.meta.as_deref() == Some("1")` — absent, `"0"`, `"yes"` etc. all take the untouched default path (D-07 byte-identical guarantee).

**Response shapes**
- Note list `?meta=1`: entries become `{"path": …, "frontmatter": {type, tags, status, trust_tier} | null}` inside the unchanged `{notes, has_more, next_cursor}` envelope. Each entry re-reads via the SAME ACL-gated `read_note` helper; `crate::okf::frontmatter_of` → `note_meta`, `None`/read-error ⇒ `frontmatter: null` (never an error-shape change). Cost bounded by page size (T-18-14, documented in comment).
- Note read `?meta=1`: JSON `{"frontmatter": … | null, "size": bytes.len(), "etag": …}` with the ETag header still set; the existing 404 collapse runs FIRST (T-18-12 — inaccessible vaults never reach the meta code); If-None-Match/304 handling stays only on the default raw path (doc-comment states meta responses are not cache validators).
- `/api/v1/vaults`: per-slug `store.vault_okf()`; key `okf` inserted ONLY when the stored string parses as JSON (parsed OkfStats object). Never-scanned or corrupt JSON ⇒ key absent entirely — no null-valued okf (D-07b, T-18-03 defensive parse). `vault_json` (:63-70) untouched — still shared with mcp.rs.
- Docs: `api_index` endpoint strings and `/api/v1/docs` gained a "Metadata opt-in" mention (string edits only).

## TDD evidence

- **RED**: tests written first; `api_list_notes_meta_objects` and `api_vaults_okf_field_when_scanned` failed (meta param ignored, no okf key); ACL/default tests passed trivially as expected pre-implementation.
- **GREEN**: `cargo test --lib api` → 60 passed / 0 failed. Full suite: **531 passed / 0 failed / 4 ignored** (baseline 522 + 9 new tests; D-10 intact).
- Nine new tests: `api_list_notes_default_shape_unchanged`, `api_list_notes_meta_objects`, `api_read_note_default_raw_markdown`, `api_read_note_meta_json`, `api_meta_inaccessible_vault_404` (T-18-12 leak test — list AND read, identical 404 problem+json with meta), `api_meta_param_non_one_is_default`, `api_vaults_okf_field_when_scanned`, `api_vaults_no_okf_key_when_never_scanned`, `api_vaults_okf_field_absent_on_corrupt_json`.
- Pin test at :1149 (`j["notes"] == json!(["a.md","b.md"])`) UNTOUCHED (git diff shows no edit inside it) and green; read-shape pin (`api_note_read_serves_raw_source`) green unmodified.
- Gates: `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean.

## Deviations

- None from the plan's discretion decisions. One transient fix during implementation: the list meta branch was first written as `.iter().map(|p| … .await …)` which cannot compile (await inside sync closure) — converted to a plain `for` loop before any test ran; no behavioral difference.

## Decisions

- Meta list entries built with a sequential `for` loop over the page (bounded by limit ≤ 200) — no parallelism needed, keeps the same ACL helper call order.
- Meta read response carries the ETag header AND the `etag` body field (plan's exact field set); content-type is `application/json` via axum `Json`.
- Docs strings mention `?meta=1` on both note endpoints and the vaults okf addition; kept the bare-object/no-envelope style.

## Lessons

- A `.map()` closure cannot `.await` — for per-item async work in axum handlers use a `for` loop (or `futures::join_all`, unnecessary at ≤200 items).
- The patch tool's per-file lint runs standalone rustc (edition 2015 default) and floods E0670 noise on this crate — ignore it; `cargo test`/`cargo clippy` are the real gates.
- `vault_okf` stores the raw string; the API layer parses defensively to `serde_json::Value` rather than `OkfStats`, so future OkfStats field additions flow through additively without an api.rs change.

## notes_for_next_executor

- **18-06 (mcp.rs filters)**: `crate::api::list_notes` / `search_notes` are unchanged — apply type/tags filters AFTER the ACL call inside the MCP wrappers, per 18-PATTERNS §8. Do NOT reuse `ListQuery`/`ReadQuery` for MCP params. `vault_json` remains the shared bare-field helper (mcp.rs:175 still uses it).
- **18-04 (export route)**: the `?meta=1` branches live inside `api_list_notes` / `api_read_note` AFTER the shared ACL helpers and 404 collapse — if export reuses those handlers, decide explicitly whether meta applies there; if it reuses the helpers (`list_notes`/`read_note`), ACL is already the same code path. `crate::okf::frontmatter_of` + `note_meta` is the single source of the frontmatter field set — reuse it, don't re-derive trust tiers.
- Test fixtures: `seed_notes_app(tag, slug, files)` returns `(app, store, src_dir)` — write frontmatter files into `src_dir` after seeding; `seed_vault` has no app. `mint_token` + `get` helpers are in the api tests mod.
