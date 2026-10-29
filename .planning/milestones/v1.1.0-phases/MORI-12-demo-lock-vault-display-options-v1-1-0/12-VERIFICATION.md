---
phase: MORI-12-demo-lock-vault-display-options-v1-1-0
verified: 2026-10-28
status: passed
score: "10/10"
plans: [12-01, 12-02, 12-03]
---

# Phase 12 Verification — Demo Lock + Vault Display Options

Goal-backward verification against the actual tree at `06fd08c`. Full suite
executed live: **`cargo test` → 306 passed, 0 failed, 4 ignored** (12.56s).
Every must_have truth below was proven by a green test and/or live code
inspection (file:line). The serve-time theme approach superseding the ROADMAP's
book.toml approach is a LOCKED user decision (D-08..D-10) — not flagged as a gap.

## ROADMAP criteria

| Criterion | Verdict | Evidence |
|---|---|---|
| REQ-DEMO-001 — locked accounts refused server-side on load/delete (+edit/share) with clear error page | ✅ | `locked_account_create_refused`, `locked_account_delete_refused`, `locked_account_edit_submit_refused`, `locked_account_share_add_refused`, `locked_account_share_revoke_refused` — all green (tower `oneshot` scripted POSTs, zero JS) |
| REQ-DEMO-002 — home popup "demo account can't add or suppress vaults" | ✅ | `home_page_shows_locked_popup_script` green; exact string at `src/server.rs:1948` inside `LOCKED_ACCOUNT_JS` |
| REQ-VOPT-001 — per-vault keep-page-numbers toggle, default hidden | ✅ | `serve_page_numbers_off_hides_by_default` + `serve_page_numbers_on_injects_override` green; migration default 0 |
| REQ-VOPT-002 — per-vault mdBook theme select, serve-time application (supersedes book.toml per D-08/D-10 — LOCKED) | ✅ | `serve_theme_rewrite_applied`, `serve_theme_unset_leaves_page_untouched`, `serve_converted_vault_page_still_gets_options` green |

## Per-truth evidence table

| # | must_have truth | Evidence |
|---|---|---|
| 1 | `[security] locked_accounts` + `MORINET_LOCKED_ACCOUNTS` env override resolve into `RuntimeConfig.locked_accounts`, env wins (D-01) | `config::tests::resolve_locked_accounts_env_overrides_file_section`, `resolve_locked_accounts_defaults_empty`, `resolve_locked_accounts_file_section_fallback` green; `src/config.rs:16` (const), `:44` (`SecuritySection`), `:186-191` (env-wins resolve) |
| 2 | Locked non-admin refused server-side on POST /vaults/create, /vaults/{slug}/delete, /vault/{slug}/edit, share add/revoke — scripted POST without JS still refused (D-02/D-03) | `is_locked` guard at `src/server.rs:2002` (`delete_my_vault`), `:2154` (`vault_edit_submit`), `:2299` (`vault_share_add`), `:2333` (`vault_share_revoke`), `:2588` (`load_personal_vault`, first statement — no network on refusal); 5 refusal tests green, each asserting 400 + exact string |
| 3 | Reads + Refresh NOT refused for locked (D-02) | `locked_account_refresh_still_allowed` green (asserts lock string absent from response); no `is_locked` call in any read handler (grep: guards only at the 5 mutation sites + `index` flag) |
| 4 | Admins never locked even if in locked_accounts (D-02) | `locked_admin_email_not_locked` green (admin POST edit → SEE_OTHER despite lock list); admin exemption `src/server.rs:2043-2046` (`role_for == Some("admin") → false`) |
| 5 | Locked comparison case-insensitive (D-01) | `eq_ignore_ascii_case` at `src/server.rs:2051`; resolve-time trim+lowercase proven by `resolve_locked_accounts_file_section_fallback` (`" Demo@Example.com "` → `demo@example.com`) |
| 6 | Home page: JS flag + popup with EXACT string intercepting Load submit + Delete confirm; byte-inert for non-locked (D-04/D-05) | `home_page_shows_locked_popup_script` + `home_page_clean_for_unlocked_user` green; `LOCKED_ACCOUNT_JS` `src/server.rs:1945-1971` (exact string `:1948`, strips inline `onsubmit` confirm then installs alert — W-4 order fix documented in 12-01-SUMMARY); slot renders `""` on the existing `{autofill}` line (`src/server.rs:551-554`) so non-locked HTML is unchanged |
| 7 | Migration 0009 adds `keep_page_numbers INTEGER NOT NULL DEFAULT 0` + `theme TEXT NOT NULL DEFAULT ''`; auto-applied by `sqlx::migrate!` (D-06) | `migrations/0009_vault_display_options.sql` (verified contents: Phase-12 header + exactly the two ALTER lines); `store::tests::display_options_migration_shape` green |
| 8 | `Vault` carries both fields; every row mapping updated; store accessors next to `set_allow_reshare` (D-06) | `src/store.rs:26,29` (fields), `:1293/:1303/:1313/:1323` (`keep_page_numbers`/`set_keep_page_numbers`/`theme`/`set_theme`); `keep_page_numbers_roundtrip` + `theme_roundtrip` green; full suite green proves no `FROM vaults` mapping missed |
| 9 | Edit page: keep_page_numbers checkbox (present/absent semantics) + theme select (auto/light/navy/rust/coal/ayu) per D-11 visibility (D-07/D-11) | `edit_page_shows_display_options` (converted vault via `wiki_repo`: checkbox + all 6 options) and `edit_page_native_vault_hides_theme_select` (native hides select) green; markup `src/server.rs:2110` (select), `:2128` (checkbox) |
| 10 | `vault_edit_submit` persists both; REJECTS off-whitelist theme via error_page — no coercion (D-07) | Whitelist `["", "light", "navy", "rust", "coal", "ayu"]` at `src/server.rs:2159-2161`, checked after `can_edit_vault` + `is_locked` gates; `edit_submit_persists_display_options`, `edit_submit_without_checkbox_clears_keep_page_numbers`, `edit_submit_rejects_bad_theme` (400, stored theme unchanged) green |
| 11 | Serve-time application inside `inject_vault_chrome`, no rebuild/Refresh/book.toml change (D-08) | `inject_vault_chrome(bytes, keep_page_numbers, theme)` `src/server.rs:658`; both `serve_vault_file` call sites pass `vault.vault.keep_page_numbers` + `&vault.vault.theme` (`:724`, `:740`); route tests prove already-built fixture pages change with no rebuild |
| 12 | TWO distinct passes: option pass runs for ALL HTML pages including chrome-early-return (converted) pages (D-08) | `src/server.rs:663-679` — chrome early-return became a skip-condition (no `return`), PASS 2 `apply_vault_display_options` always runs; `display_options_runs_on_chrome_early_return_page` + `serve_converted_vault_page_still_gets_options` green |
| 13 | keep_page_numbers ON injects exact `<style>.chapter-item a > strong[aria-hidden="true"]{display:inline;}</style>` before `</body>`; OFF injects nothing (D-09) | `PAGE_NUMBERS_OVERRIDE` const `src/server.rs:615-616` (exact, static raw string); `rfind("</body>")` insert `:629-631`; `display_options_page_numbers_style_injected_before_body_end`, `display_options_off_is_inert`, `serve_page_numbers_on_injects_override` (override after chrome, before `</body>`) green |
| 14 | Theme set rewrites the two exact literals `default_light_theme = "light";` / `default_dark_theme = "navy";`; inert when absent; visitor localStorage still wins (D-10) | `THEME_LIGHT_LITERAL`/`THEME_DARK_LITERAL` `src/server.rs:620-621`; exact `replace` with `contains` guards `:635-643`; `display_options_theme_rewrite_applied`, `display_options_theme_rewrite_inert_when_absent`, `serve_theme_rewrite_applied`, `serve_theme_unset_leaves_page_untouched` green. localStorage precedence is structural: the rewrite mutates ONLY the two default-literal assignments — mdBook's theme script reads the localStorage `preferredTheme` before consulting defaults (fixture `MDBOOK_PAGE` mirrors that script), so a visitor choice always overrides |
| 15 | `assets/zoom.css` and `tree.rs` `ZOOM_CSS`/`book_toml()` untouched; `zoom_css_hides_mdbook_chapter_numbers` green (D-12) | `git diff --stat 40bee4e~3..HEAD -- assets/zoom.css src/obsidian/tree.rs` → empty (last touch predates the phase: `2f1bcf1`); live run: `obsidian::tree::tests::zoom_css_hides_mdbook_chapter_numbers ... ok` |

## Gate

- `cargo test` (live, this verification): **306 passed, 0 failed, 4 ignored**
- Targeted subsets re-run live: `locked` (13), `home_page` (2), `display_options` (8), `serve_` (5), `edit_submit_rejects_bad_theme`, `edit_page_native_vault_hides_theme_select`, `theme_roundtrip`, `zoom_css_hides_mdbook_chapter_numbers` — all ok
- Summaries report `./scripts/check.sh` exit 0 per plan (fmt + clippy -D warnings + tests + release build)

## Deviations reviewed (all benign, documented in summaries)

- 12-01: `removeAttribute("onsubmit")` before property assignment (correct browser semantics); `is_locked(&SharedState)`; extra config test for the `.or_else` branch.
- 12-02: checkbox renders unconditionally on the owner/admin-gated Edit page (superset of "personal vaults"; theme select correctly gated on `.morinet-build` converted signal); `VaultRow` type alias for clippy.
- 12-03: pass order chrome→options (required by the plan's own "override AFTER chrome" acceptance criterion); route tests log in as admin (established pattern).
- None contradict any must_have truth.

## human_verification (not attempted — remote deploy out of scope)

1. On the live demo instance (with `MORINET_LOCKED_ACCOUNTS=demo@…` set): log in as the demo account in a real browser, click "Load vault" and a Delete confirm → native `alert("demo account can't add or suppress vaults")` appears and the action is blocked.
2. On the live demo: as a vault owner (converted vault), set theme (e.g. coal) on the Edit page, save, reopen a vault page → theme visibly applied with no Refresh/rebuild; then pick a different theme from the mdBook menu → visitor localStorage choice wins on next load.
3. On the live demo: toggle "Keep chapter page numbers" on → sidebar chapter numbers reappear; off → hidden (issue #7 default state).
4. Post-deploy at 1.1.0 ship (D-13): close GitHub issue #7.

## Conclusion

**status: passed — score 10/10.** All 15 must_have truths and all ROADMAP
criteria (REQ-DEMO-001/002, REQ-VOPT-001/002) are proven by green tests
executed against the current tree plus code inspection. No gaps.
