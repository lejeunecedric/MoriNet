# Phase 21: ui-polish — Patterns Map

**Generated:** 2026-11-21 (gsd-pattern-mapper)
**Source of intent:** `21-CONTEXT.md` (D-01..D-06), `REQUIREMENTS.md` UI-01/UI-02/UI-03
**Note:** `gsd-sdk query agent-skills gsd-pattern-mapper` returns an empty string on this install (expected); general GSD conventions applied.

Every excerpt below was read from the working tree at HEAD and is verbatim, with verified `file:line` anchors. No source file was modified.

---

## 1. `src/okf.rs` — extend `OkfStats` + `scan_conformance` (UI-01 / D-01)

**Role:** pure domain module (no store, no async). **Data flow:** vault source dir → `scan_conformance()` → `OkfStats` → `serde_json::to_string` → `vaults.okf_json` TEXT column → read back by `store.vault_okf` → `okf_badge_html` parses it.

### Analog: the struct to extend — `src/okf.rs:19-32`

```rust
/// Result of an OKF conformance scan over one vault source directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfStats {
    /// `"0.2"` / `"0.1"` / `None` (= not an OKF bundle).
    pub version: Option<String>,
    /// Non-reserved `.md` files with a non-empty `type:`.
    pub concepts: usize,
    /// Non-reserved `.md` files seen (unreadable ones count here only).
    pub total: usize,
    pub has_index: bool,
    pub has_log: bool,
    /// Epoch seconds at scan time.
    pub scanned_at: u64,
}
```

D-01 adds a non-conformant-path list here. **Convention: the new field must carry `#[serde(default)]`** so old `okf_json` rows (written before this phase) still deserialize — the serde-defensiveness precedent is the form struct at `src/server.rs:2459-2473` (`#[serde(default)]` on optional fields; cited in §5).

### Analog: the scan loop that must record misses — `src/okf.rs:54-74`

```rust
    for rel in crate::mcp::list_md_files(dir) {
        if rel == "index.md" {
            has_index = true;
            continue;
        }
        if rel == "log.md" {
            has_log = true;
            continue;
        }
        total += 1;
        let Ok(md) = std::fs::read_to_string(dir.join(&rel)) else {
            continue; // unreadable / non-UTF-8: total only, never a concept
        };
        let (fm, _) = frontmatter::split(&md);
        if fm.page_type().is_some_and(|t| !t.trim().is_empty()) {
            concepts += 1;
            if V02_KEYS.iter().any(|k| fm.has(k)) {
                v02 = true;
            }
        }
    }
```

The `else` branch of the `page_type()` check (untyped file) is exactly where the relative path gets pushed (D-01: "markdown files without a non-empty `type` frontmatter").

### Analog: the cap-with-overflow-count idiom — `src/okf.rs:131-137`

```rust
/// Entry-count bound for [`build_bundle_zip`] (T-18-10). The zip is built
/// in-memory per D-06's simplicity call, so the walk must stop before a
/// hostile tree can exhaust memory. The existing quota convention is
/// vault-COUNT not bytes, so an explicit constant is the honest bound.
pub const MAX_EXPORT_ENTRIES: usize = 20_000;
/// Total-source-bytes bound for [`build_bundle_zip`] (T-18-10): 256 MiB.
pub const MAX_EXPORT_BYTES: u64 = 268_435_456;
```

D-01's "~200 paths capped + overflow count" follows this `pub const` + explicit-bound convention (a `MAX_MISSES: usize = 200`-style constant, "+N more" rendered at display time or stored as a count field).

### Analog: the single persist call site — `src/server.rs:3310-3320`

```rust
pub async fn record_okf_scan(
    store: &Store,
    slug: &str,
    vault_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let Some(src) = webdav::source_dir_of(vault_dir) else {
        return Ok(());
    };
    if !src.is_dir() {
        return Ok(());
    }
    let stats = crate::okf::scan_conformance(&src)?;
    store
        .set_vault_okf(slug, &serde_json::to_string(&stats)?)
        .await
```

CONTEXT rule honored: detail data comes from the **stored scan only** — never re-scan at page render.

---

## 2. `src/server.rs` — `okf_badge_html` becomes the shared `<details>` component (UI-01 / D-02, D-03, D-04)

**Role:** HTML-fragment renderer over stored JSON. **Data flow:** `store.vault_okf(slug)` → `Option<&str>` JSON → defensive serde → badge markup string → passed into `vault_row` / (new) vault-page chrome.

### Analog: the function to extend — `src/server.rs:526-546`

```rust
/// Phase 18 (D-05): OKF badge pill for vault cards. Defensive serde
/// (T-18-03): unparseable `okf_json` renders nothing; only typed integers
/// and the html-escaped version reach HTML (T-18-06). Zero new CSS — reuses
/// the `.menu-btn` pill + `<small>` precedent (D-05).
fn okf_badge_html(json: Option<&str>) -> String {
    let Some(j) = json else {
        return String::new();
    };
    let Ok(stats) = serde_json::from_str::<crate::okf::OkfStats>(j) else {
        return String::new();
    };
    match stats.version {
        None => r#"<small class="menu-btn">not an OKF bundle</small>"#.to_string(),
        Some(v) => format!(
            r#"<small class="menu-btn">OKF v{v} — {}/{total} concepts</small>"#,
            stats.concepts,
            v = html_escape(&v),
            total = stats.total,
        ),
    }
}
```

Conventions to replicate exactly: (a) unparseable JSON → empty string, never an error page; (b) every string that came from the blob goes through `html_escape` (defined `src/server.rs:1201-1206`); (c) zero new CSS — the pill stays `.menu-btn`. D-02 wraps the pill in a `<details>` (the pill becomes the `<summary>`); D-04 makes the `None` arm's pill clickable too, with the reason list (0 concepts / below 80 % / missing index.md / missing log.md) rendered from the already-parsed `stats`.

### Analog: scan-date formatting (no chrono) — `src/server.rs:548-552`

```rust
/// Epoch seconds → `YYYY-MM-DD` UTC (civil-from-days reference algorithm;
/// no chrono dependency, D-10). Deliberately standalone: the existing
/// `fmt_date` has a latent doy/doe mix-up for day-of-month (18-03 notes).
fn epoch_secs_to_utc_date(secs: u64) -> String {
```

D-04's "scan date" line must use `epoch_secs_to_utc_date(stats.scanned_at)` — do not add a date crate.

### Call sites (both home lists; D-03 "one shared component") — `src/server.rs:641-650`

```rust
            let badge = okf_badge_html(
                state
                    .store
                    .vault_okf(&a.vault.slug)
                    .await
                    .ok()
                    .flatten()
                    .as_deref(),
            );
            own_items.push_str(&vault_row(a, false, &badge));
```

(the shared-with-me list repeats identically at `src/server.rs:660-669`).

### Analog: `vault_row` already threads the badge through — `src/server.rs:617-625`

```rust
    format!(
        r#"<li><a class="vault-name" href="/vault/{s}/">{n}</a>{t}{b}<span class="vault-actions">{acts}</span></li>"#,
        s = slug,
        n = name,
        t = tag,
        b = okf_badge,
        acts = actions,
    )
}
```

`vault_row(a, can_share, okf_badge)` (`src/server.rs:570`) takes the badge as an opaque string — the `<details>` wrapper fits without touching the row layout (`.vault-list li` is a flex row, `assets/style.css:213-220`).

---

## 3. `details.menu` disclosure pattern — the model for the badge expansion (D-02)

**Role:** header dropdown markup + its JS + CSS. **Data flow:** server-rendered `<details>`; JS only adds ergonomics.

### Analog: account menu markup — `src/server.rs:1258-1266`

```rust
    <details class="menu">
      <summary class="menu-btn"><span class="avatar">{initial}</span> {label} <span class="caret">&#9662;</span></summary>
      <div class="menu-body">
        <a class="menu-identity" href="/account"><strong>{label}</strong>{email_line}<span class="menu-edit">&#9998; Edit display name</span></a>
        {admin_entry}
        <hr class="menu-sep">
        <form method="post" action="/logout"><button type="submit" class="btn-ghost">Log out</button></form>
      </div>
    </details>
```

Pattern: native `<details>` + `.menu-btn` summary + `.menu-body` panel; `html_escape` on every interpolated user value (`src/server.rs:1271-1273`).

### Analog: menu ergonomics JS — `src/server.rs:2163-2176`

```rust
const MENU_JS: &str = r#"<script>
// menu-ergonomics: one open menu at a time; Escape or an outside click closes it.
(function () {
  var menus = function () { return Array.prototype.slice.call(document.querySelectorAll("details.menu")); };
  document.addEventListener("click", function (e) {
    menus().forEach(function (d) {
      if (d.open && !d.contains(e.target)) d.removeAttribute("open");
    });
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") menus().forEach(function (d) { d.removeAttribute("open"); });
  });
})();
</script>"#;
```

If the badge uses `class="menu"` it inherits this behavior for free on MoriNet pages (MENU_JS ships in every `account_menu` header, `src/server.rs:1269/1274`). Note `.menu-body` is `position:absolute; right:0` (`assets/style.css:141-150`) — a badge inside a flex row may want the disclosure to render in-flow instead (plain `<details>` without `.menu-body`, or `.menu` scoping decision is a plan call; the CSS classes that exist are the constraint).

### Analog: the pill/body CSS to reuse — `assets/style.css:107-125, 141-150`

```css
.menu-btn {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  cursor: pointer;
  list-style: none;
  padding: 5px 11px;
  border: 1px solid var(--border);
  border-radius: var(--r-pill);
  background: transparent;
  font-weight: 600;
  font-size: .86rem;
  color: var(--text);
  user-select: none;
}
```
```css
.menu { position: relative; }
.menu-body {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 50;
  width: 16.5rem;
  padding: 6px;
  border-radius: var(--r-2);
  box-shadow: var(--shadow-2);
}
```

Phase 19 D-03 lock: **no new CSS without an Impeccable pass** — the badge detail must be built only from `.menu-btn`, `.menu`, `.menu-body`, `.menu-sep`, `.menu-label`, `small`, `ul/li` (all in `assets/style.css`).

---

## 4. `src/server.rs` — `vault_edit_page` realignment (UI-03 / D-06)

**Role:** server-rendered form handler. **Data flow:** `store.theme/allow_reshare/keep_page_numbers` → prefilled form → `Form<EditForm>` POST → whitelist validate → `store.set_*` → 303 redirect.

### Analog: the form markup to realign — `src/server.rs:2524-2542`

```rust
    let body = format!(
        r#"<section class="panel">
<h2>Edit vault</h2>
<form method="post" action="/vault/{s}/edit" class="stacked">
  <div class="field"><label for="ev-name">Name</label><input id="ev-name" type="text" name="name" value="{n}" required></div>
  <div class="field field-wide"><label for="ev-url">Source URL <small>(next Refresh pulls this repo)</small></label><input id="ev-url" type="url" name="url" value="{u}" placeholder="https://github.com/owner/repo"></div>
  <div class="field"><label><input id="ev-reshare" type="checkbox" name="allow_reshare"{c}> Allow sharees to re-share this vault</label></div>
  <div class="field"><label><input id="ev-keep" type="checkbox" name="keep_page_numbers"{k}> Keep chapter page numbers</label></div>{tf}
  <button type="submit">Save</button>
</form>
</section>
"#,
```

The layout engine is flex-wrap — `assets/style.css:254-256`:

```css
form.stacked { display: flex; flex-wrap: wrap; gap: 12px; align-items: flex-end; }
.field { display: flex; flex-direction: column; gap: 5px; min-width: 11rem; }
.field-wide { flex: 1 1 20rem; }
```

D-06's culprit confirmed by these rules: `.field-wide` (Source URL) eats a flex line, then the two checkbox `.field`s and the Theme `.field` (`src/server.rs:2510-2523`, built with the `sel()` selected-attribute idiom) share the next line with the Save button (`align-items: flex-end`). The minimal fix stays inside these classes (e.g. making the Theme field wrap onto its own line via `field-wide`, or reordering markup) — **no new CSS classes**.

### Analog: write-time whitelist + serde-default checkbox semantics — `src/server.rs:2459-2473` and `2560-2565`

```rust
#[derive(Debug, Deserialize)]
struct EditForm {
    name: String,
    #[serde(default)]
    url: String,
    allow_reshare: Option<String>,
    /// Checkbox semantics: field present = on, absent = off (like
    /// `allow_reshare`). REQ-VOPT-001.
    #[serde(default)]
    keep_page_numbers: Option<String>,
    /// mdBook theme name, `""` = auto (REQ-VOPT-002). Validated against the
    /// server-side whitelist in `vault_edit_submit` — reject, never coerce.
    #[serde(default)]
    theme: String,
}
```
```rust
    // D-07: server-side theme whitelist — an unknown value rejects the whole
    // POST (400), it is never coerced to a default. `""` = mdBook auto.
    const THEMES: [&str; 6] = ["", "light", "navy", "rust", "coal", "ayu"];
    if !THEMES.contains(&p.theme.as_str()) {
        return error_page(&format!("unknown theme \"{}\"", p.theme));
    }
```

This `#[serde(default)]` idiom is also the pattern D-01 should copy for the new `OkfStats` field (§1).

---

## 5. Theme flow: serve-time literals → mdBook localStorage (UI-02 / D-05)

**Role:** serve-time HTML rewriting pipeline. **Data flow:** `store.theme(slug)` (owner default, `""` = auto) → `vault.vault.theme` in `serve_vault_file` → `inject_vault_chrome` → `apply_vault_display_options` replaces the two inline literals in every built mdBook page; at runtime mdBook's own `book.js` prefers the reader's `localStorage['mdbook-theme']` over those literals (Phase 12 D-08/D-10: visitor localStorage wins).

### Analog: literals + whitelist + substitution — `src/server.rs:752-784`

```rust
/// The two inline-theme literals present in EVERY built mdBook page (D-10),
/// verified verbatim on a real built book. Exact-match replace only.
const THEME_LIGHT_LITERAL: &str = r#"default_light_theme = "light";"#;
const THEME_DARK_LITERAL: &str = r#"default_dark_theme = "navy";"#;

/// Review fix (LOW, defense-in-depth): the same whitelist the Edit form
/// enforces at write time (plan 12-02) — the serve-time rewrite only ever
/// substitutes one of these tokens, never a raw column value.
const THEME_WHITELIST: [&str; 5] = ["light", "navy", "rust", "coal", "ayu"];

fn apply_vault_display_options(html: &mut String, keep_page_numbers: bool, theme: &str) {
    ...
    if !theme.is_empty() && THEME_WHITELIST.contains(&theme) {
        // `contains` guards before any `replace` (T-12-13: no work on pages
        // without the literals, no regex engine).
        if html.contains(THEME_LIGHT_LITERAL) {
            let replacement = format!(r#"default_light_theme = "{theme}";"#);
            *html = html.replace(THEME_LIGHT_LITERAL, &replacement);
        }
```

### Verified: mdBook's own localStorage key — `data/repos/b3wiki/.morinet-build/book/book-609e4cb8.js:365-367, 426-430`

```js
        let theme = null;
        try {
            theme = localStorage.getItem('mdbook-theme');
```
```js
        if (store) {
            try {
                localStorage.setItem('mdbook-theme', theme);
            } catch {
                // ignore error.
            }
        }
```

D-05's "mdBook's own theme key" is `mdbook-theme` (confirmed in the vendored built book; `get_theme()` returns the saved value when present, else falls back to the serve-time `default_light_theme`/`default_dark_theme` literals). The Looks picker must write `localStorage.setItem('mdbook-theme', <palette>)` with the same try/catch defensiveness, then trigger mdBook's theme application (page reload is the zero-coupling option; calling into book.js internals is not — see no-analog list).

### Analog: chrome injection point into every served mdBook page — `src/server.rs:797-819`

```rust
fn inject_vault_chrome(bytes: Vec<u8>, keep_page_numbers: bool, theme: &str) -> Vec<u8> {
    let Ok(html) = std::str::from_utf8(&bytes) else {
        return bytes; // not UTF-8: leave binary/odd payloads alone
    };
    let mut html = html.to_string();
    // PASS 1 — vault chrome. Converted pages already reference zoom.css /
    // morinet.js: skip injection, but fall through to the option pass.
    if !html.contains("zoom.css") && !html.contains("morinet.js") {
        if let Some(pos) = html.rfind("</body>") {
            let mut out = String::with_capacity(html.len() + 8192);
            out.push_str(&html[..pos]);
            out.push_str("<style>");
            out.push_str(crate::obsidian::tree::ZOOM_CSS);
            out.push_str("</style><script>");
            out.push_str(crate::obsidian::tree::MORINET_JS);
            out.push_str("</script>");
            out.push_str(&html[pos..]);
            html = out;
        }
    }
```

Two delivery paths for the Looks JS: converted vaults get `theme/morinet.js` at build time (`src/obsidian/convert.rs:124-129`: `fs::write(theme.join("morinet.js"), tree::MORINET_JS)?;`, wired via `additional-js` in `src/obsidian/tree.rs:231-236`); native vaults get the same bytes inlined at serve time (above). Editing `assets/zoom.js` (the source of `tree::MORINET_JS`, `src/obsidian/tree.rs:282`) covers both — but note converted books are pre-built, so converted vaults only pick the change up on rebuild; the serve-time inline path is instant.

---

## 6. `assets/zoom.js` (served as `theme/morinet.js`) — Looks picker host (UI-02 / D-05)

**Role:** vanilla-JS IIFE injected into every served mdBook page. **Data flow:** reads/writes localStorage; builds DOM defensively; never assumes a framework.

### Analog: localStorage read/write with try/catch — `assets/zoom.js:8-14`

```js
    var MIN = 0.5, MAX = 3.0, STEP = 0.1;
    var KEY = "morinet-zoom";
    var baseW = null;
    function clamp(v) { return Math.min(MAX, Math.max(MIN, Math.round(v * 10) / 10)); }
    var current = clamp(parseFloat(localStorage.getItem(KEY)) || 1);
    function apply() {
        try { localStorage.setItem(KEY, String(current)); } catch (e) {}
```

### Analog: DOM construction + idempotent build — `assets/zoom.js:36-47, 71-76`

```js
    function mk(html, label, fn, id) {
        var b = document.createElement("button");
        b.type = "button";
        b.innerHTML = html;
        b.title = label;
        b.setAttribute("aria-label", label);
        if (id) b.id = id;
        b.addEventListener("click", fn);
        return b;
    }
    function build() {
        if (document.getElementById("km-zoom-bar")) { apply(); return; }
```
```js
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", build);
    } else {
        build();
    }
```

Conventions: ES5-flavored vanilla JS (no arrow functions/`let` in this file), `id` prefix conventions (`km-*` / `morinet-*`), `aria-label` on icon buttons, build-once guard, DOMContentLoaded guard.

---

## 7. `src/store.rs` — no schema change; accessors already exist

**Role:** SQLite persistence (sqlx, opaque TEXT blob). **Data flow:** unchanged — D-01 extends the JSON *inside* `okf_json`, so migration-free (schema-free blob, migration `0013_okf_conformance.sql:8`: `ALTER TABLE vaults ADD COLUMN okf_json TEXT;`).

### Analog: the blob accessors — `src/store.rs:974-994`

```rust
    /// Remember the latest OKF conformance scan (set at refresh/register;
    /// best-effort callers). Opaque JSON text — serde validates in Rust (D-03).
    pub async fn set_vault_okf(&self, slug: &str, json: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE vaults SET okf_json = ? WHERE slug = ?")
            .bind(json)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The stored OKF conformance JSON (None = never scanned; unknown slug
    /// collapses to the same None — no existence leak, T-18-04).
    pub async fn vault_okf(&self, slug: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT okf_json FROM vaults WHERE slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(v,)| v))
    }
```

Theme accessors for the owner-default flow: `theme()` / `set_theme()` at `src/store.rs:1363-1381` (caller owns the whitelist — D-07 comment). UI-02 stores nothing server-side (D-05), so `store.rs` likely needs **no change at all** in this phase.

---

## 8. `docs/morinet-docs/src/sharing-and-roles.md` — user-facing docs

Analog: itself (existing display-options/statistics prose). Update only if badge/theme behavior text changes (CONTEXT canonical ref). No code pattern applies.

---

## No-analog gaps (flagged for the planner)

1. **Injecting a "Looks" entry into mdBook's own settings/theme popup.** No existing code ever touches mdBook's generated UI: `assets/zoom.js` only *appends its own* toolbar (`#km-zoom-bar`) and never queries or mutates mdBook's `theme-toggle` button / `mdbook-theme-list` popup (markup visible in `data/repos/b3wiki/.morinet-build/book/index.html`: `theme-toggle ... aria-controls="mdbook-theme-list"`). Adding an entry *inside* mdBook's popup means DOM-surgery on minified book.js output across mdBook versions — genuinely unprecedented. Safer precedent-following alternative within locked intent: the picker writes `localStorage['mdbook-theme']` (verified key, §5) from a MoriNet-owned control (zoom-bar entry), rather than editing mdBook's popup DOM. If the plan insists on the popup itself, treat it as net-new JS with version-drift risk.
2. **Badge on the vault page.** `okf_badge_html` is called only from `index` (`src/server.rs:641, 660`); `/vault/{slug}/` serves raw mdBook `index.html` via `serve_vault_file` (`src/server.rs:730-744, 822+`) which has no `okf_json` in hand (`AccessibleVault` carries `keep_page_numbers`/`theme` but not the OKF blob). D-03's "vault page" placement needs a new data fetch in the serve path plus an injection point — closest analog is `inject_vault_chrome`'s `</body>` insertion, but no existing code injects *MoriNet app chrome with per-vault DB data* into served book pages (chrome is static bytes only).
3. **Clickable "not an OKF bundle" pill.** Today it is a static `<small class="menu-btn">` (`src/server.rs:538`); no existing pill anywhere is interactive. The `<details>` mechanics exist (§3) but the summary-styled-as-pill-inside-list-row combination (`.menu` is `position:relative` + `.menu-body` is `position:absolute; right:0` — tuned for the header, not a flex list row) has no in-row precedent; expect layout fiddling with existing classes only.
4. **Evolving a persisted JSON blob struct with a new field.** `#[serde(default)]` precedent exists (`EditForm`, `src/server.rs:2462-2472`) but only for *form* structs; no struct persisted into `okf_json` has ever gained a field. The backward-compat test pattern to copy is `okf_badge_html_unparseable_is_empty` (`src/server.rs:10480-10482`) plus a new "old JSON without the field deserializes" test.

## Missing deps / features

**None required — reuse is fully possible with what Cargo.toml already provides:**
- `serde = { version = "1.0", features = ["derive"] }` and `serde_json = "1"` — `#[serde(default)]` field extension, blob (de)serialization.
- `axum 0.8`, `sqlx 0.8 (sqlite)` — form handling and the unchanged `okf_json` column.
- No new date crate: `epoch_secs_to_utc_date` (`src/server.rs:551`) formats `scanned_at` (chrono deliberately absent, "no chrono dependency, D-10").
- No new JS tooling: `theme/morinet.js` is a hand-written ES5 IIFE (`assets/zoom.js`), copied verbatim at build/serve time.
- Nothing in the phase needs `zip`, `openidconnect`, or any feature-flag addition.

**Verdict: zero new dependencies, zero new Cargo features** — consistent with CONTEXT's "reuse locked, new deps forbidden".
