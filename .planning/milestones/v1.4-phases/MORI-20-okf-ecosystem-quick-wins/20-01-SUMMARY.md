---
phase: MORI-20-okf-ecosystem-quick-wins
plan: 01
subsystem: docs/infra
tags: [okf, mdbook, preprocessor, frontmatter, seed-marker, rust]

requires:
  - phase: 18-okf-export
    provides: OKF v0.2 conformance machinery (scan_conformance, bundle export)
provides:
  - OKF v0.2 frontmatter on all six built-in docs pages (B3Wiki house pattern, honesty rule)
  - morinet mdbook-okf-preprocessor subcommand stripping frontmatter at render time
  - docs vault book.toml wired to the preprocessor (absolute current_exe at seed time)
  - versioned docs-seed marker (SEED_MARKER_VALUE "2") driving one-time re-seed on upgrade
affects: [20-02-validator-runs, 20-06-readme-badge, docs-seed, mdbook-build-paths]

tech-stack:
  added: []
  patterns:
    - mdBook preprocessor subcommand embedded in the server binary (no extra deployment)
    - versioned seed marker (content compare, not existence)

key-files:
  created: [src/mdbook_preprocessor.rs]
  modified:
    - docs/morinet-docs/src/quickstart.md
    - docs/morinet-docs/src/config-reference.md
    - docs/morinet-docs/src/webdav.md
    - docs/morinet-docs/src/mcp.md
    - docs/morinet-docs/src/sharing-and-roles.md
    - docs/morinet-docs/src/faq.md
    - docs/morinet-docs/book.toml
    - src/docs_seed.rs
    - src/main.rs
    - src/lib.rs

key-decisions:
  - "Preprocessor is a hidden morinet subcommand wired via the docs vault book.toml (D-05 option a); not optional — absence fails the build loudly"
  - "Seed-time rewrite of the preprocessor command to absolute current_exe so both mdbook build paths resolve it regardless of service PATH"
  - "Re-seed deletes stale compiled book/ because sources.rs Native build has a skip-when-compiled fast path"

patterns-established:
  - "Versioned seed marker: bump SEED_MARKER_VALUE when embedded first-party content changes; stale marker forces one overwrite re-seed"
  - "Coverage-lock test over include_str! content for docs frontmatter shape"

requirements-completed: [OKFE-01]
requirements_completed: [OKFE-01]
one_liner: "Six docs pages carry OKF v0.2 frontmatter; a first-party mdbook-okf-preprocessor subcommand strips it at render time on both build paths; the docs-seed marker is versioned to \"2\" forcing one re-seed per install"

duration: ~35min
completed: 2026-11-20
---

# Phase MORI-20, Plan 01: OKF docs frontmatter + render-time stripper + re-seed Summary

**Six built-in docs pages now carry OKF v0.2 frontmatter (type/title/description/generated, no trust fields), a first-party `morinet mdbook-okf-preprocessor` subcommand strips it at render time on both mdbook build paths, and the docs-seed marker is versioned to "2" so every existing install re-seeds once at next boot.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-11-20T15:35:00+01:00 (approx)
- **Completed:** 2026-11-20T16:11:25+01:00
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments
- All six docs pages open with OKF v0.2 frontmatter per the B3Wiki house pattern (D-01); zero `verified`/`status`/`stale_after` emitted (honesty rule); SUMMARY.md byte-identical.
- `src/mdbook_preprocessor.rs`: pure `preprocess([context, book] bytes) -> Book-object bytes` + hidden clap subcommand `mdbook-okf-preprocessor`; `supports <renderer>` probe exits 0 (silent-skip pitfall closed); malformed stdin fails the build loudly (silent-leak pitfall closed). Covers BOTH build paths (sources.rs refresh, server.rs WebDAV-PUT rebuild) via the docs vault book.toml.
- Seed marker is versioned (`SEED_MARKER_VALUE = "2"`): stale ("1") or missing markers trigger one forced re-seed — embedded pages overwrite operator edits, the stale compiled `book/` is deleted before rebuild, and the seeded book.toml carries the absolute `current_exe` preprocessor command (D-03, D-05).

## Task Commits

1. **Task 1: OKF frontmatter on six docs pages** — `598c6b8` (feat; includes coverage-lock test)
2. **Task 3: mdBook preprocessor subcommand + book.toml wiring** — `38b9893` (feat; unit tests included)
3. **Task 2: versioned seed marker — RED tests** — `b9376fe` (test)
4. **Task 2: versioned seed marker — GREEN** — `6871f44` (feat)

**Plan metadata:** this SUMMARY commit (docs(20-01)).

## Test names + RED/GREEN evidence

TDD (behavior-adding tasks):

- **Task 1** `docs_seed::tests::docs_pages_carry_okf_frontmatter` — coverage lock written FIRST:
  - RED: `test result: FAILED. 0 passed; 1 failed` (pages had no frontmatter)
  - GREEN: `test result: ok. 3 passed; 0 failed` (docs_seed module after frontmatter added)
- **Task 2** `docs_seed::tests::stale_marker_triggers_reseed`, `docs_seed::tests::seeded_book_toml_uses_absolute_preprocessor_command`:
  - RED: compile failure — `cannot find value SEED_MARKER_VALUE`, `cannot find function marker_is_current / reseed_source_tree`, `write_source_tree takes 1 argument but 2 supplied` (11 errors, committed as `b9376fe`)
  - GREEN: `test result: ok. 5 passed; 0 failed; 1 ignored` (docs_seed module)
- **Task 3** `mdbook_preprocessor::tests::{frontmatter_is_stripped, page_without_frontmatter_is_unchanged, unterminated_fence_is_unchanged, nested_sub_items_are_stripped, output_is_book_object_not_array, invalid_json_fails_loudly, supports_probe_is_detected}`:
  - RED (no-op stub): `test result: FAILED. 1 passed; 6 failed`
  - GREEN: `test result: ok. 7 passed; 0 failed`

End-to-end verification (mdbook v0.5.4):
- `morinet mdbook-okf-preprocessor supports html` → exit 0
- `mdbook build docs/morinet-docs` green; rendered HTML contains no `generated:` (excluding mdBook's own banner) and no `type: Guide`; quickstart body text renders
- Preprocessor absent from PATH → `mdbook build` fails loudly (exit 101, "command wasn't found"; no `optional = true`)

Full gate: `scripts/check.sh` exit 0 — **601 passed / 0 failed / 4 ignored** (baseline 591 + 10 new tests).

## Files Created/Modified
- `docs/morinet-docs/src/{quickstart,config-reference,webdav,mcp,sharing-and-roles,faq}.md` — OKF v0.2 frontmatter prepended (type/title/description/generated{by: process:morinet-docs, at: '2026-11-19T00:00:00Z'})
- `docs/morinet-docs/book.toml` — `[preprocessor.okf-frontmatter] before = ["links"] command = "morinet mdbook-okf-preprocessor"` (not optional)
- `src/mdbook_preprocessor.rs` (new) — pure strip core + unit tests
- `src/main.rs` — hidden `mdbook-okf-preprocessor` subcommand (supports probe → exit 0; stdin→preprocess→stdout)
- `src/lib.rs` — register `pub mod mdbook_preprocessor;`
- `src/docs_seed.rs` — SEED_MARKER_VALUE "2", `marker_is_current`, `write_source_tree(dest, force)`, `reseed_source_tree` (forced write + stale book/ delete), seed-time absolute-command rewrite of book.toml, updated module docs and tests

## Deviations from Plan

1. **Task order executed 1 → 3 → 2.** Task 2's acceptance criterion "seeded book.toml carries the absolute current_exe path" requires the `[preprocessor.okf-frontmatter]` literal that Task 3 adds to book.toml; running Task 3 first avoided a circular dependency. No other change.
2. **`src/lib.rs` modified** (module registration for `mdbook_preprocessor`) — not in the plan's files_modified list; unavoidable for a new first-party module.
3. **Task 3 RED stub was not committed separately** — the no-op stub existed only transiently between the RED run (6 failures recorded above) and the GREEN implementation, both inside commit `38b9893`. Task 2 follows the full test-commit/feat-commit split.

No pre-existing test failures encountered; no other files touched.

## Decisions Made
- Frontmatter values exactly as the plan's DECIDED VALUES (actor `process:morinet-docs`, fixed `at: '2026-11-19T00:00:00Z'`, per-page types Guide/Reference/FAQ); descriptions verified against each page's first paragraph — suggested wording fit all six.
- `strip_frontmatter` requires the opening `---` at byte 0 of the chapter content and a later exact-`---` line; unterminated fences and mid-content `---` (thematic breaks) are left byte-identical.
- Re-seed force path is reachable only via `seed_docs_vault` on the DOCS_SLUG path (`reseed_source_tree` is private) — threat-model requirement that forced overwrite can never touch non-docs vaults.
- `seeded_book_toml` falls back to the literal command if `current_exe()` errors (documented; mdBook whitespace-splitting caveat noted in code comment).

## Issues Encountered
- Clippy `ptr_arg` on `&mut Vec<Value>` → signature changed to `&mut [Value]` (fixed before commit).
- Borrow error in the coverage-lock test (temporary in filter expression) → bound `pages()` to a local first.

## Next Phase Readiness
- 20-02 (validator clean runs): docs vault is now an OKF-frontmattered bundle on disk; frontmatter stays on-disk for okflint/OKForest while rendered HTML is clean.
- 20-06 (README badge): precondition (frontmattered docs vault) shipped.
- Note for operators: first boot of an existing install after this release performs one forced docs re-seed (operator edits to the docs vault are intentionally overwritten, D-03).

---
*Phase: MORI-20-okf-ecosystem-quick-wins*
*Completed: 2026-11-20*
