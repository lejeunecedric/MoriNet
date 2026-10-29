# Phase 12 Patterns — demo-lock + vault display options

**Mode:** brownfield (every change mimics an existing in-repo analog).
All excerpts below are VERBATIM from the current tree (2026-10-28). Line numbers
verified with grep before writing. `gsd-sdk query agent-skills gsd-pattern-mapper`
returned empty; conventions follow the GSD planner template + this repo's prior phases.

---

## 1. `migrations/0009_vault_display_options.sql` — CREATE

**Role:** schema migration adding the two per-vault display columns.
**Closest analog:** `migrations/0008_shares_stats.sql:1-10` (header comment + `ALTER TABLE vaults ADD COLUMN ... NOT NULL DEFAULT`).

**Registration:** there is NO manual registry. `src/store.rs:98` and `:108`
(both `connect` and `connect_memory`) run:

```rust
        sqlx::migrate!("./migrations").run(&pool).await?;
```

`sqlx::migrate!` embeds every file in `migrations/` at compile time and applies
unapplied ones in numeric order — dropping the correctly-named file in the dir
IS the registration. Next number: **0009** (0008 is the last).

**Style to mimic** (`migrations/0008_shares_stats.sql:1-10`):

```sql
-- Phase 10.1 (REQ-VLT-010, REQ-VLT-011): direct email sharing + access log.
--
--   vault_shares  user-to-user share by email (reader|writer), with a
--                 per-share `hidden` flag ("Remove from my view") and a
--                 vault-level `allow_reshare` gate on the vaults table.
--   access_log    lightweight best-effort hit log feeding the per-vault
--                 statistics page; pruned to 90 days at boot.

ALTER TABLE vaults ADD COLUMN allow_reshare INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vaults ADD COLUMN last_refreshed_at TEXT;
```

Per CONTEXT: `ALTER TABLE vaults ADD COLUMN keep_page_numbers INTEGER NOT NULL DEFAULT 0;`
and `ALTER TABLE vaults ADD COLUMN theme TEXT NOT NULL DEFAULT '';` — same
one-line-per-column `ALTER TABLE vaults ADD COLUMN ... NOT NULL DEFAULT` shape,
with a `-- Phase 12 (REQ-VOPT-001, REQ-VOPT-002): ...` header comment.

**Migration-shape test analog** (`src/store.rs:1789-1802`) — mirror this as a
`display_options_migration_shape` test:

```rust
    #[tokio::test]
    async fn shares_stats_migration_shape() {
        let s = store().await;
        s.seed_vault("shape", "Shape", Path::new("/tmp/shape/book"))
            .await
            .unwrap();
        // New vaults columns: allow_reshare defaults to 0, last_refreshed_at
        // is stamped by seed_vault.
        let (allow, refreshed): (i64, Option<String>) = sqlx::query_as(
            "SELECT allow_reshare, last_refreshed_at FROM vaults WHERE slug = 'shape'",
        )
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert_eq!(allow, 0, "allow_reshare defaults to 0");
```

---

## 2. `src/store.rs` — MODIFY

**Role:** `Vault` struct gains `keep_page_numbers: bool` + `theme: String`;
add getter/setter pairs next to `set_allow_reshare`; extend every row→`Vault`
mapping with the two new columns.

**Struct analog** (`src/store.rs:14-24`):

```rust
#[derive(Debug, Clone)]
pub struct Vault {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub book_path: String,
    /// `global` (shared through groups) or `personal` (solo, owner only).
    pub scope: String,
    /// Owner email for a personal vault; None for a global one.
    pub owner_email: Option<String>,
}
```

**Getter/setter analog** (`src/store.rs:1206-1223`) — copy this shape exactly
for `keep_page_numbers()/set_keep_page_numbers()` (bool ↔ INTEGER) and a
`theme()/set_theme()` pair (TEXT):

```rust
    /// Current value of the re-share toggle (prefills the owner's Edit form).
    pub async fn allow_reshare(&self, slug: &str) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT allow_reshare FROM vaults WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(v,)| v != 0).unwrap_or(false))
    }

    /// Vault-level re-share toggle (part of the owner's Edit form).
    pub async fn set_allow_reshare(&self, slug: &str, on: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE vaults SET allow_reshare = ? WHERE slug = ?")
            .bind(on)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

**Row-mapping sites that must gain the new columns** (tuple `query_as` + manual
struct construction — every one of these breaks until updated):
`accessible_vaults` (`src/store.rs` — checker fix W-1: authoritative site list is `grep -n "Vault {" src/store.rs`; mappings inside `accessible_vaults` at ~:691-770, the :201-224 excerpt is a different function), `list_all_vaults`,
and the other `FROM vaults` selects at `:232`, `:270`. Example (`:214-224`):

```rust
                Ok(rows
                    .into_iter()
                    .map(|(id, slug, name, book_path, scope, owner_email)| Vault {
                        id,
                        slug,
                        name,
                        book_path,
                        scope,
                        owner_email,
                    })
                    .collect())
```

Note: `serve_vault_file` reads options via `vault.vault.*` (the `AccessibleVault`
at `src/store.rs:47-54`), so the columns must ride on the `Vault` struct itself,
not a side query.

---

## 3. `src/config.rs` — MODIFY

**Role:** new `[security]` section + `MORINET_LOCKED_ACCOUNTS` env override →
`RuntimeConfig.locked_accounts: Vec<String>`.

**Section analog** (`src/config.rs:29-42`) — `AdminSection` is the exact
pattern for a new defaulted section on `FileConfig`:

```rust
    /// Bootstrap admin credentials (fallback when the env vars are absent).
    #[serde(default)]
    pub admin: AdminSection,
}

/// `[admin]` section: bootstrap admin credentials written by `morinet init`.
/// Env vars `MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD` take precedence.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdminSection {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}
```

→ add `#[serde(default)] pub security: SecuritySection,` to `FileConfig`
(`:16-32`) with `SecuritySection { #[serde(default)] pub locked_accounts: Vec<String> }`.

**Env-override precedence analog** (`src/config.rs:150-157`, inside `resolve`):

```rust
    let admin_email = std::env::var(ENV_ADMIN_EMAIL)
        .ok()
        .or_else(|| cfg.admin.email.clone().filter(|s| !s.is_empty()))
        .ok_or_else(|| anyhow::anyhow!("missing admin email: set {ENV_ADMIN_EMAIL}"))?;
```

Locked accounts never fail hard (empty default), so the shape is
`std::env::var(ENV_LOCKED_ACCOUNTS).ok()` split on `,` + trim + lowercase,
`.or_else(|| if cfg.security.locked_accounts.is_empty() { None } else { Some(...) })`
— env wins when set, file section otherwise. Add
`pub const ENV_LOCKED_ACCOUNTS: &str = "MORINET_LOCKED_ACCOUNTS";` next to
`ENV_ADMIN_EMAIL` (`:12-13`) and `pub locked_accounts: Vec<String>,` to
`RuntimeConfig` (`src/config.rs:110-126`).

**Config-test analog** (`src/config.rs:370-394`, `resolve_env_admin_overrides_file_section`):
save/restore env under `ENV_LOCK` mutex (`:181`), `toml::from_str` a file
section, assert env wins. Fixture creds stay neutral (`test-pass-9f3a`).

---

## 4. `src/server.rs` — `inject_vault_chrome` + its two call sites — MODIFY

**Role:** make serve-time option rewrites vault-aware; split chrome injection
from option rewrite (two distinct passes — CONTEXT pitfall).

**Exact current signature** (`src/server.rs:610`):

```rust
fn inject_vault_chrome(bytes: Vec<u8>) -> Vec<u8> {
    let Ok(html) = std::str::from_utf8(&bytes) else {
        return bytes; // not UTF-8: leave binary/odd payloads alone
    };
    if html.contains("zoom.css") || html.contains("morinet.js") {
        return bytes;
    }
    let Some(pos) = html.rfind("</body>") else {
        return bytes;
    };
```

⚠ The early-return at `:614-616` (converted vaults already wire chrome) must
NOT gate the new option rewrites — restructure so the theme/page-number rewrite
runs for ALL HTML pages regardless of the chrome early-return.

**Call site 1** — directory→index branch (`src/server.rs:666-674`), inside
`serve_vault_file(state: &AppState, email: &str, slug: &str, rest: &str)` (`:631`),
which already holds `let Some(vault) = accessible.iter().find(...)` (`:649-651`):

```rust
                Ok(bytes) => {
                    // Successful serve: record the hit without ever letting a
                    // store failure touch the response.
                    log_web_access(state, email, slug, rel);
                    (
                        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                        inject_vault_chrome(bytes),
                    )
                        .into_response()
                }
```

**Call site 2** — file branch (`src/server.rs:679-688`):

```rust
        _ => match tokio::fs::read(&full).await {
            Ok(bytes) => {
                log_web_access(state, email, slug, rel);
                let ct = content_type(rel);
                let body = if ct.starts_with("text/html") {
                    inject_vault_chrome(bytes)
                } else {
                    bytes
                };
```

Pass the two option values from `vault.vault` (the `Vault` row) into both
calls. Injected snippets go in raw-string consts — pitfall #25: no `{ }` inside
`format!` HTML. The keep-numbers override (CONTEXT):
`<style>.chapter-item a > strong[aria-hidden="true"]{display:inline;}</style>`
before `</body>` — the inverse of the global hide rule at `assets/zoom.css:68`
(`.chapter-item a > strong[aria-hidden="true"] { display: none; }`, pulled in
via `pub const ZOOM_CSS: &str = include_str!("../../assets/zoom.css");`,
`src/obsidian/tree.rs:286` — do NOT edit either; the toggle only adds the
serve-time override, keeping `zoom_css_hides_mdbook_chapter_numbers`
(`src/obsidian/tree.rs:516-520`) green).
Theme rewrite: exact string replace of the two literals
`default_light_theme = "light";` and `default_dark_theme = "navy";` (verified
verbatim in built books); inert when absent.

---

## 5. `src/server.rs` — `EditForm` + `vault_edit_page` + `vault_edit_submit` — MODIFY

**Role:** add `keep_page_numbers` checkbox + `theme` select (whitelist
`""|light|navy|rust|coal|ayu`).

**Form struct analog** (`src/server.rs:1944-1952`):

```rust
/// `POST /vault/{slug}/edit` fields. `allow_reshare` follows HTML checkbox
/// semantics: present (any value) = on, absent = off.
#[derive(Debug, Deserialize)]
struct EditForm {
    name: String,
    #[serde(default)]
    url: String,
    allow_reshare: Option<String>,
}
```

**Page prefill + checkbox markup** (`src/server.rs:1978-1994`):

```rust
    let reshare = state.store.allow_reshare(&slug).await.unwrap_or(false);
    let body = format!(
        r#"<section class="panel">
<h2>Edit vault</h2>
<form method="post" action="/vault/{s}/edit" class="stacked">
  <div class="field"><label for="ev-name">Name</label><input id="ev-name" type="text" name="name" value="{n}" required></div>
  <div class="field field-wide"><label for="ev-url">Source URL <small>(next Refresh pulls this repo)</small></label><input id="ev-url" type="url" name="url" value="{u}" placeholder="https://github.com/owner/repo"></div>
  <div class="field"><label><input id="ev-reshare" type="checkbox" name="allow_reshare"{c}> Allow sharees to re-share this vault</label></div>
  <button type="submit">Save</button>
</form>
</section>
"#,
        s = html_escape(&slug),
        n = html_escape(&a.vault.name),
        u = html_escape(&source),
        c = if reshare { " checked" } else { "" },
    );
```

The `<select name="role">` markup pattern for the theme select is at
`src/server.rs:2108`: `<select id="sh-role" name="role"><option value="reader">reader</option>...`.

**Submit persistence analog** (`src/server.rs:2006-2030`): gate first
(`can_edit_vault`), trim/validate, one store call per field with
`error_page(&format!("could not ...: {e}"))` on `Err`, then
`Redirect::to("/")`:

```rust
    if let Err(e) = state
        .store
        .set_allow_reshare(&slug, p.allow_reshare.is_some())
        .await
    {
        return error_page(&format!("could not update the re-share setting: {e}"));
    }
    Redirect::to("/").into_response()
```

Theme whitelist rejection: same shape as `let role = if p.role == "writer" { "writer" } else { "reader" };`
(`src/server.rs:2139-2143`) — but CONTEXT says reject anything off-whitelist
via `error_page`, not coerce. Locked-account gate goes after `can_edit_vault`
(see §8).

---

## 6. `src/server.rs` — home page: vault rows + Load form (locked JS flag) — MODIFY

**Role:** render a server-side "current user is locked" flag into `index` and
intercept the Delete confirm + Load-vault submit with
`alert("demo account can't add or suppress vaults")`.

**Delete form to intercept** — `vault_row` (`src/server.rs:474-477`), the
inline `onsubmit="return confirm(...)"` is the hook:

```rust
        actions.push_str(&format!(
            r#"<a class="btn-ghost" href="/vault/{s}/edit">Edit</a><form method="post" action="/vaults/{s}/refresh"><button type="submit" class="btn-ghost">Refresh</button></form><a class="btn-ghost" href="/vault/{s}/share">Share</a><a class="btn-ghost" href="/vault/{s}/stats">Statistics</a><form method="post" action="/vaults/{s}/delete" onsubmit="return confirm('Remove this vault from your collection?')"><button type="submit" class="btn-quiet">Delete</button></form>"#,
            s = slug,
        ));
```

**Load-vault form** — inside `index` (`src/server.rs:568-576`), action
`/vaults/create`, submit button text `Load vault`:

```rust
<section class="panel">
<h2>Load a vault</h2>
<form method="post" action="/vaults/create" class="stacked">
  <div class="field"><label for="lv-name">Name</label><input id="lv-name" type="text" name="name" placeholder="e.g. My Wiki"></div>
  <div class="field field-wide"><label for="lv-url">GitHub URL</label><input id="lv-url" type="url" name="url" required placeholder="https://github.com/owner/repo"></div>
  <div class="field"><label for="lv-token">Token <small>(optional)</small></label><input id="lv-token" type="text" name="token" placeholder="private repos only"></div>
  <button type="submit">Load vault</button>
</form>
```

**Flag injection point** — the `index` html `format!` (`src/server.rs:552-584`)
already interpolates a raw-string JS const (`autofill = AUTOFILL_NAME_JS,` at
`:583`) before `</body>` — same slot pattern for the locked-flag `<script>`
(discretion call: inline `<script>` const vs data attribute; must be inert for
non-locked users). Compute the flag in `index` from
`state.cfg.locked_accounts` vs `claims.email` (case-insensitive; admins never
locked — `role_for` pattern at `can_edit_vault`, `src/server.rs:1931-1934`).

---

## 7. Route-test style — oneshot + login cookie (quote for executor)

**Helpers** (`test_router` at `src/server.rs:2818`, `form_headers` at `:3152`, `login_as` at `:3388` — checker fix W-2): `test_router()` →
`(Router, SharedState)`; `req(app, method, uri, headers, body)` wraps
`app.oneshot(...)`; `login_cookie(app)` / `login_as(app, email, pw)` POST
`/login` and return the `Set-Cookie` value; `form_headers(&cookie)` =
content-type urlencoded + cookie.

**Full login-then-POST test to mimic** (`src/server.rs:3184-3216`):

```rust
    #[tokio::test]
    async fn edit_submit_renames_resources_and_sets_reshare() {
        let (app, state) = test_router().await;
        seed_edit_fixture(&state).await;
        let cookie = login_as(&app, "pal@example.com", "pw").await;
        let res = req(
            app.clone(),
            "POST",
            "/vault/mine/edit",
            form_headers(&cookie),
            Some(Body::from(
                "name=New+Name&url=https%3A%2F%2Fgithub.com%2Fo%2Fr&allow_reshare=on",
            )),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let vaults = state
            .store
            .accessible_vaults("pal@example.com")
            .await
            .unwrap();
        assert_eq!(vaults[0].vault.name, "New Name", "renamed in store");
```

Checkbox-off analog (`edit_submit_without_checkbox_clears_reshare`,
`src/server.rs:3218-3237`) posts without the field and asserts the toggle
cleared — mirror for `keep_page_numbers`. Fixture creds neutral
(`test-pass-9f3a`, `test_config()` at `src/server.rs:2782-2794`); each test
its own data_dir/vault name (pitfalls #16/#23). Locked-account refusal tests:
add `locked_accounts` to `test_config()`'s `RuntimeConfig` (new field) and
assert `error_page` status (`StatusCode::BAD_REQUEST`, see §8) on scripted
POSTs — the popup is UX only, tests hit the server gate.

---

## 8. Locked-account server gate — refusal sites — MODIFY

**Role:** refuse load + delete + edit-submit + share add/revoke for locked
non-admin accounts.

**Refusal helper** (`src/server.rs:2674-2685`):

```rust
fn error_page(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(format!(
            r#"<html lang="en"><head><meta charset="utf-8"><title>MoriNet — Error</title>
<link rel="stylesheet" href="/assets/style.css"></head><body>
<p style="color:red">{}</p><p><a href="/admin/vaults">Back to vaults</a></p>
</body></html>"#,
            html_escape(msg)
        )),
    )
        .into_response()
}
```

**Gate placement analog** — first-statement guard in each handler, e.g.
`delete_my_vault` (`src/server.rs:1908-1922`) currently opens with the store
call; insert the locked check before it, same position as
`if !can_edit_vault(&state, &claims.email, &slug).await { return ... }`
(`src/server.rs:1960-1962`, `2006-2008`) and the `can_share` guards
(`src/server.rs:2127-2134`, `2158-2165`). `load_personal_vault`
(`src/server.rs:2413-2421`) puts its first validation right after the
signature — the locked check goes there, before quota/clone (no network on
refusal, same principle as the quota-first comment at `:2434`).
Helper location (discretion): a cheap **async** `async fn is_locked(state: &AppState, email: &str) -> bool` (fix W-3r — `role_for` is async, so the helper must be too; call sites `.await` it)
on `AppState`/free fn in server.rs reading `state.cfg.locked_accounts`
(case-insensitive `eq_ignore_ascii_case`) + `role_for(email).await != Some("admin".into())`
— `role_for` usage pattern at `src/server.rs:1931`.

---

## Files NOT to touch (reference only)

- `assets/zoom.css` (hide rule `:68`) and `src/obsidian/tree.rs` `ZOOM_CSS`
  (`:286`) — global hide stays; toggle adds serve-time override only.
- `src/obsidian/tree.rs::book_toml()` — ROADMAP `default-theme` approach is
  SUPERSEDED by the serve-time rewrite (CONTEXT decisions).

## PATTERN MAPPING COMPLETE
