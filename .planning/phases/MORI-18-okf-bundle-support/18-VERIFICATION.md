---
status: PASSED
verified_at: 2026-11-06
head: 68d0a391d498b56a47cd8b384e1f040814e46bff
verifier: goal-backward code inspection + targeted test runs (summaries treated as claims only)
---

# Phase 18 Verification — okf-bundle-support

## Test evidence (run at HEAD)

| Command | Result |
|---|---|
| `cargo test --lib` (full) | **548 passed / 0 failed / 4 ignored** (≥ 493 baseline, D-10) |
| `cargo test --lib okf` | 36 passed / 0 failed |
| `cargo test --lib export` | 5 passed / 0 failed |
| `cargo test --lib meta` | 6 passed / 0 failed |
| `cargo test --lib mcp` | 24 passed / 0 failed |
| `cargo test --lib bundle` | 9 passed / 0 failed |

## Requirements

| Criterion | Evidence | Verdict |
|---|---|---|
| **OKF-01** detection at refresh/register, persisted, best-effort | `record_okf_scan` src/server.rs:3146 (is_dir guard :3155, single `set_vault_okf` write only after complete scan). Six call sites: server.rs:3216 (git refresh), :3301 (personal load), :3653 (upload-register), :3843 (finish_upload_update), :4022 (admin create) + main.rs:226 (GitHub sync, only on seed success). All wrapped `if let Err(e) = … eprintln!` — redirects byte-identical, never blocks. Persistence: `migrations/0013_okf_conformance.sql` (`ALTER TABLE vaults ADD COLUMN okf_json TEXT`), store helpers `set_vault_okf`/`vault_okf` src/store.rs:976/:987. Tests: server.rs:9934/:9963/:10019 (hook, missing-source skip, rerun) | ✅ |
| **OKF-02** badge per D-05 | `okf_badge_html` src/server.rs:530: exact strings `OKF v{v} — {concepts}/{total} concepts` (em-dash U+2014) / `not an OKF bundle` / empty for unscanned; serde-defensive (unparseable ⇒ empty, test :10058). Rendered on home cards via `vault_row` `{b}` interpolation (server.rs:570, :619; call sites :641/:660) and stats panel `okf_line` (server.rs:2886-2908). Zero new CSS (`.menu-btn` pill). Tests: :10039-:10088 | ✅ |
| **OKF-03** export zip route | Route registered server.rs:258. Handler `okf_export` :3021: `access_for` read gate (Err ⇒ invisible), store-error/unknown/no-access/missing-source all collapse to identical 404 "vault not found" — never 403 (T-18-09). `build_bundle_zip` src/okf.rs:158: byte-identical entries (`fs::read` + `write_all`, test `bundle_zip_entries_byte_identical` :423), dot-dirs skipped (:181-185, test :442), symlink-escape traversal guard via canonicalize+starts_with (:188-194, test :481), caps 20 000 / 256 MiB checked on metadata **before** buffering (:207-208, constants :134-136, test :491), virtual root `index.md` with `okf_version: "0.2"` only when absent (:216-219, tests :457/:470 — never written to disk). Tests: `export_zip_requires_access` :10096, `export_zip_unknown_vault_404` :10119, `export_zip_streams_bundle` :10136, `export_link_in_stats_panel` :10171 | ✅ |
| **OKF-04** API/MCP metadata | Default shapes byte-identical: pin test `api_notes_pagination_round_trip` (api.rs:1216, bare-array assert :1228) and `api_note_read_serves_raw_source` (:1397) — `git diff 2485600^..HEAD -- src/api.rs` shows **no edits inside either**; only additions were doc strings + vaults-handler loop. `?meta=1` list → `{path, frontmatter:{type,tags,status,trust_tier}|null}` inside existing envelope (api.rs:449-466); read → `{frontmatter,size,etag}` JSON + ETag header (:545-561); non-"1" ⇒ default (:1225 test). Vaults endpoint gains additive `okf` only when valid stats exist (api.rs:330-332; tests :2156/:2187/:2211 incl. corrupt-JSON absence). MCP filters: `note_matches` mcp.rs:165 applied **inside the Ok arm after** ACL-gated `crate::api::list_notes`/`search_notes` (:237/:298); ACL leak test `mcp_filters_never_resurrect_inaccessible_vault` mcp.rs:854; schema test :911 | ✅ |

## Locked decisions

| Decision | Evidence | Verdict |
|---|---|---|
| **D-01** scope A+D+C, no rendering | No mdBook/template rendering changes; `git diff 2485600^..HEAD -- src/obsidian/convert.rs` empty | ✅ |
| **D-02** same hook as change-capture, source dir, best-effort | Six call sites above; `source_dir_of` + `is_dir` guard (server.rs:3150-3157); failure leaves previous stats untouched (single write after complete scan) | ✅ |
| **D-03** §11 conformance + version guess + persistence | src/okf.rs:47-87: threshold `concepts*100 >= total*80` (:74, tests `scan_below_80_percent_is_not_okf` :283, `scan_exactly_80_percent_is_okf` :296); reserved root-only `rel == "index.md"/"log.md"` (:54-61, presence-only, excluded from denominators — test :334); v0.2 keys `["generated","verified","sources","status","stale_after"]` (:16) checked only on typed concepts; else v0.1; None below threshold. OkfStats persisted as JSON text (migration 0013) | ✅ |
| **D-04** reuse flat parser, no YAML | Only addition to frontmatter.rs is `pub fn has()` (presence-only, `fields.contains_key`); no YAML dep in Cargo.toml diff | ✅ |
| **D-05** badge strings, no new CSS | See OKF-02 row; exact strings pinned by tests :10044/:10052 | ✅ |
| **D-06** export contract + one archive crate | See OKF-03 row; `zip = { version = "9", default-features = false, features = ["deflate"] }` Cargo.toml:50 | ✅ |
| **D-07** additive meta, byte-identical defaults, trust tiers | See OKF-04 row; trust tier `trust_tier` okf.rs:94-105 (unverified / machine-confirmed / human-reviewed, tests :351-:378); no-frontmatter ⇒ null (`frontmatter_of` :120, test :398) | ✅ |
| **D-08** MCP type/tags post-ACL | See OKF-04 row; schemars exposure via doc-comments on `r#type`/`tags` params (mcp.rs:122-140), test :911 | ✅ |
| **D-09** converter untouched | `git log 2485600^..HEAD -- src/obsidian/convert.rs` → 0 commits; `git diff` empty; pin assertion `"frontmatter must not survive into src"` intact at convert.rs:328-331 inside `convert_builds_a_complete_mdbook_tree` (:304), green in full suite | ✅ |
| **D-10** baseline + one new dep | Cargo.toml diff adds exactly one dep (`zip`); `cargo tree -e normal | grep -iE 'bzip2|zstd|xz|lzma|bzip3'` → empty; Phase 17 comment rescoped to Phase 17+18 in same hunk; suite 548 ≥ 493 | ✅ |

## Gaps / notes

**No blocking gaps.** Informational notes:

- **[info] Pre-existing MCP test bodies edited (18-06 deviation #1):** four existing tests got `r#type: None, tags: None` struct-literal fields — forced by the param-struct change, no assertion/semantics altered; all green. Accepted deviation.
- **[info] `record_okf_scan` is `pub` not `pub(crate)`** (plan said `pub(crate)`) — forced: main.rs is a separate binary crate. Documented in 18-03 summary.
- **[info] D-09 "pin test name"** is an assert message inside `convert_builds_a_complete_mdbook_tree`, not a standalone test — verified via the host test.

## Verdict

**PASSED** — all four requirements (OKF-01..04) and all ten locked decisions (D-01..D-10) are satisfied by the code at HEAD 68d0a39, with 548/0/4 tests green and the v1.2 API/converter pin tests provably unmodified.
