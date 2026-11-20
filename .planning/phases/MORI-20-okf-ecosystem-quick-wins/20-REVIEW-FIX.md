---
status: all_fixed
phase: 20
findings_fixed: [20-R1, 20-R2, 20-R3, 20-R4, 20-R5]
date: 2026-11-20
---

# Phase 20 — Review Fix Report

Baseline: 607 passed / 0 failed / 4 ignored @ 7cf8fff.
Final gate: `scripts/check.sh` exit 0 — **611 passed / 0 failed / 4 ignored**
(607 baseline + 4 new regression tests); `cargo clippy --all-targets -- -D warnings`
clean. Log: scratch `check-20-fix.log`. All five review premises reproduced
against the code before fixing (viz.js confirmed: labels/descriptions are
`textContent`-rendered at viz.js:180-182; only `body` reaches
`marked.parse` → `innerHTML` at viz.js:263-265 — so body-only sanitization
is sufficient and the existing `viz_escapes_script_breakout` title test stays
valid).

## 20-R1 (HIGH) — render-time XSS in synthesized viz.html — commit `0489653`

Premise verified: marked v12 passes raw HTML and `javascript:` hrefs through
to `innerHTML`; `viz_json_escape`'s `\u003c` only guards script-breakout at
document-parse time and decodes back to `<` for JS.

Fix (Rust synthesis path, `src/okf.rs`; vendored assets untouched, no new
dependencies): new `sanitize_viz_body()` applied to `VizConcept.body` before
injection. Two deterministic transforms:
1. escape every `<` that starts an HTML tag / comment / PI (followed by ASCII
   letter, `/`, `!`, `?`) to `&lt;` — marked then emits inert text, never live
   markup;
2. rewrite inline link targets `](...)` whose scheme (whitespace/control-char
   stripped, case-insensitive — defeats `JaVaScRiPt:` variants) is in
   `{javascript, data, vbscript, file}` to `blocked:`.
Documented side effect: a literal `<` inside fenced/inline code renders as
`&lt;` — accepted trade-off for a shareable offline viewer.

Test: `viz_sanitizes_markdown_bodies` (body with `<img onerror>`, `<script>`,
`javascript:`/`JaVaScRiPt:`/`data:` links; asserts no decodable `\u003cimg`/
`\u003cscript` survives, payloads neutralized to `blocked:`). Observed RED
("img tag not escaped"), then GREEN.

## 20-R2 (MEDIUM) — marker-missing + stale tree bakes stale docs — commit `12257fb`

Premise verified: `seed_docs_vault` branched on `marker.exists()`; a crashed
first boot (tree written, build failed before the marker write) took the
non-forcing write, built the stale tree, then stamped the marker current.

Fix: extracted `seed_tree(src_dir)`; decision is now TREE STATE — a present
`book.toml` with a non-current marker (stale OR missing) forces
`reseed_source_tree`; only a genuinely absent tree takes the plain first-boot
write (per the review's suggested fix).

Test: `missing_marker_with_existing_tree_forces_reseed` (tree + stale page +
stale book/, no marker → forced re-seed restores embedded pages, deletes
stale book). Observed RED against the old branch logic (assertion failure),
then GREEN.

## 20-R3 (LOW) — unescaped current_exe path in seeded book.toml — commit `9d06da4`

Fix: `seeded_book_toml_with()` escapes `\` → `\\` and `"` → `\"` before
interpolating the path into the TOML basic string; `seeded_book_toml` delegates
with `current_exe`. Test: `seeded_book_toml_escapes_special_chars_in_exe_path`
(path with `"` and `\` → escaped output, result parses via `toml::Value`,
`None` falls back to the literal). Test added in the same commit as the
one-line fix (LOW finding, labeled honestly).

## 20-R4 (LOW) — check-okf.sh warning enforcement single-point-of-failure — commit `aec8291`

Fix: the python assertion now (a) also fails when `summary.errors` or
`summary.warnings` is non-zero, and (b) fails closed when neither `findings`
nor `summary` is present, instead of treating an unknown schema as clean.
Verified: synthetic payloads — clean → exit 0; `summary.warnings=1` → exit 1;
`{}` → exit 1 (fail-closed); findings-warning → exit 1; `summary.errors=2` →
exit 1. Live `bash scripts/check-okf.sh` with okforest 0.14.0: exit 0 (one
info finding passes). Script-level verification in lieu of a Rust test (shell
gate script; no test harness for it in the suite).

## 20-R5 (LOW) — trim_end_matches strips repeated `.md` — commit `407a047`

Fix: `rel.strip_suffix(".md")` once, entries whose id would be empty skipped
(dot-file exclusion already prevents a bare `.md`, so the guard is defensive).
Test: `viz_id_strips_single_md_suffix` (`a.md.md` → id `a.md`, viz.js
`resolveTarget` parity). Observed RED, then GREEN (test in same commit as the
one-liner).

## Commits

- `0489653` fix(20): 20-R1 sanitize synthesized viz bodies at synthesis time
- `12257fb` fix(20): 20-R2 decide docs re-seed by tree state, not marker presence
- `9d06da4` fix(20): 20-R3 TOML-escape the current_exe path in the seeded book.toml
- `aec8291` fix(20): 20-R4 assert okforest summary counters and fail closed on unknown schema
- `407a047` fix(20): 20-R5 strip exactly one .md suffix for viz concept ids

Files touched: `src/okf.rs`, `src/docs_seed.rs`, `scripts/check-okf.sh`.
Vendored `assets/okf-viz/*` untouched — provenance sha256 lock test still
passes. Not pushed.
