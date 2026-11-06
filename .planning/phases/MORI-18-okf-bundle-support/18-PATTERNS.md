# Phase 18: okf-bundle-support — Pattern Map

**Mapped:** 2026-11-06 · **Anchors verified against HEAD** (server.rs 9696 lines, store.rs 4036, api.rs 1913, mcp.rs 610 — all excerpts taken via grep windows, never whole-file reads).
Style reference: `MORI-17-.../17-PATTERNS.md` (not duplicated here).

---

## 1. Vault refresh/register hook — where the OKF scan attaches (D-02)

The Phase 13 change-capture lives in **server.rs**, best-effort after a successful `seed_vault`. Three register/refresh call sites matter:

**a) Git refresh — `refresh_my_vault`, src/server.rs:3027-3041:**
```rust
    match state.store.seed_vault(&slug, &name, &book_dir).await {
        Ok(()) => {
            // Best-effort change log (D-04): only when HEAD actually moved;
            // any failure is eprintln'd and the redirect stays byte-identical.
            if let (Some(old), Some(new)) = (old_head, git_head_oid(&repo_dir)) {
                if old != new {
                    if let Err(e) =
                        record_vault_changes(&state.store, &slug, &repo_dir, old, new).await
                    {
                        eprintln!("vault-changes: refresh log failed for {slug}: {e}");
                    }
                }
            }
            Redirect::to(&format!("/vault/{slug}/")).into_response()
```
The `record_vault_changes` helper itself (src/server.rs:2964-2986) shows the error-propagation contract: `anyhow::Result` up to the caller's `eprintln!` arm — never blocks the redirect.

**b) Personal load (register) — `load_personal_vault`, src/server.rs:3113-3121:**
```rust
    match state
        .store
        .create_personal_vault(&claims.email, &slug, &name, &book_dir)
        .await
    {
        Ok(()) => {
            // Remember the upstream so the vault can be refreshed later.
            let _ = state.store.set_vault_source(&slug, url).await;
```

**c) Admin create — src/server.rs:3826:** `Ok(book_dir) => match state.store.seed_vault(&p.slug, &p.name, &book_dir).await {`

The **source dir** (what D-02 says to scan, never `.morinet-build/`) is resolved by `webdav::source_dir_of(&book_dir)` — see vault_stats usage at src/server.rs:2798.

> **Use this for:** the OKF scan call site — same `Ok(()) =>` arms, same `if let Err(e) = … { eprintln!("okf-scan: …") }` best-effort shape, scanning `webdav::source_dir_of(&book_dir)`.

---

## 2. `src/obsidian/frontmatter.rs` public API (D-04)

Exact signatures (src/obsidian/frontmatter.rs:20-63):
```rust
pub struct Frontmatter { fields: BTreeMap<String, Field> }   // :20
impl Frontmatter {
    pub fn is_empty(&self) -> bool                            // :25
    pub fn scalar(&self, key: &str) -> Option<&str>           // :29
    pub fn list(&self, key: &str) -> Vec<String>              // :37
    pub fn title(&self) -> Option<&str>                       // :44
    pub fn page_type(&self) -> Option<&str>                   // :49  (scalar("type"))
    pub fn is_contested(&self) -> bool                        // :54
}
pub fn split(md: &str) -> (Frontmatter, &str)                 // :63
```
Contract (file header, :1-6): flat `key: value`, inline lists `tags: [a, b]`, block lists tolerated, everything stays a string, **malformed frontmatter → treated as absent** (`split` returns `(empty, whole input)` — the page is never truncated).

> **Use this for:** the conformance scan and D-07 metadata — `split()` + `page_type()`/`scalar("status")`/`list("tags")`. Nested `sources:` list-of-maps arrives as an empty `List` (key with no inline value → pending block-list collector, :114-118) — treat as opaque presence flag only, per D-04.

---

## 3. Migration style + `sqlx::migrate!` pickup

Pickup: src/store.rs:117 and :127 — `sqlx::migrate!("./migrations").run(&pool).await?;` (embedded at compile time; a new file `migrations/0013_*.sql` is picked up automatically on next build; tests use `Store::connect_memory()` which runs the same migrator).

**0011 (ALTER style), migrations/0011_vault_changes_forced_actor.sql:**
```sql
-- Phase 15 (CONF-04, D-05): force-overwrite audit columns.
--   forced  1 when the write overwrote a conflicting file …
ALTER TABLE vault_changes ADD COLUMN forced INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vault_changes ADD COLUMN actor TEXT NOT NULL DEFAULT '';
```

**0012 (CREATE style), migrations/0012_user_identities.sql:**
```sql
-- Phase 17 (SSO-05/SSO-06, D-11, D-05): generic OIDC SSO foundations.
--   user_identities  links an external OIDC identity …
CREATE TABLE IF NOT EXISTS user_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    …
    UNIQUE(provider, iss, sub)
);
```

> **Use this for:** `migrations/0013_okf_conformance.sql` — naming `NNNN_snake_case.sql`, header comment `-- Phase 18 (D-0x): …` explaining each column; D-03's single JSON column = `ALTER TABLE vaults ADD COLUMN okf_json TEXT;` (NULL = never scanned), 0011-style ALTER.

---

## 4. `vaults` table + store helper that updates a vault row (D-03)

Schema is 0001-era CREATE plus per-phase ALTERs — migrations/0002_vaults.sql:
```sql
CREATE TABLE IF NOT EXISTS vaults (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    book_path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```
Later columns added by ALTER: `source_url` (0006), `allow_reshare`, `last_refreshed_at` (0008), `keep_page_numbers`, `theme` (0009) — the table accretes nullable/defaulted columns, never rebuilds.

Store helper analog, src/store.rs:964-972:
```rust
    /// Remember where a vault came from (set at load time).
    pub async fn set_vault_source(&self, slug: &str, url: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE vaults SET source_url = ? WHERE slug = ?")
            .bind(url)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```
Read-side analog: `vault_source` (:955-962, `query_as::<(Option<String>,)>` + `fetch_optional`).

> **Use this for:** `set_vault_okf(slug, json)` / `vault_okf(slug)` pair — same UPDATE-by-slug / SELECT-by-slug shape; JSON string column keeps sqlx SQLite tuple reads trivial.

---

## 5. Vault card / vault page badge attach points (D-05)

**Vault card:** the single row renderer `vault_row`, src/server.rs:527-581. The existing small-label analog is the `(private)` tag (:532-536, :574-580):
```rust
    let tag = if a.personal {
        " <small>(private)</small>"
    } else {
        ""
    };
    …
    format!(
        r#"<li><a class="vault-name" href="/vault/{s}/">{n}</a>{t}<span class="vault-actions">{acts}</span></li>"#,
```
> **Use this for:** the card badge — same `{t}` interpolation slot after the vault name, HTML-escaped, empty string when never scanned.

**Vault page:** chrome is injected at serve time — `inject_vault_chrome` (src/server.rs:735-758) inlines `ZOOM_CSS`/`MORINET_JS` (src/obsidian/tree.rs:279+, the zoom toolbar + Close-vault pill built in assets/zoom.js:58-65, `id="morinet-close-vault"`, styled in assets/zoom.css:45+). A page badge would ride the same injection path (or the vault_stats/edit panel).

**Existing pill/badge CSS classes (no new CSS allowed, D-05):**
- `assets/style.css:32` — `--r-pill: 999px;` (the pill radius token)
- `assets/style.css:106-124` — `.menu-btn` — *"pill controls (admin entry, account trigger)"*: `border: 1px solid var(--border); border-radius: var(--r-pill); padding: 5px 11px; font-weight: 600; font-size: .86rem;`
- `assets/style.css:190-204` — `.subnav a` — pill tabs, same `var(--r-pill)` recipe
- `#morinet-close-vault` (zoom.css:46+) — the in-page pill
- `<small>` tag in `vault_row` — the zero-CSS small-label precedent

> **Use this for:** the "OKF v0.2 — 42/45 concepts" pill: reuse `.menu-btn`-style pill or the `<small>` precedent; zero new CSS satisfies D-05.

---

## 6. GET action route on a vault — ACL gate + not-found pattern (D-06)

Route registration, src/server.rs:257: `.route("/vault/{slug}/stats", get(vault_stats))`

Handler signature + gate, src/server.rs:2760-2780:
```rust
async fn vault_stats(
    State(state): State<SharedState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Response {
    …
    let accessible = state
        .store
        .accessible_vaults(&claims.email)
        .await
        .unwrap_or_default();
    let Some(entry) = accessible.iter().find(|a| a.vault.slug == slug) else {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    };
```
(gate at :2778-2780.)

⚠️ **Divergence to flag:** the *web* layer answers 403 (`vault_stats` :2779, `serve_vault_file` :765), while the Phase 16 **API** answers 404-on-invisible — src/api.rs:367-369:
```rust
        Err(ApiReadError::NotFound) | Err(ApiReadError::NoSuchFolder) => {
            return problem(StatusCode::NOT_FOUND, "not_found", "vault not found")
        }
```
D-06 demands the 404 rule → follow the api.rs shape (invisible vault → 404, never 403), not `vault_stats`' 403.

> **Use this for:** `GET /vault/<slug>/okf-export.zip` — axum route line beside :257, handler signature like `vault_stats`, ACL via `access_for(…).can_read()` (serve_vault_file:763-765) but answering **404** when inaccessible; serve bytes like `api_read_note` (raw body + content-type).

---

## 7. `/api/v1` note response construction (D-07)

**Note list** — src/api.rs:424-429 (inside `api_list_notes`):
```rust
    Json(serde_json::json!({
        "notes": page,
        "has_more": has_more,
        "next_cursor": next_cursor,
    }))
```
⚠️ **Shape tension:** `"notes"` is currently a bare array of path **strings** (test pin: src/api.rs:1149 `assert_eq!(j["notes"], serde_json::json!(["a.md", "b.md"]))`). Turning entries into objects `{path, frontmatter}` is a breaking change — D-07 says additive-only. Planner must resolve: e.g. a parallel `"frontmatter_by_path"` map, a per-note metadata sub-endpoint, or frontmatter only on read.

**Note read** — src/api.rs:467-474: `api_read_note` serves **raw `text/markdown` bytes with ETag** (D-06/B3: "RAW markdown source … never mdBook build output") — no JSON object exists to extend. A `"frontmatter"` object therefore needs either a new JSON read variant or header/`?meta=` affordance; the `read_note` helper returning `Vec<u8>` (see mcp.rs:207-209 wrapper comment) is the plumbing to extend.

Other JSON construction sites for style: `api_index` src/api.rs:303-312 (bare object, no envelope, D-08 style); `api_search` :449 `Json(hits)` (bare array of `Hit` — src/mcp.rs:69).

> **Use this for:** the D-07 additive field — `serde_json::json!` construction, bare-object convention, and the trust-tier derivation as a pure fn beside `list_notes`/`read_note` helpers.

---

## 8. MCP list/search tool definitions + schemars params (D-08)

Input structs, src/mcp.rs:113-129:
```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListNotesParams {
    /// Vault slug.
    pub slug: String,
    /// Optional sub-folder inside the vault (defaults to the root).
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Case-insensitive substring to find in the markdown of accessible vaults.
    pub query: String,
    /// Restrict to one vault slug (defaults to every accessible vault).
    #[serde(default)]
    pub slug: Option<String>,
}
```
Tool wrappers, src/mcp.rs:184-199 (`list_notes`) and :222-251 (`search_notes`) — thin wrappers: *"all ACL + path logic lives in crate::api (D-04)"*, e.g.:
```rust
    #[tool(description = "List markdown notes of a vault, optionally under a sub-folder")]
    pub async fn list_notes(
        &self,
        Parameters(p): Parameters<ListNotesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match crate::api::list_notes(&self.store, &self.email, &p.slug, p.path.as_deref()).await {
```
> **Use this for:** D-08 — add `#[serde(default)] pub r#type: Option<String>` / `pub tags: Option<Vec<String>>` to both structs (doc-comment becomes the schema description; schemars 1 + `#[tool]` regenerates automatically), and apply filters **after** the `crate::api::list_notes`/`search_notes` ACL call inside the wrapper or the shared helper.

---

## 9. Test fixture conventions

**No `tempfile`/`TempDir` anywhere** (absent from Cargo.toml dev-deps and all of src/). The house style is a tagged dir under `std::env::temp_dir()`, cleaned first — src/mcp.rs:265-270:
```rust
    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morinet-mcp-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
```
Store side: `Store::connect_memory()` (src/mcp.rs:261-263). Composite fixture pattern — src/api.rs:706-717 `seed_vault(tag, slug) -> (Store, PathBuf)` (keeps `_src` alive so the dir isn't dropped).

**Converter pin test** — src/obsidian/convert.rs:326-331:
```rust
        // Frontmatter consumed: meta block present, raw YAML gone.
        assert!(a.contains("<div class=\"note-meta\">"));
        assert!(
            !a.starts_with("---"),
            "frontmatter must not survive into src"
        );
```
> **Use this for:** OKF scan unit tests — `tmpdir("okf-v02")`-style neutral fixtures (one v0.2, one v0.1, one not-OKF per CONTEXT specifics), `connect_memory()` store, and D-09: the pin test above must stay green untouched.

---

## 10. Cargo.toml — dependency audit for `zip`

Verified: **no `zip`, `flate2`, `tar`, or any archive crate** in `[dependencies]` or `[dev-dependencies]`.

Relevant current flags (Cargo.toml):
- `sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio", "migrate"] }`
- `git2 = { version = "0.19", features = ["vendored-openssl"] }` (pre-existing OpenSSL for git; unrelated to TLS policy)
- TLS policy comment on the Phase 17 addition: *"the ONLY new runtime dependency. default-features=false keeps native-tls out; reqwest + rustls-tls route through the existing single rustls 0.23"* — `lettre` likewise pinned to `tokio1-rustls-tls`. The rustls-only rule is about **TLS stacks**.

**Conflict check for `zip = { version = "…", default-features = false, features = ["deflate"] }`:** none foreseen — `deflate` pulls `flate2` (pure-Rust `miniz_oxide` backend by default), no TLS, no async runtime. It becomes the **second** crate added after the Phase 17 "ONLY one" comment — D-06 explicitly allows exactly one new archive crate, so the comment must be updated to scope it (Phase 18 = zip). `zip`'s default features also enable zstd/xz/bzip2/etc. — `default-features = false` is mandatory to keep the tree lean.

> **Use this for:** the single permitted dependency addition + updating the "ONLY new runtime dependency" comment to reflect the phase policy.

---

## No analog (genuinely new in this phase)

1. **Zip creation/streaming** — no archive-writing code exists anywhere; `api_read_note` (api.rs:467+) is the closest *byte-serving* analog (content-type + raw body) but nothing builds an archive. In-memory `zip::ZipWriter<std::io::Cursor<Vec<u8>>>` is new ground.
2. **`Content-Disposition: attachment` download** — no route in server.rs serves a download-disposition response today.
3. **Directory-tree conformance scan with version guess** — `crate::mcp::list_md_files` (mcp.rs:39, skips hidden dirs — conveniently already excludes `.morinet-build/`/`.git/`) is the walk analog, but the ≥80%-type threshold + v0.1/v0.2 guess logic has no precedent.
4. **Trust-tier derivation** (D-07 §5.3 pure function) — no existing trust/status vocabulary in the codebase.
5. **Synthesized in-memory `index.md` inside an export** (never written to disk) — no precedent for virtual archive entries.

---

*Phase: 18-okf-bundle-support · Mapper: pattern pass complete · No source files modified.*
