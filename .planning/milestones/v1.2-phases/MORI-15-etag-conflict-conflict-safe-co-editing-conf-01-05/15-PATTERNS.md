# Phase 15: etag-conflict — Pattern Map

**Anchors verified against:** working tree 2026-10-30 (server.rs 8033 lines, webdav.rs 590, store.rs 3327).
Every excerpt below is **verbatim** from the cited file:line. Classify: 🆕 new file · ✏️ modify existing.

---

## 1. `src/webdav.rs` — PROPFIND `getetag` ✏️

**Role:** `prop_entry` / `multistatus` / `file_propfind_response` gain a `<D:getetag>` property (CONF-01).
**Closest analog:** `prop_entry` itself — `src/webdav.rs:136-169`.

```rust
/// One entry in a PROPFIND multistatus response.
fn prop_entry(href: String, is_dir: bool, size: u64, modified: SystemTime) -> String {
    let displayname = href
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let resourcetype = if is_dir { "<D:collection/>" } else { "" };
    format!(
        r#"<D:response>
  <D:href>{href}</D:href>
  <D:propstat>
    <D:prop>
      <D:resourcetype>{rt}</D:resourcetype>
      <D:displayname>{displayname}</D:displayname>
      <D:getcontentlength>{size}</D:getcontentlength>
      <D:getlastmodified>{lm}</D:getlastmodified>
      <D:getcontenttype>{ct}</D:getcontenttype>
    </D:prop>
    <D:status>HTTP/1.1 200 OK</D:status>
  </D:propstat>
</D:response>"#,
```
(`src/webdav.rs:135-157`)

**Extension note:** Phase 15 threads an `etag: Option<String>` (or `&str`) through `prop_entry`, `Entry` (`:172-177`), `multistatus` (`:181-196`), `list_dir` (`:199-243`) and `file_propfind_response` (`:255-263`), emitting `<D:getetag>"…"</D:getetag>` inside `<D:prop>` for files only (dirs stay without one). The hash-on-read happens at the caller (`webdav_entry`), since `webdav.rs` has no store/IO access to the vault beyond `tokio::fs` — `list_dir` already does per-child `item.metadata()` (`:221`), the same loop gains a `tokio::fs::read` + hash per file. The `multistatus_marks_dirs_and_sorts_them_first` test (`:389-417`) shows the assertion style for the new property.

---

## 2. `src/webdav.rs` — header-parsing helper style ✏️

**Role:** home for the new `If-Match` / `If-None-Match` parsers.
**Closest analog:** `depth_of` — `src/webdav.rs:309-319` (the existing "read one header, fail safe" helper).

```rust
/// Read the `Depth` header (0 or 1; anything else is treated as 0 per RFC).
pub fn depth_of(headers: &HeaderMap) -> u8 {
    match headers
        .get("depth")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim())
    {
        Some("1") => 1,
        _ => 0,
    }
}
```
(`src/webdav.rs:309-319`)

**Extension note:** the new parsers follow this shape — `headers.get("if-match").and_then(|v| v.to_str().ok())` returning a tri-state (absent / `*` / list of quoted tags) plus a malformed signal that the caller turns into 400 (pitfall C2: fail closed, never silently write). Note the deliberate difference: `depth_of` defaults on garbage, but preconditions must NOT — malformed → 400, so the return type needs an error variant. Custom-header access via string keys (`headers.get("depth")`) is the established idiom; `header::ETAG`/`header::IF_MATCH` exist in the http crate if preferred. Unit tests for the parser belong in `mod tests` in the same file (style: `webdav.rs:320-471`, plain `#[test]`, table-driven loops like `encoded_traversal_is_revealed_then_refused` `:555-570`).

---

## 3. `src/server.rs` — `webdav_entry` GET (ETag emission) ✏️

**Role:** GET must return `ETag: "<sha256-hex>"` on the raw source bytes (CONF-01).
**Closest analog:** the GET branch itself — `src/server.rs:862-875`.

```rust
        Method::GET => {
            let full = match webdav::safe_join(&src_dir, &rel) {
                Some(p) => p,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };
            match tokio::fs::read(&full).await {
                Ok(bytes) => (
                    [(axum::http::header::CONTENT_TYPE, content_type(&rel))],
                    bytes,
                )
                    .into_response(),
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
```
(`src/server.rs:862-875`)

**Extension note:** the bytes are already in hand (`tokio::fs::read` at `:867`) — Phase 15 computes the shared `etag_of(&bytes)` hash and adds `header::ETAG` to the header array. The authz gate (`access_for` can_read, `:821-828`) already runs **before** this branch, satisfying the authz-before-precondition rule for free. The PROPFIND branch (`:837-859`) is the sibling to extend with `getetag` (single-file path at `:846-852` already has `meta`; dir path goes through `webdav::list_dir`).

---

## 4. `src/server.rs` — `webdav_entry` PUT (preconditions + write + audit) ✏️

**Role:** If-Match/If-None-Match enforcement, 412/400, new ETag on success, shared atomic write helper, vault_changes with actor (CONF-02/03/05).
**Closest analog:** the PUT branch — `src/server.rs:876-927`.

```rust
        Method::PUT => {
            // Writing needs the write role: admins, the owner of a personal
            // vault, or a group granted write.
            if !access.can_write() {
                return StatusCode::FORBIDDEN.into_response();
            }
            if rel.is_empty() {
                return StatusCode::METHOD_NOT_ALLOWED.into_response();
            }
            let full = match webdav::safe_join(&src_dir, &rel) {
                Some(p) => p,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };
            let bytes = match axum::body::to_bytes(body, 8 * 1024 * 1024).await {
                Ok(b) => b,
                Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
            };
            // [excerpt elides the create_dir_all(parent) guard at :893-897]
            // Review LOW-02: on a metadata error assume the file existed
            // ("modified") rather than silently mislabeling as "added".
            let existed = tokio::fs::try_exists(&full).await.unwrap_or(true);
            if tokio::fs::write(&full, &bytes).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
```
(`src/server.rs:876-903`)

And the fire-and-forget audit spawn immediately after — the pattern Phase 15 extends with `actor`/`forced` (D-06):

```rust
            {
                let store = state.store.clone();
                let slug_c = slug.clone();
                let rel_c = rel.clone();
                tokio::spawn(async move {
                    let event_id = uuid::Uuid::new_v4().to_string();
                    let kind = if existed { "modified" } else { "added" };
                    if let Err(e) = store
                        .log_vault_change(&slug_c, &event_id, kind, &rel_c)
                        .await
                    {
                        eprintln!("vault-changes: webdav PUT log failed for {slug_c}: {e}");
                    }
                });
            }
            // Rebuild so the served HTML reflects the new note (AD-10).
            // llm-wiki sources are re-converted first (AD-14).
            rebuild_book(&book_dir, &src_dir, &vault_name);
            StatusCode::CREATED.into_response()
```
(`src/server.rs:908-926`)

**Extension note:** precondition evaluation slots **after** `access.can_write()` (`:879`) and `safe_join` (`:885`) but **before** the body read / any write — read current file, compare hash, return 412 (body naming the current ETag, per pitfall C2) or 400 (malformed) with nothing touched. The `tokio::fs::write` at `:901` is replaced by the shared write helper (see §11) under the per-vault lock. `rebuild_book` (`:925`, defined `:936`) stays where it is — 412 paths return before it, satisfying "rebuild only on success". The claims email (`:802-809`) is the actor source for D-06. Successful responses gain the new ETag header (200 vs 201 wording is the planner's discretion per CONTEXT "Claude's Discretion").

---

## 5. `src/server.rs` — `upload_local_vault` (conflict gate + update path) ✏️

**Role:** gate order to preserve; slug pre-check `:3001-3005` becomes the update path (D-01); conflict gate inserts between gates and write loop; `force=1` text field (D-03).
**Closest analog:** the gate sequence — `src/server.rs:2947-3025`.

```rust
    // Gate order mirrors load_personal_vault (D-05): locked refusal FIRST.
    if is_locked(&state, &claims.email).await {
        return error_page("demo account can't add or suppress vaults");
    }
```
(`src/server.rs:2947-2950`)

```rust
    // Slug-collision pre-check BEFORE any disk touch (T-14-07): an uploaded
    // vault's served book lives INSIDE uploads/<slug>, so a registered vault
    // must be refused here (sequential UX). Dir existence is deliberately NOT
    // checked — the exclusive create_dir below owns that race instead.
    if state.store.vault_name(&slug).await.ok().flatten().is_some() {
        return error_page(&format!(
            "the slug '{slug}' is already taken — rename the vault"
        ));
    }
```
(`src/server.rs:2997-3005`)

The exclusive-create branch (`:3012-3025`, `create_dir` + `AlreadyExists` orphan-recovery retry) is kept verbatim for the **new-vault** path; the update path bypasses it.

The per-file streaming write loop the conflict gate must hash-compare against (`:3034-3090`), key excerpt:

```rust
        let target = dir.join(&rel);
        let parent = target.parent().unwrap_or(dir.as_path());
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return fail_upload(&dir, &format!("could not write the upload: {e}")).await;
        }
        // Defense-in-depth (D-03): the joined parent must resolve inside the
        // canonicalized uploads root before ANY byte is written.
        match parent.canonicalize() {
            Ok(p) if p.starts_with(&root) => {}
            _ => return fail_upload(&dir, "invalid file path in upload").await,
        }
```
(`src/server.rs:3055-3065`)

**Extension note:** when the slug pre-check hits an **uploaded, owned** vault (D-04: `vault_source = None` + `owner_email == claims.email`; ownership fields available via `store` — see `Vault.owner_email`, `store.rs:23`), the handler branches to the update path instead of the "already taken" `error_page`: skip quota + exclusive create, stream parts to a staging area (or hash-then-write via the §11 helper), compare incoming sha256 vs current `uploads/<slug>` file per D-02, and on conflict (no `force=1`) return the JSON conflict response the two-phase JS consumes. `fail_upload` (`:2922-2925`, `remove_dir_all` + `error_page`) is the cleanup idiom — the update path must NOT reuse its tree-wipe blindly on an existing vault (the tree is live). The multipart text-field read style for `force` mirrors the `name` field loop (`:2953-2969`); note fields may arrive after files in a `fetch`-built FormData, so the loop that `continue`s on non-file parts (`:3042`) is where `force` gets picked up.

---

## 6. `src/store.rs` — `vault_changes` methods ✏️

**Role:** `log_vault_change` gains `actor`/`forced` binds; new query for per-path change history (conflict page D-07).
**Closest analog:** `log_vault_change` — `src/store.rs:1435-1456`.

```rust
    /// Record one vault file change. Callers treat this as best-effort:
    /// errors must be logged and swallowed, never propagated into a response
    /// (D-04). `kind` is 'added', 'modified' or 'removed' (CHECK-enforced by
    /// migration 0010, D-08); all rows of one refresh share one `event_id`.
    pub async fn log_vault_change(
        &self,
        slug: &str,
        event_id: &str,
        kind: &str,
        path: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO vault_changes (vault_slug, event_id, kind, path) VALUES (?, ?, ?, ?)",
        )
        .bind(slug)
        .bind(event_id)
        .bind(kind)
        .bind(path)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
```
(`src/store.rs:1435-1456`)

Query style for the conflict page's per-path lookup — analog: `recent_vault_changes` (`:1462-1481`):

```rust
        Ok(sqlx::query_as(
            "SELECT event_id, ts, kind, path FROM vault_changes \
             WHERE vault_slug = ? AND event_id IN (\
                 SELECT event_id FROM vault_changes WHERE vault_slug = ? \
                 GROUP BY event_id ORDER BY MAX(ts) DESC, MAX(id) DESC LIMIT ?) \
             ORDER BY ts DESC, id DESC LIMIT ?",
        )
```
(`src/store.rs:1468-1473`)

**Extension note:** extend the INSERT column list with `actor, forced` (defaults exist in migration 0011 so existing callers could stay source-compatible, but D-06 wants actor populated on PUT/upload-update — pass it explicitly). Tuple-return `query_as` with `Vec<(String, String, …)>` is the house style; a per-path history query is `WHERE vault_slug = ? AND path = ? ORDER BY ts DESC LIMIT ?`. `prune_vault_changes` (`:1486-1491`) is untouched.

---

## 7. `migrations/0011_vault_changes_forced_actor.sql` 🆕

**Role:** additive `ALTER`s on `vault_changes`.
**Closest analog:** `migrations/0009_vault_display_options.sql` (additive ALTER style) + `0010` header-comment style.

```sql
-- Phase 12 (REQ-VOPT-001, REQ-VOPT-002): per-vault display options.
--
--   keep_page_numbers  owner toggle: keep mdBook's chapter page numbers
--                      visible for this vault (0 = hidden, the global rule).
--   theme              mdBook theme name; '' means mdBook default (auto).
--                      Server-side whitelist: light|navy|rust|coal|ayu.

ALTER TABLE vaults ADD COLUMN keep_page_numbers INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vaults ADD COLUMN theme TEXT NOT NULL DEFAULT '';
```
(`migrations/0009_vault_display_options.sql:1-9`)

**Extension note:** same shape — a `-- Phase 15 (…)` comment block documenting each column, then exactly two statements: `ALTER TABLE vault_changes ADD COLUMN forced INTEGER NOT NULL DEFAULT 0;` and `ALTER TABLE vault_changes ADD COLUMN actor TEXT NOT NULL DEFAULT '';`. The 0010 CHECK constraint on `kind` is **not touched** (D-05). No new index unless the conflict-page query needs one (existing `idx_vault_changes_slug_ts` covers slug+ts).

---

## 8. Home page "Load a vault" panel + JS ✏️ (layout changes under D-08)

**Role:** the panel the two-phase fetch upload JS lives in. **D-08 supersedes this layout**: one "Load local" button beside "Load vault", hidden `webkitdirectory` input triggered by the button, name derived from the picked folder — no name input, no "Upload vault" button. Map the current state as the analog being replaced; do not keep the two-form shape.

**Closest analog (form):** current upload form — `src/server.rs:587-591`:

```html
<form method="post" action="/vaults/upload" enctype="multipart/form-data" class="stacked">
  <div class="field"><label for="up-name">Name</label><input id="up-name" type="text" name="name" required placeholder="e.g. My Wiki"></div>
  <div class="field field-wide"><label for="up-files">Folder</label><input id="up-files" type="file" name="files" webkitdirectory multiple required><span id="up-count" class="empty"></span></div>
  <button type="submit">Upload vault</button>
</form>
```
(`src/server.rs:587-591`, inside the panel at `:579-593`)

**Closest analog (JS const style):** `LOCAL_UPLOAD_COUNT_JS` — `src/server.rs:1978-1992`:

```rust
/// Plan 14-01 (D-06): live count of the files picked by the Local vault
/// directory picker, shown next to the input before submit. Raw-string const:
/// brace-bearing JS never goes inside a `format!` (pitfall #25).
const LOCAL_UPLOAD_COUNT_JS: &str = r#"<script>
// morinet-upload-count: show how many files the folder picker selected.
(function () {
  var input = document.querySelector('form[action="/vaults/upload"] input[name="files"]');
  var out = document.getElementById("up-count");
  if (!input || !out) return;
  input.addEventListener("change", function () {
    var n = input.files ? input.files.length : 0;
    out.textContent = n === 0 ? "" : n + " files selected";
  });
})();
</script>"#;
```

And the form-interception idiom (the closest thing to "intercept the upload submit") — `LOCKED_ACCOUNT_JS` — `src/server.rs:1994-2011`:

```rust
/// Phase 12 (D-04/D-05): rendered ONLY for a locked account — intercepts the
/// Load-vault submit and the Delete confirm with the required popup. UX only;
/// the server-side `is_locked` gates are the security boundary (T-12-01).
/// Raw-string const: brace-bearing JS never goes inside a `format!` (pitfall #25).
const LOCKED_ACCOUNT_JS: &str = r#"<script>
// morinet-locked-account: demo guard popup (UX only; the server refuses too).
(function () {
  var MSG = "demo account can't add or suppress vaults";
  function block(form) {
    // Strip the inline confirm FIRST: removeAttribute("onsubmit") unregisters
    // the handler slot, so assigning the property before stripping would be
    // wiped (checker fix W-4, order corrected — see 12-01-SUMMARY).
    form.removeAttribute("onsubmit");
    form.onsubmit = function (e) {
      e.preventDefault();
      alert(MSG);
      return false;
    };
  }
```

**Extension note:** the new JS is another `const …: &str = r#"<script>…</script>"#` raw-string const (pitfall #25 rule: brace-bearing JS never inside `format!`), interpolated into the home-page `format!` alongside `upload = LOCAL_UPLOAD_COUNT_JS` / `locked = locked_script` (`:592-601`). It wires: "Load local" button → hidden input `.click()` → `change` handler keeps the `FileList` in memory → builds `FormData` (name derived client-side from `files[0].webkitRelativePath`'s first segment, mirroring the server's top-dir strip) → `fetch("/vaults/upload", {method:"POST", body})` → on the JSON conflict response renders the list + "Upload anyway" → re-sends the same `FileList` with `force=1`. ES5-style `var`/`function` and `DOMContentLoaded` guarding are the established JS dialect (`:1998-2027`). The two-phase fetch flow itself has **no analog** — existing JS is alert/count only.

---

## 9. Conflict page rendering ✏️ (new page, existing shell)

**Role:** D-07 rich conflict list inside `app_page()` shell, per-file links to vault_changes entries.
**Closest analog:** the Wiki-changes section grouping style — `src/server.rs:2585-2618`:

```rust
    // Group consecutive rows sharing event_id (query is already newest-first).
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for (event_id, ts, kind, path) in changes {
        // D-08: display prettification only; stored vocabulary stays exact.
        let mut label = kind;
        if let Some(first) = label.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let row = format!(
            r#"<tr><td>{k}</td><td>{p}</td></tr>"#,
            k = html_escape(&label),
            p = html_escape(&path)
        );
```
(`src/server.rs:2585-2597`; shell: `app_page` at `:65`, panel markup `<section class="panel">` at `:2620`)

**Extension note:** conflict rows are built the same way — `html_escape` every path (attacker-controlled, T-13-08 comment at `:2577-2579` applies verbatim), `<table>` markup inside `<section class="panel">`, no new CSS (Phase 9b rule). The "who changed it" link target is the existing vault stats page (`/vault/{slug}/stats` Wiki-changes section). If the conflict response is JSON for the fetch flow (D-03), this shell pattern applies to the non-JS fallback rendering only.

---

## 10. Test style ✏️

**Role:** inline `#[tokio::test]` tests for ETag/If-Match (server.rs) and migration shape (store.rs).
**Closest analogs:** `upload_registers_personal_vault` — `src/server.rs:7581-7635` (multipart e2e via `multipart_body` `:7507-7527`, `login_as`, `req`, `uploads_dir_for`); `vault_changes_migration_shape` — `src/store.rs:3205-3226`. Representative excerpt:

```rust
    #[tokio::test]
    async fn upload_rejects_traversal_path() {
        let (app, state) = test_router().await;
        let cookie = login_as(&app, "admin@example.com", "test-pass-9f3a").await;
        for bad in ["../../evil.md", "/etc/passwd.md"] {
            let body = multipart_body(
                "bnd",
                &[("name", None, b"Trav Test"), ("files", Some(bad), b"x")],
            );
            let res = req(
                app.clone(),
                "POST",
                "/vaults/upload",
                multipart_headers(&cookie, "bnd"),
                Some(Body::from(body)),
            )
            .await;
            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "{bad} must be refused"
            );
```
(`src/server.rs:7637-7658`)

**Extension note:** ETag tests are plain `#[tokio::test]` in the server test module: WebDAV requests go through the same `req(...)` helper with Basic-auth headers and an added `.header("if-match", …)` — assert `412`/`400`/`ETag` header presence and (for C2/C3) that the file bytes and `vault_changes` rows are unchanged after a 412 (the `assert!(state.store.vault_name(...).is_none())` post-condition style at `:7665-7667`). Migration tests copy `vault_changes_migration_shape`: raw `sqlx::query` INSERT + tuple `query_as` read-back, asserting `forced` defaults to 0 and `actor` to `''`. `multipart_body` already supports text fields (`("force", None, b"1")`).

---

## 11. No analog — genuinely new primitives 🆕

Grep-verified: **no** `tokio::sync::Mutex`/`RwLock` anywhere in `src/` (only `std::sync::Mutex` at `src/config.rs:223` for env-lock and `src/email.rs:131` for a test sender's `Vec`), **no** `tempfile` crate in `Cargo.toml`, **no** `fs::rename` usage in `src/`. The following have no in-repo pattern to copy; listed with the primitives the project already has available:

1. **`etag_of()` / ETag computation** — *closest partial analog:* `token_hash` (`src/server.rs:1349-1355`): `sha2::{Digest, Sha256}` + `hex::encode(h.finalize())`. Both crates are already deps (`Cargo.toml:30,32`). New part: hashing `&[u8]` file bytes instead of `&str`, and the quoted `"<hex>"` wire form. One helper feeds GET, PROPFIND, PUT check, and upload compare ("one hash, three consumers").
2. **If-Match / If-None-Match parsing** — no analog. Hand-rolled parser in `webdav.rs` (house style for hand-rolled header/URI parsing: `depth_of` `:310`, `parse_basic` `:476`, `decode_path` `:269` — all small pure `fn`s with table-driven `#[test]`s). Needs: absent vs `*` vs comma-list, quoted tags, strong-compare only (no `W/` acceptance), malformed → caller 400.
3. **Per-vault in-process write lock map** — no analog. `AppState` (`src/server.rs:30-36`) is the DI point (`pub cfg/codec/store/email` — a new field like `vault_locks` slots here; `SharedState = Arc<AppState>` `:38` already shares it with every handler). Primitives available: `tokio = "1.40"` is a dep (`Cargo.toml:22`) but its `sync` feature is **not** in the feature list (`["macros", "rt-multi-thread"]`) — the executor must add `sync` (or `full`) to `Cargo.toml`, or use `std::sync::Mutex` keyed map guarding only non-await sections. Keyed-map-of-Mutex pattern exists nowhere; `std::sync::Mutex` usage at `config.rs:223` (`static ENV_LOCK`) is the only Mutex precedent.
4. **Temp-file + atomic-rename write helper** (shared by WebDAV PUT and upload — roadmap criterion 7) — no analog. Current writes are direct `tokio::fs::write` (`server.rs:901`) and streamed `tokio::fs::File::create` (`:3066`). No `tempfile` crate; the stdlib path is `tokio::fs::File::create` a sibling `.<name>.tmp-<uuid>` (uuid v4 already a dep, `Cargo.toml:39`) + `tokio::io::AsyncWriteExt::write_all` (precedent: upload loop `:3078`) + `tokio::fs::rename` (same-filesystem rename; uploads and vault dirs live under `data_dir`). The helper also returns the hash of the bytes written (D-06 recompute-from-written-bytes per pitfall C3).
5. **Two-phase fetch upload JS** — no analog (see §8: existing JS is alert/count interception only).

---

## Summary table

| # | Target | Class | Analog |
|---|--------|-------|--------|
| 1 | `src/webdav.rs` prop_entry/multistatus/file_propfind_response | ✏️ modify | self (`webdav.rs:136/181/255`) |
| 2 | `src/webdav.rs` If-Match/If-None-Match parsers | ✏️ modify (new fns) | `depth_of` `webdav.rs:310` (shape only) |
| 3 | `src/server.rs` webdav_entry GET + PROPFIND | ✏️ modify | GET branch `server.rs:862` |
| 4 | `src/server.rs` webdav_entry PUT | ✏️ modify | PUT branch `server.rs:876-927` |
| 5 | `src/server.rs` upload_local_vault | ✏️ modify | gate order `server.rs:2947-3025` |
| 6 | `src/store.rs` log_vault_change + new query | ✏️ modify | `store.rs:1439/1462` |
| 7 | `migrations/0011_*.sql` | 🆕 new | `0009` ALTER style + `0010` header |
| 8 | Home page panel + upload JS (D-08 layout change) | ✏️ modify | forms `server.rs:587`, `LOCAL_UPLOAD_COUNT_JS:1981`, `LOCKED_ACCOUNT_JS:1998` |
| 9 | Conflict page rendering | ✏️ modify (new page in shell) | Wiki-changes `server.rs:2585-2618`, `app_page:65` |
| 10 | Tests (server.rs + store.rs inline) | ✏️ modify | `upload_registers_personal_vault:7581`, `vault_changes_migration_shape:3205` |
| 11 | etag_of / lock map / atomic-write helper / fetch JS | 🆕 no analog | see §11 |
