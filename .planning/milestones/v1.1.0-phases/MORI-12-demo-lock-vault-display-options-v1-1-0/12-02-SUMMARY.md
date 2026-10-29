# 12-02 Summary — per-vault display options: storage + Edit form (REQ-VOPT-001, REQ-VOPT-002)

**Status:** complete. Full gate `./scripts/check.sh` exit 0 — **296 passed** (288 baseline + 8 new), 4 ignored, clippy `-D warnings` clean.
**Commits:** `bdf1e7b` feat(12-02) store/migration · `036b327` feat(12-02) server Edit page · `98c5c90` style(12-02) cargo fmt

## What was built

### Task 1 — migration + store (`migrations/0009_vault_display_options.sql`, `src/store.rs`)
- `0009_vault_display_options.sql`: `ALTER TABLE vaults ADD COLUMN keep_page_numbers INTEGER NOT NULL DEFAULT 0` and `theme TEXT NOT NULL DEFAULT ''` — additive, in-place (T-12-08). Auto-picked by `sqlx::migrate!`.
- `Vault` struct gains `keep_page_numbers: bool` + `theme: String` (they ride on `Vault` so `serve_vault_file` in 12-03 needs no side query).
- New `type VaultRow = (i64, String, String, String, String, Option<String>, bool, String)` alias (clippy `type_complexity` on the 8-tuples; mirrors the existing `ShareRow` precedent).
- Every row-mapping site updated to select + carry the new columns: `list_vaults_for`, `list_all_vaults`, and all four `accessible_vaults` branches (admin / own-personal / globals / direct-shares).
- Accessors next to the `allow_reshare` pair: `keep_page_numbers(slug) -> bool`, `set_keep_page_numbers(slug, on)`, `theme(slug) -> String`, `set_theme(slug, theme)` — same bool↔INTEGER query_as/UPDATE pattern.

### Task 2 — Edit page + submit (`src/server.rs`)
- `EditForm` gains `keep_page_numbers: Option<String>` (checkbox semantics: present = on, absent = off, like `allow_reshare`) and `theme: String` (`#[serde(default)]`).
- `vault_edit_page`: prefills both via the Task 1 getters; renders the "Keep chapter page numbers" checkbox (checked-slot pattern of the reshare checkbox) and, **only for converted vaults**, a `<select name="theme">` with options `auto` (value `""`), `light`, `navy`, `rust`, `coal`, `ayu`, stored value marked `selected`.
- **D-11 converted test (checker fix B-1r):** converted ⇔ `Path::new(&book_path).components().any(|c| c.as_os_str() == ".morinet-build")` — same signal as `webdav.rs::source_dir_of`; no new column added.
- `vault_edit_submit`: theme whitelist `["", "light", "navy", "rust", "coal", "ayu"]` checked **after** the existing `can_edit_vault` + 12-01 `is_locked` gates (T-12-07); unknown value → `error_page` 400 REJECT, never coerced (T-12-06; message passes through `error_page`'s html_escape). On success, persists `set_keep_page_numbers` + `set_theme` after the existing `set_allow_reshare` block, then 303 → `/`.

## Tests added (8, all TDD-red first)
- `store::tests::display_options_migration_shape` — new vaults get 0 / `''` defaults.
- `store::tests::keep_page_numbers_roundtrip` — default false, set/get true, fresh store defaults false.
- `store::tests::theme_roundtrip` — default `''`, set/get `coal`.
- `server::tests::edit_page_shows_display_options` — vault loaded through `wiki_repo(...)` (converted pipeline): checkbox + theme select with all five options + auto `value=""`.
- `server::tests::edit_page_native_vault_hides_theme_select` — native vault (`seed_edit_fixture`): checkbox present, theme select absent (D-11 negative case).
- `server::tests::edit_submit_persists_display_options` — POST `keep_page_numbers=on&theme=coal` → 303, store shows true/`coal`.
- `server::tests::edit_submit_without_checkbox_clears_keep_page_numbers` — absent field clears the toggle (on/off pair contract, same as reshare).
- `server::tests::edit_submit_rejects_bad_theme` — `theme=hacker` → 400, stored theme untouched (reject-not-coerce).

## Deviations
1. **Commit split:** the plan suggested `feat` + `test` commits per task; because tests were written first (TDD) and only pass with the implementation, each task landed as one `feat(12-02)` commit containing tests + code, plus one `style(12-02)` commit for `cargo fmt` (rustfmt collapsed a multi-line `format!` the first gate run flagged). No functional difference.
2. **Extra test:** added `edit_page_native_vault_hides_theme_select` beyond the plan's four Task-2 tests — the plan asserts the positive D-11 case only; the negative case (native vault hides the select) is the half most likely to regress.
3. **Checkbox visibility:** the plan says the checkbox shows "for personal vaults"; the Edit page is already owner/admin-gated (`can_edit_vault`), so the checkbox renders unconditionally on it — an admin editing a *global* vault also sees it. The theme select is gated purely on the converted test (converted vaults are always personal). Trivial to gate on `a.personal` later if desired.
4. **`VaultRow` alias:** introduced to satisfy `clippy::type_complexity` on the widened 8-tuples (the plan's acceptance gate includes clippy `-D warnings`); follows the existing `ShareRow` pattern.

## Threat model disposition
T-12-06 mitigated (whitelist reject, six fixed tokens only), T-12-07 mitigated (persistence strictly after can_edit_vault + is_locked), T-12-08 mitigated (additive DEFAULT columns), T-12-09 accepted (owner's own value, whitelist-bounded), T-12-SC accepted (no new deps). No new threats found in touched code.

## For 12-03 (consumer plan)
- `vault.vault.keep_page_numbers: bool` and `vault.vault.theme: String` are on `Vault` — available wherever `AccessibleVault` is in scope, no extra query.
- `""` theme means "leave mdBook default"; only non-empty values should trigger the book.toml rewrite.
- Converted-vault detection precedent if needed again: `.morinet-build` path component (webdav.rs:33).
