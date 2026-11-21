---
status: passed
phase: 20
score: 5/5 must-haves verified
date: 2026-11-20
verified_head: c97f85a
verifier: gsd-verifier (standard verification practice; `gsd-sdk query agent-skills gsd-verifier` returned empty)
---

# Phase 20 Verification: okf-ecosystem-quick-wins

**Phase goal:** MoriNet's OKF claims are verifiable by third-party tools, and exported bundles open in the ecosystem's standard viewer.

**Verdict: PASSED — 5/5 success criteria verified against the live codebase at HEAD `c97f85a` (all re-runs performed fresh by this verifier, not taken from SUMMARY claims).**

## Per-criterion evidence

### Criterion 1 — Docs vault passes okflint + OKForest CLI with zero errors ✅ VERIFIED

Re-ran both official validators live at HEAD:

```
$ okflint validate --manifest docs/morinet-docs/okf-base.yaml docs/morinet-docs
✅ All files are OKF-conformant.                       (exit 0)

$ npx okforest validate docs/morinet-docs --format json
{"ok": true, "errors": 0, "warnings": 0, "infos": 1, "concepts": 6, "files": 6}   (exit 0)
```

D-04 bar (0 errors AND 0 warnings) met live, matching `20-EVIDENCE.md` (okflint 0.5.1, okforest 0.14.0, run at commit `75a83ae`, content pages unchanged since `598c6b8`). The evidence note includes a scan-genuineness probe (broken scratch copy correctly flagged `F001`), so the clean run is not vacuous. The single `okf/okf-version` **info** (no on-disk root index.md — synthesized at zip time per Phase 18 D-06) is below the bar and documented as an exception in 20-EVIDENCE.md.

### Criterion 2 — Optional check.sh validator step; no GitHub CI ✅ VERIFIED

- `scripts/check-okf.sh` exists, executable; probes `command -v okflint` / `command -v okforest` only — absent → explicit skip note + green; present + failing → hard gate (`set -euo pipefail`, no `|| true`, never auto-installs, never touches network). D-04 warning bar enforced by parsing `--format json` findings with python3 stdlib, not the exit code (per 20-CONTEXT correction).
- Wired into `scripts/check.sh:17` (`bash scripts/check-okf.sh`) between tests and release build.
- Live run: `bash scripts/check-okf.sh` → `okf-check: OK`, exit 0.
- `.github/` untouched by this phase: `git log 598c6b8~1..HEAD -- .github` is empty (last `.github` commit predates the phase).

### Criterion 3 — README conformance badge, honest scope ✅ VERIFIED

- Badge: `README.md:6` — `![OKF v0.2 conformance](https://img.shields.io/badge/OKF%20v0.2-conformant-brightgreen.svg)` (static shields.io, matching License badge style).
- Claim: `README.md:55` — "The built-in docs vault passes **okflint** and **OKForest CLI** with zero errors and zero warnings; validation of the public demo vault is in progress (evidence: `.planning/phases/MORI-20-okf-ecosystem-quick-wins/20-EVIDENCE.md`). This certifies MoriNet's own content, not arbitrary user vaults."
- Scope is honest and backed by criterion 1; demo-vault "in progress" wording matches the user-approved checkpoint decision of 2026-11-20 (Option C) — per instruction, not flagged as a gap. Demo-vault findings and exact upstream fixes are recorded in 20-EVIDENCE.md (D-02 one-off, operator-repo work out of scope).

### Criterion 4 — Export zip contains offline root viz.html; T-18-10 caps enforced ✅ VERIFIED

- `src/okf.rs`: `synthesize_viz_html` (:278) builds a self-contained viewer from vendored assets (`assets/okf-viz/` — official template + viz.css/js + cytoscape@3.28.1 + marked@12.0.0, pinned upstream commit + sha256 table in `PROVENANCE.md`), injected in-memory at zip root only-when-absent (Phase 18 index.md precedent).
- Tests (`cargo test --lib okf::` → **26 passed; 0 failed**):
  - `bundle_contains_synthesized_viz` (src/okf.rs:855): zip contains `viz.html`; concept data + `window.BUNDLE =` present; **`!viz.contains("cdn.jsdelivr.net")`** and cytoscape/marked inlined → fully offline; never written to disk.
  - `vault_viz_wins` (:886): vault's own viz.html ships byte-identical, one viz entry either way.
  - `viz_bytes_count_toward_caps` (:903) + `export_caps_guard_direct` (:934): T-18-10 caps pinned at 20 000 entries / 256 MiB (`MAX_EXPORT_ENTRIES` :135, `MAX_EXPORT_BYTES` :137), viz bytes counted against the budget before write.
  - `viz_escapes_script_breakout` (:916): `<` → `\u003c` blocks stored-XSS via `</script>` breakout.

### Criterion 5 — scripts/check.sh green; existing suite green ✅ VERIFIED

- `bash scripts/check.sh` → full gate (fmt, clippy -D warnings, cargo test, OKF validators, release build) ended with `check: OK`, exit 0.
- `cargo test --lib` → **607 passed; 0 failed; 4 ignored** — exactly matches the orchestrator's measurement at HEAD `c97f85a`.

## Requirement traceability

| Requirement | Status | Evidence |
|---|---|---|
| **OKFE-01** | ✅ Satisfied (docs-vault scope per 2026-11-20 checkpoint) | Criteria 1+2+3: live 0/0 runs by both official validators; `scripts/check-okf.sh` optional gate in `scripts/check.sh`; README badge + scoped claim; no GitHub CI. Demo-vault one-off run recorded in 20-EVIDENCE.md with per-vault findings and upstream fix lists (D-02; operator-repo fixes pending by design, checkpoint-approved). |
| **OKFE-02** | ✅ Satisfied | Criterion 4: root `viz.html` in every export zip, official viewer, offline (no CDN, no build step, no network), caps enforced. |

## Decision compliance (20-CONTEXT.md)

- **D-01** frontmatter on 6 pages, honesty rule: `grep -rn -E '^(verified|status|stale_after):' docs/morinet-docs/src/` → no matches (re-verified live).
- **D-02** demo vault one-off manual validation, no automation — recorded in 20-EVIDENCE.md, check.sh untouched by demo flow.
- **D-03** versioned docs-seed marker (`SEED_MARKER_VALUE` in `src/docs_seed.rs`) drives one-time re-seed.
- **D-04** 0-errors/0-warnings bar enforced via JSON parsing in both evidence and gate.
- **D-05** frontmatter stripped at render via `src/mdbook_preprocessor.rs` (hidden `morinet` subcommand wired in book.toml); `cargo test mdbook_preprocessor` green within the lib suite.

## Minor observations (non-blocking)

- `.planning/REQUIREMENTS.md` still shows OKFE-01/02 as `- [ ]` / "Planned" — checkbox flip is milestone-close bookkeeping, not a functional gap (both requirements are functionally verified above).

## human_verification items

- None required for pass. Operator follow-up (already checkpoint-declared, outside this repo): apply the recorded warning fixes to the upstream demo-vault repos (B3Wiki, ProgWiki) and re-run the one-off validation to extend the claim to the demo vault.
