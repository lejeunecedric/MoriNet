---
phase: 21-ui-polish
verified: 2026-11-22
status: passed
score: "15/15 must-haves verified"
requirements: [UI-01, UI-02, UI-03]
deferred:
  - gap: "UI-02 original wording ('account menu … applied across the app and served books') — app-chrome restyling not shipped"
    deferred_to: "explicit deferral in ROADMAP Phase 21 success criterion 2 (amended 2026-11-21): app-chrome restyling would need new CSS + server prefs, out of phase boundary per locked D-05"
---

# Phase 21: ui-polish Verification

**Goal (ROADMAP):** "The three UI friction points users reported are gone." (UI-01 #16, UI-02 #17, UI-03 #13)

**Overall: PASSED** — all 15 must-haves verified against the codebase with passing tests (620/0 at verification time; 623/0 after review fixes), and all three browser items (HV-1..HV-3) resolved 2026-11-22 by orchestrator live UAT on a local instance (see "Human verification — RESOLVED").

## Goal-backward summary

| Friction point | Delivered? | Evidence |
|---|---|---|
| #16 OKF badge is a static label | ✓ Gone — badge is a clickable `<details>` disclosure with pass/fail reasons and capped non-conformant list | `src/server.rs:537-598`, 8 badge tests green |
| #17 No reader color-scheme choice | ✓ Gone — "Looks" section in mdBook theme popup, 5 palettes, persisted via `mdbook-theme` localStorage key | `assets/zoom.js:81-140`, `src/obsidian/tree.rs:530` |
| #13 Vault Edit Theme select floats onto checkbox row | ✓ Gone — Theme field forced onto its own flex row below checkboxes, above Save | `src/server.rs:2565`, order test at `:4902` |

## Must-haves verified

### 21-01 — OKF badge conformance details (UI-01)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Clicking badge pill expands inline detail (version, concepts/total, index.md/log.md, scan date, non-conformant list) (D-02, D-04) | ✓ | `src/server.rs:537-598` — both arms return `<details><summary class="menu-btn">`; panel at `:546-558, 583-596`. Tests: `okf_badge_html_v02_string_exact` (`:10579`), `okf_badge_html_none_when_unscanned` |
| 2 | "not an OKF bundle" pill is clickable and explains WHY (0 concepts / below 80 % ratio / missing index.md / missing log.md) (D-04) | ✓ | `src/server.rs:562-581` (reasons derived from stats, incl. `concepts*100 < total*80` at `:566`). Test: `okf_badge_html_not_bundle_explains_reasons` (`:10622`) asserts `missing index.md`, `no concepts (0/3)` |
| 3 | Detail data comes from STORED `okf_json` only — no re-scan at render (D-01) | ✓ | `okf_badge_html(json: Option<&str>)` takes the stored blob; call sites read `store.vault_okf` (`server.rs:693-702`, `:712`). Single persist site unchanged: `record_okf_scan` → `scan_conformance` + `set_vault_okf` (`server.rs:3369-3371`) |
| 4 | Non-conformant paths recorded at scan time, capped at 200 with overflow "+N more" (D-01) | ✓ | `src/okf.rs:22` `MAX_MISSES = 200`; cap/overflow logic `:88-92`; render `server.rs:555-557`. Tests: `scan_misses_capped_at_max_with_overflow` (`okf.rs:735`), `okf_badge_html_overflow_shows_plus_n_more` (`server.rs:10613`) |
| 5 | Pre-phase `okf_json` rows still deserialize and render (D-01 backward-compat) | ✓ | `#[serde(default)]` on `non_conformant` + `misses_overflow` (`okf.rs:40-44`). Tests: `old_okfstats_json_without_new_fields_deserializes` (`okf.rs:747`), `okf_badge_html_old_json_renders` (`server.rs:10632`) |
| 6 | One shared component at every badge call site — both home lists render through `vault_row` (D-03) | ✓ | `okf_badge_html` threaded through `vault_row(a, can_share, &badge)` (`server.rs:622`); own-list call site `:693`, shared-list call site `:712` — no forked markup |

Security gate T-21-01 (stored XSS via file paths): every path interpolated only via `html_escape` (`server.rs:553`, version at `:587`); locked by `okf_badge_html_escapes_nonconformant_paths` (`:10603`, asserts `&lt;script&gt;` present and raw `<script>` absent).

### 21-02 — Looks color-scheme picker (UI-02)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | "Looks" entry appears in the mdBook book theme popup, positioned before About when one exists (D-05) | ✓ | `assets/zoom.js:104-121` — anchors on `#mdbook-theme-list`; defensive About lookup (`#about-btn`, then exact-text "About" in `#mdbook-menu-bar`), else inserts at popup top (mdBook 0.5.4 renders no static About — documented in amended ROADMAP criterion 2) |
| 2 | Picking a scheme writes `localStorage['mdbook-theme']` and reloads restyled from light/navy/rust/coal/ayu (D-05) | ✓ | `assets/zoom.js:89-97` — five palette entries with ids `morinet-looks-{light,navy,rust,coal,ayu}`; `localStorage.setItem('mdbook-theme', palette)` + `window.location.reload()`. Test: `morinet_js_ships_looks_picker` (`src/obsidian/tree.rs:530`) locks embedded bytes |
| 3 | Choice persists per reader and wins over the vault's serve-time default | ✓ (code) | Same key mdBook's `get_theme()` reads (verified against vendored built book, 21-PATTERNS §5); `aria-current` marks saved choice (`zoom.js:120, 135`). Cross-session behavior in a real browser → human item HV-2 |
| 4 | Nothing stored server-side for this preference (D-05) | ✓ | Phase diff touches no store/schema for prefs; picker is pure client JS in `assets/zoom.js` |
| 5 | Degrades silently on pages without the popup markup | ✓ | `zoom.js:117` — returns if `#mdbook-theme-list` absent; localStorage access wrapped in try/catch (`:97, 120`); build-once guard `:116`. Zero-console-error in a real browser → human item HV-2 |

Delivery wiring verified: `MORINET_JS = include_str!("../../assets/zoom.js")` (`tree.rs:282`); converted vaults get `theme/morinet.js` at build (`convert.rs:129`); native vaults inline at serve time (`server.rs:856-863`). ES5 convention held (no `let`/`const`/`=>` in `assets/zoom.js`).

### 21-03 — Vault Edit realignment + docs (UI-03)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Theme selector renders below the "allow…" checkbox line, directly above Save (D-06) | ✓ (markup order) | `src/server.rs:2565` — `class="field field-wide"` on the `ev-theme` div; `.field-wide { flex: 1 1 20rem }` (`assets/style.css:256`) forces its own flex row. Test `edit_page_theme_field_sits_below_checkboxes_above_save` (`server.rs:4902`) asserts rendered order `ev-reshare < ev-keep < ev-theme < Save` (`:4941-4954`). Pixel-level appearance → human item HV-3 |
| 2 | Markup-only fix, no form restructure, no new CSS (D-06, Phase 19 D-03) | ✓ | `git diff --stat be6ec18~1..HEAD -- '*.css'` is empty across the whole phase; only a class string changed in `theme_field` |
| 3 | Theme select still converted-vault-only, same auto/light/navy/rust/coal/ayu options, unchanged POST handling | ✓ | Options intact at `server.rs:2565`; `THEMES` whitelist untouched; `edit_submit_rejects_bad_theme` and `edit_page_native_vault_hides_theme_select` green |
| 4 | User docs describe badge click-detail (UI-01) and Looks picker (UI-02) | ✓ | `docs/morinet-docs/src/sharing-and-roles.md:90-94` (Looks paragraph) and `:107-113` (badge detail paragraph with "non-conformant" and "+N more"); grep confirms no server-side/per-account preference claims (D-05) |

## Requirements coverage (REQUIREMENTS.md)

| Req | Description (abridged) | Status | Notes |
|---|---|---|---|
| UI-01 (#16) | Clicking OKF badge opens conformance detail with pass/fail reasons | ✓ SATISFIED | All 6 plan-21-01 truths verified |
| UI-02 (#17) | "Looks" entry before "About", color scheme persisted per reader | ✓ SATISFIED (as amended) | Original wording said "account menu … across the app"; ROADMAP criterion 2 amended 2026-11-21 to mdBook book menu + localStorage per locked D-05. App-chrome restyling explicitly deferred (see Deferred) |
| UI-03 (#13) | Vault Edit: Theme below "allow…" line, above Save | ✓ SATISFIED | Order test locks source order; visual check in HV-3 |

Traceability table in `.planning/REQUIREMENTS.md:58-60` still shows "Pending" for UI-01..UI-03 — expected; updated by the phase-completion step, not by this verifier (per task constraints).

## Decision coverage (21-CONTEXT.md D-01..D-06)

6/6 honored: D-01 (capped scan fields + serde-default) → `okf.rs:22-44`; D-02 (inline `<details>`, zero new CSS) → `server.rs:537`; D-03 (shared component, corrected scope = vault cards) → `vault_row` threading; D-04 (reasons list) → `server.rs:562-581`; D-05 (book-menu Looks + localStorage, no server prefs) → `zoom.js`; D-06 (field-wide one-class fix) → `server.rs:2565`.

## Behavioral verification

| Check | Result | Detail |
|---|---|---|
| `cargo test` (full suite) | ✓ 620 passed / 0 failed | Run by verifier 2026-11-22; matches expected 620/0 (baseline 611 + 9 new phase tests) |
| Phase test names | ✓ present | `scan_records_non_conformant_paths`, `scan_misses_capped_at_max_with_overflow`, `old_okfstats_json_without_new_fields_deserializes`, `okf_badge_html_escapes_nonconformant_paths`, `okf_badge_html_overflow_shows_plus_n_more`, `okf_badge_html_not_bundle_explains_reasons`, `okf_badge_html_old_json_renders`, `morinet_js_ships_looks_picker`, `edit_page_theme_field_sits_below_checkboxes_above_save` |
| Zero-new-CSS rule | ✓ | `git diff --stat be6ec18~1..HEAD -- '*.css'` empty |

## Test quality audit

| Area | Linked req | Skipped | Circular | Assertion level | Verdict |
|---|---|---|---|---|---|
| `src/okf.rs` scan tests | UI-01 | none | no (fixtures hand-built via tmpdir helpers) | value (`assert_eq!` on lists/counts) | PASS |
| `okf_badge_html_*` tests | UI-01 | none | no (assert against literal expected substrings) | value (contains specific strings; negative assert on raw `<script>`) | PASS |
| `morinet_js_ships_looks_picker` | UI-02 | none | no (asserts asset bytes, independent of runtime) | value (contains) | PASS |
| `edit_page_theme_field_sits_below_checkboxes_above_save` | UI-03 | none | no (GETs real route, asserts source order) | behavioral (rendered-page order) | PASS |

No disabled/ignored tests in phase-touched files; no circular patterns found.

## Anti-patterns

No TBD/FIXME/XXX/TODO/HACK in phase-touched files. The `placeholder` hits in `src/okf.rs` (`:176, 425, 928, 1014-1017`) are pre-existing viz-template token comments from Phase 20, not stubs — ℹ️ Info only.

## Deferred items

| Item | Disposition |
|---|---|
| App-chrome restyling from mdBook palette (UI-02 original wording "applied across the app") | Explicitly deferred in ROADMAP criterion 2 amendment (2026-11-21): would need new CSS + server-side prefs, out of phase boundary per locked D-05. Not assigned to a later phase — flag for backlog if the user still wants it |
| OKF badge on served book pages (D-03 original "vault page") | Corrected in CONTEXT D-03 correction + ROADMAP criterion 1 amendment: badge exists only on home vault cards; injecting into book pages needs new per-request fetch + injection = new capability, out of scope |

## Human verification — RESOLVED (2026-11-22, orchestrator live UAT)

Local instance (`target/release/morinet serve`, demo data), real Chromium (CDP):

- **HV-1 — RESOLVED ✓**: B3Wiki card badge ("OKF v0.2 — 153/160 concepts") click expands the in-flow panel showing version, concepts ratio, index.md/log.md presence, scan date and the non-conformant file list; second click collapses; card layout intact.
- **HV-2 — RESOLVED ✓ (after HIGH review fix `10d6e0f` + book rebuild)**: theme popup shows "Looks" with Light/Navy/Rust/Coal/Ayu; picking Ayu writes `localStorage['mdbook-theme']='ayu'` (valid value — the pre-fix double-fire wrote the invalid `morinet-looks-ayu`, reproducing the review's HIGH finding in a real browser), `<html class="ayu">` applies the palette, value survives reload/navigation, `aria-current` marks the saved choice. Note: the built book's `theme/morinet.js` is copied at vault build time — a vault refreshed before the fix keeps the old handler until re-refreshed.
- **HV-3 — RESOLVED ✓ (after fix `d564f0c`)**: initial live check found Save rendering beside the Theme select (the plan's premise that `.field-wide { flex: 1 1 20rem }` forces a full row was false). Fix applied inline TDD (test strengthened → RED → markup `style="flex-basis:100%"` → GREEN): Theme now occupies its own full-width row below the checkboxes with Save directly below it (measured: checkboxes y=230, theme row y=261–328, Save y=340). Layout choice: D-06 wording ("directly above Save") applied as recommended default after the clarify gate timed out — override possible.

## Metadata

- Verified commit: `f2de841` (HEAD of phase 21 work)
- Phase commits: `be6ec18..f2de841` (15 commits, 3 plans)
- Test suite: 620 passed / 0 failed (verifier-run, 2026-11-22)
- Gaps: none. HV-1..HV-3 resolved by live browser UAT 2026-11-22 (see above); status upgraded to `passed`.
