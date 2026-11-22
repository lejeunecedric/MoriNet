---
phase: MORI-21-ui-polish
range: 989629e..HEAD (be6ec18..867e350)
date: 2026-11-22
reviewer: gsd-code-reviewer
status: issues_found
---

# Phase 21 Code Review — UI Polish (OKF badge details, Looks picker, Edit Theme row)

## Findings

### HIGH-1 — Looks picker never applies the palette and corrupts `mdbook-theme` (double-handled click)

**File:** `assets/zoom.js:95-102,131` (interacting with `mdbook-html-0.5.4` `book.js:455-472,395-437,380`)

The injected Looks buttons set `b.className = "theme"` (`assets/zoom.js:131`) to reuse the
popup's styling. But mdBook 0.5.4's `book.js` attaches its own click listener to the popup
element itself (`themePopup.addEventListener('click', …)`, book.js:455) which matches **any**
event target with `className === 'theme'`, strips the `mdbook-theme-` prefix (a no-op for
`morinet-looks-*` ids), and calls `set_theme('morinet-looks-light')`. `set_theme` then
unconditionally writes `localStorage.setItem('mdbook-theme', 'morinet-looks-light')`
(book.js:427-432, `store` defaults to true).

Event ordering makes this deterministic, not a race: MoriNet's handler is on the **button**
(target phase) and runs first — it writes the valid value (`zoom.js:97`) and calls
`window.location.reload()` (`zoom.js:100`), which schedules navigation but does **not**
interrupt the current event dispatch. book.js's popup listener (bubble phase on the ancestor)
then runs and overwrites the key with the invalid `morinet-looks-*` value. After the reload,
`get_theme()` rejects it (`!themeIds.includes('mdbook-theme-' + theme)`, book.js:380) and falls
back to the vault default. Net effect:

1. Picking a Look **never changes the palette** (the feature is inert on the pipeline binary,
   mdBook 0.5.4 — verified installed: `mdbook --version` → v0.5.4).
2. It **clobbers the visitor's previously chosen theme** with a permanently invalid value until
   something else rewrites the key.

The embedded-bytes test (`src/obsidian/tree.rs:529-543`) only asserts string presence inside
`MORINET_JS` and cannot catch this.

**Fix:** make the pick handler stop the event before reloading:
`function pick(palette) { return function (e) { e.preventDefault(); e.stopPropagation(); try { localStorage.setItem('mdbook-theme', palette); } catch (e) {} window.location.reload(); }; }`
`stopPropagation` prevents the bubble-phase popup listener from ever seeing the click, keeps
the zero-new-CSS `class="theme"` styling, and needs no mdBook coupling. Add a note to the
SUMMARY/plan that the popup-level listener exists (book.js:455) so future injections don't
re-introduce this.

### MEDIUM-1 — "Looks" section duplicates mdBook's own palette items in the same popup

**File:** `assets/zoom.js:115-152`; ground truth `mdbook-html-0.5.4/front-end/templates/index.hbs:152-158`

`ul#mdbook-theme-list` already statically renders Auto + Light + Rust + Coal + Navy + Ayu
buttons, and they work (book.js:455-472). The picker inserts a second list of the same five
palettes at the top of that same popup, so the menu shows two near-identical palette lists.
`21-02-PLAN.md:57-66` enumerates the existing buttons and chose this anyway ("the popup IS the
settings menu"), so it is plan-intended — but the user-visible redundancy was likely not the
issue-#17 intent ("Looks before About" implies a menu *without* palettes). The user doc
(`docs/morinet-docs/src/sharing-and-roles.md:90-94`) presents "Looks" as *the* palette picker,
which reads oddly when the popup already had one.

**Fix:** either hide/replace mdBook's own `li` items when the Looks section is injected
(e.g. `display:none` via the existing vocabulary is not available without CSS — so instead
`remove()` the built-in `li`s after building Looks, keeping zero new CSS), or drop the Looks
section and rely on mdBook's built-in buttons. At minimum, record the duplication decision in
the SUMMARY.

### LOW-1 — "Looks" header `<li>` lacks `role="none"` inside `ul[role="menu"]`

**File:** `assets/zoom.js:122-123`

mdBook's own items use `<li role="none">` so the menu's accessibility tree only exposes the
buttons. The header `head.textContent = "Looks"` li has no role, so screen readers announce an
extra unnamed menuitem-ish item. **Fix:** `head.setAttribute("role", "none");`.

### LOW-2 — `aria-current` marker is static and unstyled

**File:** `assets/zoom.js:120,134-136`

`aria-current="true"` is set once at build time from localStorage. If the visitor then picks a
palette via mdBook's own buttons (which apply without reload), the Looks marker goes stale;
and no stylesheet in the popup targets `aria-current`, so there is no visible "current"
indicator anyway (mdBook marks selection with the `.theme-selected` class instead).
**Fix:** drop the attribute, or mirror mdBook's `.theme-selected` class on the matching button
(class already styled by mdBook's chrome.css — zero new CSS).

### LOW-3 — Reload fires even when `localStorage` write fails

**File:** `assets/zoom.js:97-100`

In private-mode/disabled-storage the `catch` swallows the failure and the page reloads with no
visible change — a confusing no-op loop for the user. **Fix:** track success and skip
`window.location.reload()` when `setItem` threw.

### LOW-4 — `MAX_MISSES` caps the miss count but not the stored byte size

**File:** `src/okf.rs:22,88-93`

The D-01/T-21-02 bound is on the number of paths (200), not their length. A hostile vault with
very long relative paths (up to filesystem PATH_MAX per entry) can still push a few hundred KB
into a single `okf_json` row. Bounded, but the comment claims "cannot grow okf_json without
bound" — the bound is loose. **Fix:** truncate each recorded path at a fixed length (e.g. 200
chars, with an ellipsis) before pushing.

### LOW-5 — Test body cap raised to 64 KiB is brittle

**File:** `src/server.rs:5031,5036` (`admin_reads_vault_files`)

The cap must exceed the inlined-chrome page size; the next asset growth breaks the test with an
opaque `to_bytes` length error rather than an assertion. **Fix:** use a generous fixed cap
(e.g. 512 KiB) or `usize::MAX`-style bound for test reads. Test-only.

## Constraint checks (all pass unless noted)

- **Zero new CSS (Phase 19 D-03):** pass — no `.css` file touched in range; `summary.menu-btn`
  reuses `assets/style.css:107-125` (incl. pre-existing `::-webkit-details-marker` rule),
  `field-wide` is pre-existing (`assets/style.css:256`), popup items reuse mdBook's own
  `li`/`button.theme` vocabulary.
- **serde-default on new `okf_json` fields:** pass — `src/okf.rs:40,43`; old-row compat tested
  (`src/okf.rs:746-754`, `src/server.rs` `okf_badge_html_old_json_renders`).
- **XSS escaping:** pass — non-conformant paths go through `html_escape`
  (`src/server.rs:553`, impl `src/server.rs:1253-1258`); version escaped; counts are typed
  integers; overflow count is a server-side `usize`. Test `okf_badge_html_escapes_nonconformant_paths`
  covers the stored-XSS gate (T-21-01). zoom.js uses `textContent` only.
- **localStorage key exactly `mdbook-theme`:** pass on write (`assets/zoom.js:97`) — but see
  HIGH-1: mdBook's own listener rewrites the key with a corrupted value on the same click.
- **No secrets / no new dependencies:** pass — Cargo.toml/lock untouched in range.
- **zoom.js ES5 constraint (21-02-PLAN):** pass — `var`/function expressions only.
- **Badge reason logic vs scan logic:** consistent — the None-arm reasons
  (`src/server.rs:561-576`) exactly mirror the version predicate
  (`src/okf.rs:95-99`: `total > 0 && concepts*100 >= total*80`), so reasons are never empty.

## Verdict

`issues_found` — HIGH-1 must be fixed before this feature can be considered working on the
pipeline mdBook (0.5.4): the picker is inert and corrupts the visitor's theme key. MEDIUM-1
should get an explicit product decision. LOWs are polish.
