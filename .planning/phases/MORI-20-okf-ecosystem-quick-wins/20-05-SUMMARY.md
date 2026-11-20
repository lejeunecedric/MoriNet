---
phase: MORI-20-okf-ecosystem-quick-wins
plan: 05
subsystem: src/okf.rs (bundle export)
tags: [okf, viz.html, bundle-export, cytoscape, offline, xss-escape, t-18-10]

requires:
  - phase: 18-okf-export
    provides: build_bundle_zip walk, index.md only-when-absent precedent, check_export_caps
  - phase: 20-04
    provides: vendored viewer assets under assets/okf-viz/ (template + viz.css/js + cytoscape@3.28.1 + marked@12.0.0)
provides:
  - Every exported OKF bundle carries a self-contained root viz.html (official viewer, fully offline) unless the vault ships its own
  - synthesize_viz_html / viz_edges / resolve_md_link helpers in src/okf.rs
affects: []

tech-stack:
  added: []
  patterns:
    - Compile-time include_str! of vendored assets; placeholder substitution with drift-bail on missing literals
    - JSON embedded in <script> escaped `<` → \u003c to block </script> breakout

key-files:
  created: []
  modified:
    - src/okf.rs

key-decisions:
  - "Data shape adapted to what viz.js actually reads (plan-sanctioned): {name, types, nodes:[{data:{id,label,type,color,size,description?,tags?}}], edges:[{data:{source,target}}], bodies:{id: markdown}} — NOT the plan's minimal {concepts, edges} sketch; viz.js consumes cytoscape element objects plus a types list and a bodies map"
  - "Concept id = bundle-relative path minus .md (matches viz.js resolveTarget); root index.md/log.md excluded as reserved (scan_conformance precedent); untyped pages included with type 'untyped'"
  - "Substitution order: CSS → JS → CDN tags → BUNDLE_NAME → BUNDLE_DATA last, so vault-controlled bytes are never scanned for further placeholders"
  - "Node color: FNV-1a over type into an 8-color palette (deterministic, no dep); size 26 + min(degree,8)*2"
  - "Bundle name = source dir file name (no slug parameter by design; stated in code comment)"

patterns-established:
  - "In-memory synthesized zip entry: build bytes → check_export_caps(entries+1, bytes+len) BEFORE start_file — same caps discipline as walked files (T-18-10)"

requirements-completed: [OKFE-02]
requirements_completed: [OKFE-02]
one_liner: "build_bundle_zip now synthesizes the official offline OKF viewer (vendored template + inlined cytoscape/marked + injected graph JSON with < escaped) at the zip root, only-when-absent (vault's own viz.html wins), caps-guarded — 4 new tests, suite 607/0/4"

duration: ~50min
completed: 2026-11-20
---

# Phase MORI-20, Plan 05: In-bundle viz.html synthesis Summary

**Every exported OKF bundle now contains a root `viz.html` — the official OKF graph viewer, fully self-contained (cytoscape@3.28.1 + marked@12.0.0 inlined, bundle graph injected as `window.BUNDLE`), synthesized in-memory only when the vault ships no viz.html of its own, with T-18-10 caps counting its bytes.**

## Performance

- **Duration:** ~50 min
- **Tasks:** 2
- **Files modified:** 1 (src/okf.rs)

## Task Commits

1. **Task 1+2 tests (RED first)** — `da324cb` (test(20-05))
2. **Task 1 implementation** — `e71dd2f` (feat(20-05); amended once — see Deviations)

**Plan metadata:** this SUMMARY commit (docs(20-05)).

## Test evidence (TDD)

**RED** (tests written first, observed failing on `da324cb`):
- `okf::tests::bundle_contains_synthesized_viz` — FAILED (no viz.html entry)
- `okf::tests::vault_viz_wins` — FAILED (no viz.html entry)
- `okf::tests::viz_escapes_script_breakout` — FAILED (no viz.html entry)
- `okf::tests::viz_bytes_count_toward_caps` — passed on write (asserts existing `check_export_caps` semantics the injection reuses; first draft had a test-side bug — bytes did not cross the cap — fixed before the RED commit)

**GREEN** (after implementation):
- `cargo test --lib okf::tests::` → `test result: ok. 26 passed; 0 failed`
- Full suite: `cargo test` → **607 passed / 0 failed / 4 ignored** (baseline 603 + 4 new)
- `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt` applied
- Gate: `scripts/check.sh` exit 0 (log: scratch `check-20-05.log`)

**End-to-end spot-check** (temporary dump test, removed after): 3-page vault (2 types, relative + root-relative links) → zip entries `[beta.md, alpha.md, gamma.md, index.md, viz.html]`; viz.html 420 425 B; `cdn.jsdelivr.net` count 0; placeholder tokens 0; `window.BUNDLE` JSON parses: name = source dir name, types `[Guide, Reference]`, edges include `alpha→gamma` (root-relative `/gamma.md`) and `beta→alpha`; bodies map complete; HTML skeleton intact.

## Acceptance criteria verification

- `viz.html` injection guarded by `has_viz` + `check_export_caps(entries + 1, bytes + viz.len())` before `start_file` ✔
- Synthesized output: zero `cdn.jsdelivr.net`, inlined `cytoscape`/`marked` markers present ✔
- `</script>` breakout in a title appears only as `\u003c/script>` (escaped `<`; `>` is harmless without `<`) ✔ pinned by `viz_escapes_script_breakout`
- `git diff Cargo.toml` empty — no new crates ✔
- Server export-route tests (server.rs ~:10621+) use `names.contains(...)` — no exact-entry-count assertions to update ✔

## Deviations from Plan

1. **Data shape adapted** (plan explicitly allows: "read viz.js first; adapt"): viz.js reads `bundle.types`, cytoscape element objects `bundle.nodes`/`bundle.edges` (`{data:{...}}`), and `bundle.bodies[id]` — the final injected document is `{name, types, nodes, edges, bodies}` (see key-decisions), not the plan's `{concepts, edges}` sketch.
2. **Feat commit amended once** (`8bcd2e0` → `e71dd2f`): the commit chain ran despite a failing test because of a pipeline exit-code masking (`| grep`); the two fixes (test expected-string for the `<`-only escape — the plan escapes `<` but not `>`; clippy dead-store on post-guard counter bumps) were folded into the feat commit via amend. Nothing was pushed.
3. **Escaped form is `\u003c`-only** (no `\u003e`): sufficient and per plan — a `</script>` breakout is impossible without a literal `<`.

No pre-existing test failures encountered; no files outside src/okf.rs touched.

## Lessons

- `path.find(':')?` inside an Option-returning resolver silently rejects every colon-less input — scheme checks must be `if let Some(...)`.
- Pipeline exit codes: `cmd | grep` returns grep's status; a chained commit after a failing test still runs. Check the real exit code or run unpiped before committing.
- The vendored template's JS is the contract for the injected JSON shape — read viz.js before fixing the data schema.

## Next Phase Readiness

- OKFE-02 complete: bundles open in the ecosystem's standard viewer offline straight out of the zip (success criterion 4).
- 20-06 (demo vault section) can rely on the export emitting viz.html; demo vault already carries its TODO(20-06) marker from 20-02.

---
*Phase: 20-okf-ecosystem-quick-wins*
*Completed: 2026-11-20*
