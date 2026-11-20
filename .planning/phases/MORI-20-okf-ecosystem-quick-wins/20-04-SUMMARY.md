---
phase: MORI-20-okf-ecosystem-quick-wins
plan: 04
subsystem: assets/okf-viz
tags: [okf, viz.html, vendoring, provenance, apache-2.0, cytoscape, marked]

requires:
  - phase: 18-okf-export
    provides: OKF bundle export (build_bundle_zip) that will embed these assets
provides:
  - Vendored official OKF viewer asset set under assets/okf-viz/ (template + viewer CSS/JS + cytoscape@3.28.1 + marked@12.0.0)
  - PROVENANCE.md license + sha256 integrity record with pinned upstream commit
  - Coverage-lock + tamper-evidence tests in src/okf.rs for plan 20-05 synthesis
affects: [20-05-viz-html-synthesis]

tech-stack:
  added: []
  patterns:
    - Vendored third-party assets with pinned-commit provenance table + sha256 lock test

key-files:
  created:
    - assets/okf-viz/viz-template.html
    - assets/okf-viz/viz.css
    - assets/okf-viz/viz.js
    - assets/okf-viz/cytoscape.min.js
    - assets/okf-viz/marked.min.js
    - assets/okf-viz/PROVENANCE.md
  modified:
    - src/okf.rs

key-decisions:
  - "Pinned upstream commit 527e81aff55992be21507e97004286789126f88c (main, 2026-11-18) of open-knowledge-format/open-knowledge-format; all viewer files fetched from raw.githubusercontent at that SHA only"
  - "cytoscape@3.28.1 / marked@12.0.0 vendored from cdn.jsdelivr.net — the exact versions the template's CDN tags reference"
  - "Template kept byte-identical to upstream (placeholders + CDN script tags intact); 20-05 substitutes at synthesis time"
  - "Tamper-evidence enforced by a test that recomputes sha256 of each asset and compares against the PROVENANCE.md table"

patterns-established:
  - "Vendored-asset provenance: pinned SHA + sha256 table + license verification per file + update rule, locked by test"

requirements-completed: [OKFE-02]
requirements_completed: [OKFE-02]
one_liner: "Official OKF viewer vendored under assets/okf-viz/ from pinned upstream commit 527e81a (Apache-2.0 headers intact) plus cytoscape@3.28.1/marked@12.0.0, with PROVENANCE.md sha256/license record and two lock tests"

duration: ~65min
completed: 2026-11-20
---

# Phase MORI-20, Plan 04: Vendored OKF viewer assets + provenance Summary

**The official OKF static viewer (template + viz.css + viz.js) is vendored from `open-knowledge-format/open-knowledge-format` at pinned commit `527e81aff55992be21507e97004286789126f88c`, together with the exact CDN libraries it references (cytoscape@3.28.1, marked@12.0.0), under `assets/okf-viz/` with Apache-2.0 headers intact and a PROVENANCE.md license + sha256 integrity record — the complete offline-render-capable asset set plan 20-05 needs.**

## Performance

- **Duration:** ~65 min
- **Started:** 2026-11-20T14:55:00Z (approx)
- **Completed:** 2026-11-20T15:21:52Z
- **Tasks:** 2
- **Files created:** 6 (+1 test module in src/okf.rs)

## Accomplishments
- Five vendored assets in `assets/okf-viz/`: `viz-template.html` (← `src/reference_agent/viewer/templates/viz.html`), `viz.css` + `viz.js` (← `src/reference_agent/viewer/static/`, confirmed as the `__VIZ_CSS__`/`__VIZ_JS__` injection sources by reading upstream `generator.py` at the pinned SHA), `cytoscape.min.js` (365 KB) + `marked.min.js` (35 KB) from jsdelivr at the exact template-referenced versions.
- Template sanity: all four placeholders (`__BUNDLE_NAME__`, `__BUNDLE_DATA__`, `__VIZ_CSS__`, `__VIZ_JS__`) present exactly once; both `cdn.jsdelivr` script tags present (20-05 replaces them); Google LLC Apache-2.0 header intact in template, viz.css, and viz.js.
- License verification: upstream root `LICENSE.md` is the Apache-2.0 text (viewer assets carry the Google LLC Apache-2.0 header); cytoscape.min.js carries the MIT Cytoscape Consortium notice; marked.min.js carries the MIT Christopher Jeffrey banner. MoriNet is `MIT OR Apache-2.0` (Cargo.toml) — vendoring compatible; attribution = retained headers + PROVENANCE.md.
- `PROVENANCE.md` (51 lines): purpose + non-modification statement, license table with verification URLs, provenance table (file | pinned source URL | sha256 verbatim), update rule.
- Two lock tests in `src/okf.rs`: `vendored_okf_viewer_assets_are_complete` (presence + placeholders + header + CDN versions) and `vendored_okf_viewer_provenance_checksums_match` (recomputes sha256 via the existing sha2 dependency and diffs against the PROVENANCE.md table — catches any later tampering in CI).

## Task Commits

1. **Task 1: Fetch and vendor the viewer assets** — `dd84624` (chore; includes coverage-lock test)
2. **Task 2: PROVENANCE.md license + integrity record** — `285bf2f` (docs; includes checksum-lock test)

**Plan metadata:** this SUMMARY commit (docs(20-04)).

## Test evidence

TDD status: both tests are **coverage locks over already-true facts** (assets were fetched before the tests were written) — legitimately GREEN-on-write per the plan's TDD note; no behavior-adding code in this plan, so no manufactured RED.

- `okf::tests::vendored_okf_viewer_assets_are_complete` — GREEN on write: `test result: ok. 1 passed; 0 failed`
- `okf::tests::vendored_okf_viewer_provenance_checksums_match` — first run FAILED on a test-side parsing bug (license-table rows share the `| \`file\` |` prefix; fixed by scoping to the `## Provenance` section, then GREEN). Asset hashes were correct from the start — CLI check confirmed independently: `awk ... | sha256sum -c` → all five `OK`.
- Task 1 acceptance: placeholder grep count 4 (each token ×1); `head -20` shows Apache-2.0 header; `head -c 200` of both .min.js files shows real JS (MIT banners), not HTML error pages; sizes plausible (cytoscape 365 827 B, marked 35 159 B); versions `3.28.1` / `v12.0.0` present in the files.
- Full gate: `scripts/check.sh` exit 0 — **603 passed / 0 failed / 4 ignored** (baseline 601 + 2 new locks). Log: scratch `check-20-04.log`.

## Deviations from Plan

1. **`src/okf.rs` modified** (two lock tests appended to its existing `mod tests`) — not in the plan's `files_modified` list; the orchestrator's TDD instruction required coverage-lock tests for the vendored set, and `okf.rs` is the module plan 20-05 will extend. No behavior change.
2. **cytoscape.min.js is 365 KB, below the plan's "≈ 400-900 KB" plausibility hint.** Verified genuine: fetched from the official jsdelivr distribution of the exact pinned version, starts with the Cytoscape Consortium MIT banner, contains the `3.28.1` version string, and is byte-stable under sha256. The hint was an estimate, not a criterion.

No pre-existing test failures encountered; no other files touched.

## Decisions Made
- Pinned commit `527e81aff55992be21507e97004286789126f88c` (main at fetch time, 2026-11-18T17:00:25Z) recorded in PROVENANCE.md; every upstream file fetched from `raw.githubusercontent.com/.../<SHA>/...` only.
- Upstream layout matched the plan exactly (`templates/viz.html`, `static/viz.css`, `static/viz.js`); generator.py at the pinned SHA confirms the placeholder substitution map, so no path adaptation was needed.
- No generated per-bundle viz.html vendored (per plan instruction — it would embed someone else's bundle data).
- Checksum lock parses only the `## Provenance` section of PROVENANCE.md (license-table rows start with the same file names).

## Lessons
- A markdown-table-parsing test must scope to the right section: two tables in one document can share row prefixes.
- Vendored `.min.js` files were verified by size / `head -c 200` / grep only — never read whole (context discipline).
- jsdelivr serves the exact upstream dist bytes; pinning version + recording sha256 makes the CDN fetch as auditable as the git-commit fetches.

## Next Phase Readiness
- 20-05 (in-bundle viz.html synthesis): asset set complete — template placeholders, injection sources, and offline libraries all vendored and checksum-locked. 20-05 replaces the two `cdn.jsdelivr` `<script>` tags with the inlined library bytes and fills `__BUNDLE_*__` from the export bundle; T-18-10 export caps apply to the resulting ~420 KB + data.

---
*Phase: MORI-20-okf-ecosystem-quick-wins*
*Completed: 2026-11-20*
