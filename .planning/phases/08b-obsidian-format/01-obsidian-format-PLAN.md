# Phase 08b — obsidian-format (the real input)

> Plan 01 — In-pipeline llm-wiki/Obsidian → mdBook converter.
> Status: 🟡 in progress. Decisions locked with the user on 2026-10-21.

## Why this phase exists

MoriNet's vault contract was mdBook-native (`book.toml` + `src/SUMMARY.md`), but the
real input is **always an llm-wiki / Obsidian vault** (Karpathy layout: `entities/`,
`concepts/`, `comparisons/`, `queries/`, `raw/`, `index.md`, `SCHEMA.md`, `log.md`).

Measured on the local corpus `ProgWiki` (real vault, same format as kodiawiki):
131 `.md`, **0 `book.toml`**, **812 `[[wikilinks]]`** (542 with `|alias`), YAML
frontmatter on every page. mdBook renders `[[…]]` as literal text → broken links.
The earlier raw-wiki import was reverted on purpose (`2b924f3`); this is the
proper feature.

## Locked decisions (user, 2026-10-21)

| # | Decision |
|---|----------|
| D1 | **In-pipeline converter** in MoriNet (`AD-14`): sync transforms the Obsidian tree → a generated mdBook tree, then `mdbook build` runs on the *generated* tree. mdBook stays upstream (`AD-2` respected). No preprocessor plugin, no custom renderer. |
| D2 | **Detection inverted** (`PRD-6`): MoriNet only loads llm-wiki vaults. Absence of `book.toml`/`src/SUMMARY.md` → llm-wiki pipeline. A vault matching *neither* format gets the admin-visible warning: `LLM-Wiki not detected: this vault might not work as expected`. |
| D3 | **Wikilink resolution à l'Obsidian**: `[[slug]]` → first `**/slug.md` match (root first, then shallowest, then alphabetical, case-insensitive fallback). `[[target\|Alias]]` → `[Alias](rel/path.md)`. `[[page#Heading]]` → anchor kebab-cased to mdBook ids. `![[img.png]]` → image embed. **Broken link → inert text `<span class="wikilink-missing">`, never a 404.** |
| D4 | **Frontmatter is consumed**: `title` (or first H1) → SUMMARY entry title; `type`/`tags`/`created`/`updated` → meta block at the top of the rendered page; everything else stripped from the body. |
| D5 | **SUMMARY.md regenerated at every sync**: tree by folder, alphabetical, `index.md` as "Home". So notes created via WebDAV `PUT` appear without manual step. |

## Conversion contract (the mechanics)

- The **source vault is never modified**. Generated artifacts go to
  `<vault-src>/.morinet-build/` (mirrored tree + `book.toml` + `src/SUMMARY.md`),
  and mdBook writes its output to `<vault-src>/.morinet-build/book/` —
  that becomes `vaults.book_path`.
- The mirror copies **all** content files (`.md` rewritten, `assets/` and other
  binaries verbatim) so relative links and embeds resolve.
- **Excluded** from scan/mirror: `.obsidian/`, `.git/`, `_archive/`,
  `.morinet-build/`, dotfiles.
- WebDAV `PUT` writes markdown to the **source**, then rebuild = convert + build
  (dispatch on D2 detection, `server.rs`).
- Frontmatter subset parser in pure Rust (no new YAML dep) — the llm-wiki SCHEMA
  is a closed contract (flat `key: value`, inline lists `[a, b]`, bare dates).
  Block lists (`- item`) tolerated for Obsidian compat. No comment stripping, no
  YAML typing. Malformed frontmatter → treated as absent (graceful).
- Code-fence-aware rewriting: ``` fences, inline code spans and HTML comments are
  never rewritten (ProgWiki's `raw/` contains literal `[[…]]`).

## Bites (TDD, strict)

> **Status**: bite 1 (frontmatter) ✅ `c2e6270` — 8 tests + corpus oracle 127/127.
> bite 2 (links) ✅ `d242e4d` — 9 tests + corpus oracle: 0 `[[` left outside code,
> 809 resolved, 4 inert-missing on ProgWiki. Lesson captured: assets must resolve
> by bare filename too (Obsidian), pages win ties.
> bite 3 (tree) ✅ `76aad86` — SUMMARY/book.toml/meta-block generators, 5 golden tests.
> bite 4 (convert+detect) ✅ — mirror into `.morinet-build/`, source untouched;
> **real mdbook 0.5.4 oracle caught 2 generator bugs** (`multilingual` removed in
> 0.5; SUMMARY draft-parents illegal → generated `_section.md` folder notes).
> Built corpus: 139 HTML, 0 broken links. 161 tests.

| Bite | Deliverable | Acceptance |
|------|-------------|------------|
| 1 | `obsidian/frontmatter.rs` — `split_frontmatter` + getters (`title`, `type`, `tags`, dates, `confidence`) | unit tests on corpus-shaped samples: flat, quoted, inline list, block list, absent, unterminated, CRLF, body `---` untouched |
| 2 | `obsidian/links.rs` — slug index + fence-aware wikilink rewriter (alias, `#section`, embeds, broken → inert span) | unit tests incl. `[[a/b]]` path targets, ties resolved shallow-first, 812-link corpus page rewritten 100% (no leftover `[[`), fenced code preserved |
| 3 | `obsidian/tree.rs` — SUMMARY.md + `book.toml` + meta-block HTML generation | golden tests: Home=index.md, sections per folder, titles from frontmatter/H1/stem fallback |
| 4 | `obsidian/convert.rs` — whole-dir convert `source → .morinet-build/` | tempdir integration tests (dir named per test — pitfall #16): mirror + exclusions + rewritten links + assets copied verbatim |
| 5 | wiring: D2 detection in `sources.rs` sync + `server.rs` rebuild dispatch + admin warning badge | **route-level** e2e (pitfall #14): obsidian vault in tempdir → `GET /vault/<slug>/` serves resolved HTML; neither-format vault → admin shows the warning string |
| 6 | E2E browser pass + deploy: copy of a real llm-wiki, local `mdbook build` (v0.5.4 present), Chromium walkthrough, screenshots, styled meta block + inert broken links; then tag + release + deploy | every page styled (`AD-12` checklist), CI green, v0.1.8 live |

## Out of scope (deliberately, this phase)

- Provenance markers `^[raw/articles/…]` rendered as links — follow-up.
- `_archive/` surfaced in the UI — it stays invisible.
- Native-mdBook vaults get no behaviour change (they keep their own SUMMARY/book.toml).
- REQ-OBS-* visual mods (zoom/fold/switcher) remain Phase 11.

## Follow-ups noticed while building

- `index.md` sections ("Every new page must be added to index.md") duplicate what
  SUMMARY now generates automatically — could become auto-generated too later.
