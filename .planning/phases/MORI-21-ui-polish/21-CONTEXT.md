# Phase 21: ui-polish - Context

**Gathered:** 2026-11-21
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix the three UI friction points users reported on GitHub (#16 OKF badge details, #17 "Looks" color-scheme picker, #13 Vault Edit menu alignment). No new capabilities beyond these three fixes; nothing else in the app is touched.

Requirements: UI-01 (badge click → conformance detail), UI-02 ("Looks" before "About"), UI-03 (Vault Edit realignment).

</domain>

<decisions>
## Implementation Decisions

### OKF badge details (UI-01, issue #16 — discussed)
- **D-01:** The conformance scan (`scan_conformance` in `src/okf.rs`) additionally records the relative paths of non-conformant files (markdown files without a non-empty `type` frontmatter), capped at ~200 paths with an overflow count ("+N more") so `okf_json` stays bounded. `OkfStats` JSON grows accordingly — deserialization must stay backward-compatible (missing field = empty list; the badge serde is already defensive, `src/server.rs:526-538`).
- **D-02:** Clicking the badge expands an **inline `<details>`** directly under it — same disclosure pattern as the account menu (`details.menu`). Zero new CSS: reuse existing classes only (project rule from Phase 19 D-03: no new CSS without an Impeccable pass).
- **D-03:** The clickable detail lives **everywhere the badge exists**: home vault cards (`vault_row`) and the vault page — one shared component. *(Correction 2026-11-21, plan-checker grounded: `okf_badge_html` renders only through `vault_row` on home lists — served book pages carry no badge today; adding one there needs a new per-request store fetch + injection = new capability, out of phase boundary. Scope = vault cards; ROADMAP criterion 1 amended to match.)*
- **D-04:** The "not an OKF bundle" pill is clickable too and explains **why**: 0 concepts / below the 80 % concept ratio / missing `index.md` / missing `log.md`. The expanded detail shows: version, concepts/total, index.md and log.md presence, scan date, and the non-conformant file list (when any).

### Looks color schemes (UI-02, issue #17 — Claude's discretion)
- **D-05:** "Looks" lives in the **mdBook book menu** (where "About" is — mdBook's own settings popup), as an entry before "About", per the issue text. It offers the mdBook palette (light/navy/rust/coal/ayu) and persists the choice in **localStorage** under mdBook's own theme key — consistent with mdBook's built-in theme behavior and the Phase 12 rule that visitor localStorage wins over the vault's serve-time theme default. No server-side per-reader preference storage in this phase.

### Vault Edit alignment (UI-03, issue #13 — Claude's discretion)
- **D-06:** Minimal fix, no form restructure: the Theme field already follows the "allow…" checkboxes in source order (`src/server.rs:2475+`); correct the rendered layout so the Theme select sits visually below the "allow…" line and directly above the Save button (the `field-wide` Source URL wrapping is the likely culprit). Reuse existing `field` classes.

### Claude's Discretion
- UI-02 and UI-03 implementation details above were not user-selected (user chose to discuss only the OKF badge); the decisions above are Claude's recommended approach, consistent with the issue texts and prior locked decisions.
- D-05 and D-06 **locked as Claude-recommended defaults on 2026-11-21** (plan-phase gate timed out — override possible): Looks picker via mdBook book menu + localStorage; minimal Vault Edit layout realignment. Phase research and UI-SPEC likewise skipped per project precedent (phases 19–20 shipped without either).

</decisions>

<canonical_refs>
## Canonical References

### GitHub issues (scope source)
- https://github.com/lejeunecedric/MoriNet/issues/16 — OKF label click → list of unfulfilled concepts (with screenshot)
- https://github.com/lejeunecedric/MoriNet/issues/17 — "look" button before "about" in the settings menu, schemes matching mdBook themes
- https://github.com/lejeunecedric/MoriNet/issues/13 — Vault Edit menu realignment

### Code anchors (current HEAD)
- `src/okf.rs` — `OkfStats` struct + `scan_conformance()` (extend with non-conformant path list)
- `src/server.rs` — `okf_badge_html` (~:530), `vault_row` (~:570), account menu markup (~:1256), `vault_edit_page` (~:2475), `apply_vault_display_options` + `THEME_WHITELIST` (~:752-786)
- `src/store.rs` — `set_vault_okf` / `vault_okf` (~:976-990), `theme` accessor
- `docs/morinet-docs/src/sharing-and-roles.md` — user-facing display-options + statistics docs (update if behavior text changes)

### Prior-phase constraints
- `.planning/milestones/v1.4-phases/MORI-19-statistics-observability/19-CONTEXT.md` D-03 — **no new CSS without an Impeccable pass**; reuse existing section/table/pill classes
- Phase 12 D-08/D-10 (ROADMAP v1.1 details block) — display options applied at SERVE time; visitor localStorage wins

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `details.menu` disclosure pattern (account dropdown, `src/server.rs:1256`) — model for the badge `<details>` expansion
- `okf_badge_html()` (`src/server.rs:530`) — defensive serde over `vaults.okf_json`; extend to emit the `<details>` wrapper
- `vault_row()` (`src/server.rs:570`) — already takes the badge string; both home and vault page render through it
- `THEME_WHITELIST` + `apply_vault_display_options()` (`src/server.rs:766`) — serve-time theme literals; the Looks picker writes the same localStorage keys mdBook reads (`default_light_theme` / `default_dark_theme` flow)
- Existing form classes `field`, `field-wide`, `btn-ghost`, `menu-btn` — all styling must come from these

### Established Patterns
- HTML built inline in Rust handlers via `format!` with `html_escape` — no template engine
- `vaults.okf_json` is a JSON blob column (migration 0013) — schema-free extension, but old rows must deserialize
- Scan runs at refresh/register; detail data must come from the stored scan, never a re-scan at page render

### Integration Points
- `scan_conformance()` call sites (refresh/register paths) persist via `store.set_vault_okf`
- mdBook chrome injection for converted vaults goes through `theme/zoom.css` + `theme/morinet.js` (`additional-css`/`additional-js`) — the natural place for the "Looks" menu entry JS

</code_context>

<specifics>
## Specific Ideas

- Issue #16 screenshot shows the two pills ("OKF v0.2 — N/M concepts" / "not an OKF bundle") — the user's ask verbatim: "When clicking on the OKF label and some concepts are not fullfilled show a list".
- Issue #17 verbatim: "In the settings menu, add a 'look' button before the 'about' button for the choice of color schemes matching the ones available for the mdbooks".

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope. (Server-side per-reader theme preference was considered and deferred: localStorage covers the issue's ask; a `user_prefs` column would be its own phase.)

</deferred>

---

*Phase: 21-ui-polish*
*Context gathered: 2026-11-21*
