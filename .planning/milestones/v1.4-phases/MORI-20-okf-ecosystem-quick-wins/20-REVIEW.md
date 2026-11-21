---
status: issues_found
phase: 20
date: 2026-11-20
reviewer: gsd-code-reviewer (senior-review practice; gsd-sdk agent-skills query returned empty)
range: 2e80d37..HEAD -- ':(exclude).planning'
verified_against:
  - "cargo test --lib: 607 passed / 0 failed / 4 ignored (matches 20-05-SUMMARY claim)"
  - "bash scripts/check-okf.sh exit 0 with okflint 0.5.1 + okforest 0.14.0 installed: 0 errors / 0 warnings (D-04 bar live-verified)"
  - "sha256sum of all five vendored assets matches PROVENANCE.md table exactly"
---

# Phase 20 (okf-ecosystem-quick-wins) — Code Review

## Findings

| id | severity | file:line | finding | suggested fix |
|----|----------|-----------|---------|---------------|
| 20-R1 | HIGH | `assets/okf-viz/viz.js:263-265` (invoked by `src/okf.rs` `synthesize_viz_html`) | **Render-time XSS in synthesized viz.html — the `\u003c` guard only covers script-breakout, not innerHTML.** `viz_json_escape` correctly prevents `</script>` breakout at document-parse time, but `\u003c` decodes back to `<` when JS parses the object literal, and viz.js then renders markdown bodies with `marked.parse(body, {gfm:true})` → `bodyEl.innerHTML = html` with **no sanitizer**. marked v12 passes raw HTML through verbatim (empirically confirmed with the vendored `marked.min.js`: `<img src=x onerror=alert(1)>` in a `.md` body emerges intact; `[x](javascript:alert(2))` yields a live `javascript:` href that `rewriteInternalLinks` leaves untouched — it only rewrites resolvable `.md` targets). Exported bundles are shareable artifacts: a malicious vault → victim opens `viz.html` → arbitrary script executes on node click (and on the auto-shown first node, viz.js:349-351). The SUMMARY's "script-breakout escaped / T-20 XSS guard" claim is accurate but materially narrower than the perceived protection. | Sanitize at synthesis or render: (a) append a small post-VIZ_JS shim that wraps `showDetail`'s body assignment through a sanitizer (bundle DOMPurify, or a marked renderer override that escapes raw-HTML tokens and neutralizes non-`http(s)`/non-relative hrefs); or (b) strip/escape raw-HTML blocks in `VizConcept.body` in Rust before injection. If upstream-only fix is chosen, document the residual risk in PROVENANCE.md and stop claiming blanket XSS safety in summaries/tests. |
| 20-R2 | MEDIUM | `src/docs_seed.rs:134-139` | **Marker-missing + stale-tree case skips the forced re-seed, then bakes the marker.** `seed_docs_vault` treats "marker absent" as first boot and calls `write_source_tree(dest, false)`, which never overwrites existing files. If a v1 seed failed after writing the tree but before writing the marker (e.g. mdbook missing at first boot — the build runs before `fs::write(&marker, …)` at line 148), an upgraded v2 binary finds no marker, leaves the stale v1 pages/book.toml in place, builds successfully, and writes marker `"2"` — the D-03 re-seed never happens and the docs stay permanently stale (no frontmatter, no preprocessor wiring) with a marker claiming current. | Decide re-seed by tree state, not marker presence alone: `if src_dir.join("book.toml").exists() { reseed_source_tree(..) } else { write_source_tree(.., false) }` — a present tree with a non-current marker (stale OR missing) forces. |
| 20-R3 | LOW | `src/docs_seed.rs:66-71` (`seeded_book_toml`) | The `current_exe` path is interpolated into the TOML string with no escaping. A binary path containing `"` or a newline produces broken or injectable TOML (arbitrary keys in the seeded `book.toml`). Practically benign (whoever controls the install path already controls execution), but the doc-comment only reasons about whitespace, not quoting. | Escape the path (`replace('"', "\\\"")`) or bail/fall back to the literal when the path contains `"`/control chars. |
| 20-R4 | LOW | `scripts/check-okf.sh:38-49` | Warning enforcement depends on okforest's JSON keeping a top-level `findings[]` with per-item `severity`. A schema change (renamed key, findings omitted when clean-but-warned) makes `bad` empty → warnings silently pass while exit stays green (errors still fail via exit code). The observed 0.14.0 output also carries `summary.errors`/`summary.warnings` counters — a second, cheaper assertion. | Also assert `data["summary"].get("warnings", 0) == 0` (and `errors == 0`), and fail loudly if neither `findings` nor `summary` is present rather than treating an unknown schema as clean. |
| 20-R5 | LOW | `src/okf.rs:441` (`id = rel.trim_end_matches(".md")`) | `trim_end_matches` removes *repeated* suffixes: `a.md.md` → id `a.md` (diverges from viz.js `resolveTarget`, which strips exactly one `.md` via `slice(0,-3)`), and a file literally named `.md` yields an empty-string id. Cosmetic graph weirdness only, but the two resolvers should agree by construction. | Use `strip_suffix(".md")` once and skip entries whose id would be empty. |

## Areas reviewed with no findings

- **`src/mdbook_preprocessor.rs`** — protocol handling correct (`supports` probe exits 0 to avoid mdBook's silent-skip; Book *object*, not array, on stdout); `strip_frontmatter` is byte-conservative (exact `---` fences, CRLF-safe, unterminated fence untouched, thematic breaks preserved); recursion into `sub_items` covers both build paths; invalid stdin fails loudly. Good test coverage.
- **Export path traversal / symlinks** — canonicalize-and-stay-under-root, symlink skip (no follow), dot-file exclusion, non-file exclusion all intact and unchanged by this range; synthesized `viz.html`/`index.md` names are fixed literals, not vault-derived.
- **Caps discipline (T-18-10)** — `check_export_caps(entries+1, bytes+viz_len)` is checked *before* `start_file` for the in-memory entry, matching walked-file semantics; comment about counters having no later consumers is correct.
- **`viz_json_escape` (script-breakout scope)** — within its stated scope it is correct: `<` → `\u003c` is a valid JSON escape, serde_json already escapes `"`/control chars, single-line JSON output means `-->`/`<!--` script-data states can't trigger. (Residual render-time hole is 20-R1, not the escape itself.)
- **Placeholder substitution order** — vault-controlled data substituted last; `viz_replace` bails on template drift instead of silently emitting raw placeholders.
- **`scripts/check-okf.sh` shell hygiene** — `set -euo pipefail`, quoted paths, JSON piped via stdin into `python3 -c` (no shell re-evaluation), no `|| true` swallowing, no network access, fail-closed on validator presence. Live run green.
- **Vendored asset integrity / licenses** — all five sha256s match PROVENANCE.md (independently recomputed); lock test parses the provenance table correctly (post-`## Provenance` split avoids the license-table rows); attribution headers retained.
- **`viz_edges` / `resolve_md_link` robustness** — no panic paths (ASCII-pattern slicing on char boundaries, `pop()?` guards `..` past root, scheme detection matches viz.js regex); dedup + self-edge exclusion correct.
- **Secrets** — none introduced (no tokens/keys/credentials in any changed file).
- **Race conditions** — seed runs once at boot before serve; marker write ordering gap is 20-R2 (logic, not concurrency).
- **README badge claims (20-06)** — "zero errors and zero warnings" for the docs vault re-verified live with both validators; demo-vault validation explicitly *not* claimed (pending operator, per approved checkpoint); docs-vault-only scope stated in the badge paragraph.
- **Test-suite claim** — `607 passed; 0 failed; 4 ignored` reproduced exactly.

## Verdict

`issues_found` — 1 HIGH, 1 MEDIUM, 3 LOW. The HIGH (20-R1) does not invalidate the shipped script-breakout guard but shows the XSS surface is wider than the guard's claim; recommend fixing at synthesis time (Rust-side body sanitization or an injected sanitizer shim) before advertising the in-bundle viewer to untrusted-vault sharing.
