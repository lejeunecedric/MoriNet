# 12-03 Summary — serve-time display options: two-pass inject_vault_chrome (REQ-VOPT-001, REQ-VOPT-002)

**Status:** complete. Full gate `./scripts/check.sh` exit 0 — **306 passed** (296 baseline + 10 new), 4 ignored, clippy `-D warnings` clean, release build OK.
**Commits:** `d46709d` feat(12-03) two-pass restructure + unit tests · `3649f16` feat(12-03) route e2e tests

## What was built

### Task 1 — two-pass `inject_vault_chrome` (`src/server.rs`)
- New raw-string consts (pitfall #25, no interpolation): `PAGE_NUMBERS_OVERRIDE` (exact D-09 style `<style>.chapter-item a > strong[aria-hidden="true"]{display:inline;}</style>`), `THEME_LIGHT_LITERAL` (`default_light_theme = "light";`), `THEME_DARK_LITERAL` (`default_dark_theme = "navy";`).
- New pure helper `apply_vault_display_options(html: &mut String, keep_page_numbers: bool, theme: &str)`: keep-on inserts the override before the LAST `</body>` (only if one exists); non-empty theme exact-`replace`s each of the two literals with the chosen theme (whitelist-bounded at write time by 12-02), guarded by `str::contains` first (T-12-13), inert when literals absent (T-12-11).
- `inject_vault_chrome(bytes, keep_page_numbers, theme) -> Vec<u8>` restructured into two distinct passes: non-UTF-8 → bytes unchanged (unchanged); **PASS 1** chrome injection — the old `zoom.css`/`morinet.js` early-return became a skip-condition (no longer returns from the function), `rfind("</body>")` insert otherwise; **PASS 2** option pass, runs for ALL HTML pages including converted vaults.
- Both `serve_vault_file` call sites (dir→index branch and file branch, text/html gate kept) pass `vault.vault.keep_page_numbers` + `&vault.vault.theme` — no side query (12-02 fields ride on the `Vault` row).
- NOT touched: `assets/zoom.css`, `src/obsidian/tree.rs` (`git diff --stat` empty); `zoom_css_hides_mdbook_chapter_numbers` green (D-12). No rebuild, no book.toml change (D-08/D-13).

### Task 2 — route e2e tests (real router, oneshot, admin cookie)
Fixture: `MDBOOK_PAGE` const — realistic built-mdBook page (`<html class="light ...">`, inline theme script with both literals verbatim, `chapter-item` + `<strong aria-hidden="true">1.</strong>` markup, `</body></html>`), written per-test into a unique tempdir book dir (`seed_options_vault`).
- `serve_page_numbers_off_hides_by_default` — default vault: no override in served HTML (D-09/D-12 "off" state).
- `serve_page_numbers_on_injects_override` — `set_keep_page_numbers(slug, true)`: override present, positioned AFTER chrome injection and BEFORE `</body>` (both passes coexist).
- `serve_theme_rewrite_applied` — `set_theme(slug, "rust")`: both literals now `rust`, originals gone; covers BOTH `/vault/{slug}/` (index branch) and `/vault/{slug}/index.html` (file branch).
- `serve_theme_unset_leaves_page_untouched` — theme `""`: both original literals intact.
- `serve_converted_vault_page_still_gets_options` — page already wiring `theme/morinet.js` (early-return path): theme rewrite + override still applied, chrome not double-injected.

## Tests added (10, TDD-red first)
Unit: `display_options_theme_rewrite_applied`, `display_options_theme_rewrite_inert_when_absent`, `display_options_page_numbers_style_injected_before_body_end`, `display_options_off_is_inert`, `display_options_runs_on_chrome_early_return_page`.
Route: the five `serve_*` tests above.

## Deviations
1. **Pass order swapped vs the plan's suggestion.** The plan text suggested PASS 1 = options, PASS 2 = chrome, but its own acceptance criterion requires the override to appear "AFTER any chrome injection and before `</body>`" — with options first, chrome would land between the override and `</body>`. Implementation runs chrome first, options second, satisfying the acceptance criterion and the two-pass requirement (option pass still runs for every page regardless of the chrome skip). Semantics identical.
2. **Commit split:** each task landed as one `feat(12-03)` commit (tests + code together, TDD), same pattern as 12-02's deviation 1.
3. **Route tests log in as admin** (`login_cookie`) instead of seeding a personal owner — admin access to any vault is the established pattern (`admin_reads_vault_files`); authorization paths are 12-01/earlier territory and unchanged (T-12-14: call sites changed only in data passed, not in auth).

## Threat model disposition
T-12-10 mitigated (fixed-literal substitution; only the quoted value inside the two assignments mutates; value whitelist-bounded at write time by 12-02; no path surface). T-12-11 mitigated (exact full-assignment match; inertness unit-proven). T-12-12 mitigated (static raw-string const, byte-identical on every page). T-12-13 mitigated (two `contains` guards before any `replace`; non-UTF-8 returns untouched; no regex). T-12-14 accepted (scope unchanged). T-12-SC accepted (no new deps). No new threats found.

## For 12-04+ / ship
- D-13: GitHub issue #7 closes at 1.1.0 ship (post-deploy), not here.
- The option pass is content-type-gated only where the file branch already gates on `text/html`; the dir→index branch always treats the file as HTML (pre-existing behavior).
