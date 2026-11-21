# Phase 16: rest-api — Pattern Map

**Anchors verified against:** working tree 2026-10-31 (server.rs 9475 lines, webdav.rs 825, store.rs 3426, mcp.rs 647).
Every excerpt below is **verbatim** from the cited file:line. Classify: 🆕 new file · ✏️ modify existing.
Phase 15's pattern map (`../MORI-15-.../15-PATTERNS.md`) covers webdav.rs/store.rs ETag plumbing (§1–§4, §6, §11 there) — this map does not repeat it; cross-references say "see 15-PATTERNS §n".

---

## 1. `src/api.rs` 🆕 — module + router branch shape

**Role:** new module exposing `pub fn router(...)` merged into the main router; four GET endpoints + index + docs page.
**Closest analog:** the `/mcp` branch construction — `src/server.rs:265-274`.

```rust
    // MCP (Phase 10): Bearer API token, stateless StreamableHttp (rmcp).
    let mcp = Router::new()
        .route("/mcp", any(mcp_entry))
        .with_state(state.store.clone());

    public
        .merge(protected)
        .merge(admin)
        .merge(webdav)
        .merge(mcp)
```

**Extension note:** add `pub mod api;` to `src/lib.rs` (module list is alphabetical-ish, `src/lib.rs:3-14` — insert `pub mod api;` before `pub mod browse;`). In `build_router_with_email` (`src/server.rs:162`) build `let api = crate::api::router(state.clone());` (or `Router::new().route(...).route_layer(middleware::from_fn_with_state(state.clone(), require_api_bearer))`) and append `.merge(api)` to the merge chain at `:270-274`. Route syntax is axum 0.8 / matchit 0.8: `{slug}` params and `{*rest}` wildcards — precedent: `src/server.rs:223-224`:

```rust
        .route("/vault/{slug}/", get(vault_index))
        .route("/vault/{slug}/{*rest}", get(vault_file))
```

with tuple extraction `Path((slug, rest)): Path<(String, String)>` (`src/server.rs:635`). The `{path}` nested-slash parameter (D-04 discretion) uses the same `{*path}` wildcard style: `/api/v1/vaults/{slug}/notes/{*path}`. **Critical (D-01):** the api router must NOT be inside the `protected` group — `require_session` (`route_layer` at `src/server.rs:225-228`) would mix cookies with Bearer (Avoids B4). It is a standalone merged group with its own Bearer `route_layer`.

---

## 2. `src/api.rs` — Bearer auth gate ✏️-shape (new fn)

**Role:** reject missing/invalid tokens with 401 (problem+json per D-03), never accept session cookies; hand the resolved `User` to handlers.
**Closest analog (auth lookup + 401):** `mcp_entry` — `src/server.rs:281-301`.

```rust
async fn mcp_entry(State(store): State<Store>, req: Request) -> Response {
    let bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());
    let user = match bearer {
        Some(token) => store.user_for_api_token(&token).await.ok().flatten(),
        None => None,
    };
    let Some(user) = user else {
        return (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
            axum::Json(serde_json::json!({
                "error": "a valid Bearer API token is required (create one on /account)"
            })),
        )
            .into_response();
    };
```

**Closest analog (middleware that injects identity):** `require_dav_auth` — `src/server.rs:323-362`, key part:

```rust
    let Some((user, pass)) = creds else {
        return unauthorized();
    };
    match state.store.verify_login(&user, &pass).await {
        Ok(Some(user)) => {
            // Same pattern as require_session: hand the verified identity to
            // the handler as an extension.
            req.extensions_mut().insert(Claims {
                email: user.email,
                expires_at: 0,
            });
            next.run(req).await
        }
```

**Extension note:** combine both: an `async fn require_api_bearer(State(state): State<SharedState>, mut req: Request, next: Next) -> Response` applied via `.route_layer(middleware::from_fn_with_state(state.clone(), require_api_bearer))` (idiom at `src/server.rs:225-228, 259-262`). Copy the bearer-strip chain verbatim (`:282-291`), insert the resolved `User` (or email) as an extension (`:350-353` style). **Differences from /mcp:** the 401 body must be RFC 9457 `application/problem+json` (D-03), not the `Json({"error":...})` shape above — keep `WWW-Authenticate: Bearer` (`:295`). The rate limiter (D-13) also slots in this middleware or a second layer: over-limit → 429 + `Retry-After` (no `Retry-After` usage exists anywhere in `src/` — grep-verified). Cookies are structurally ignored because only `AUTHORIZATION` is read — that satisfies D-01's "session cookies rejected 401" only in the sense that a cookie without a Bearer yields 401; a request carrying BOTH must succeed on the Bearer (cookie is simply not consulted).

---

## 3. `src/api.rs` — shared read helpers extracted from mcp.rs (D-04)

**Role:** free functions both MCP tools and REST handlers call, so ACL is single-sourced. The bodies already exist verbatim in `src/mcp.rs`; extraction is mechanical.

**`source_dir` (ACL gate, 404-not-403 source)** — `src/mcp.rs:156-168`:

```rust
    /// The source directory of an accessible vault (same resolution as
    /// WebDAV: the markdown source, never the generated tree). One generic
    /// error for unknown slug and no-access — no existence leak.
    async fn source_dir(&self, slug: &str) -> Option<PathBuf> {
        let entry = self
            .store
            .accessible_vaults(&self.email)
            .await
            .ok()?
            .into_iter()
            .find(|a| a.vault.slug == slug)?;
        crate::webdav::source_dir_of(Path::new(&entry.vault.book_path))
    }
```

→ becomes `pub(crate) async fn source_dir(store: &Store, email: &str, slug: &str) -> Option<PathBuf>`.

**`list_vaults` body** — `src/mcp.rs:184-204` (the JSON field set D-08 locks):

```rust
        let vaults: Vec<serde_json::Value> = self
            .store
            .accessible_vaults(&self.email)
            .await
            .unwrap_or_default()
            .iter()
            .map(|a| {
                serde_json::json!({
                    "slug": a.vault.slug,
                    "name": a.vault.name,
                    "scope": a.vault.scope,
                    "role": a.role,
                })
            })
            .collect();
```

**`list_notes` body** — `src/mcp.rs:212-229` (safe_join sub-folder gate + `list_md_files`):

```rust
        let Some(src) = self.source_dir(&p.slug).await else {
            return tool_error("vault not found or access denied");
        };
        let base = match p.path.as_deref() {
            Some(sub) => match safe_join(&src, sub) {
                Some(b) => b,
                None => return tool_error("invalid path"),
            },
            None => src.clone(),
        };
        if !base.is_dir() {
            return tool_error("no such folder");
        }
        let files = list_md_files(&base);
```

**`read_note` body** — `src/mcp.rs:238-254`:

```rust
        let full = match safe_join(&src, &p.path) {
            Some(f) => f,
            None => return tool_error("invalid path"),
        };
        if !full.is_file() {
            return tool_error("no such note");
        }
        match std::fs::read_to_string(&full) {
```

**`search_notes` body** — `src/mcp.rs:263-281` (slug-scoped vs global vault list, `search_md(&vaults, &p.query, 50)` cap = D-10).

**Already-`pub(crate)` free helpers (no move needed):** `safe_join` `src/mcp.rs:20-34`, `list_md_files` `src/mcp.rs:39-65` (returns sorted `Vec<String>` — the cursor in D-09 sorts over exactly this output), `search_md` `src/mcp.rs:78-103`, and the hit DTO `Hit` `src/mcp.rs:68-74`:

```rust
#[derive(Debug, serde::Serialize, PartialEq)]
pub struct Hit {
    pub slug: String,
    pub path: String,
    pub line: usize,
    pub snippet: String,
}
```

`Hit` already derives `Serialize` — REST search can serve it directly (D-08 field-name parity).

**Store signatures the helpers depend on (verbatim):**
- `src/store.rs:730` — `pub async fn accessible_vaults(&self, email: &str) -> anyhow::Result<Vec<AccessibleVault>>`
- `src/store.rs:1595` — `pub async fn user_for_api_token(&self, plain: &str) -> anyhow::Result<Option<User>>`
- `AccessibleVault` fields — `src/store.rs:66-74`: `pub vault: Vault, pub role: String, pub personal: bool, pub direct_share: bool`; `Vault` — `src/store.rs:15-30`: `id, slug, name, book_path, scope, owner_email: Option<String>, keep_page_numbers, theme`.
- `src/store.rs:1372-1379` — `pub async fn log_access(&self, email: &str, slug: &str, source: &str, path: &str) -> anyhow::Result<()>`. **⚠ CHECK constraint:** `migrations/0008_*.sql:27` — `source TEXT NOT NULL CHECK (source IN ('web', 'mcp'))`. Logging REST hits as `source='api'` requires a new migration (ALTER is not possible on SQLite CHECK — needs table rebuild) — planner decision: reuse `'mcp'`, reuse `'web'`, or skip REST access logging (CONTEXT leaves it to planner discretion).

---

## 4. `src/api.rs` — query DTOs and endpoint handlers

**Role:** `?limit=` / `?cursor=` / `?q=` extraction.
**Closest analog:** `BrowseQuery` — `src/server.rs:1820-1822` + use at `:1780`:

```rust
struct BrowseQuery {
    path: Option<String>,
}
```
```rust
async fn admin_browse(State(state): State<SharedState>, Query(q): Query<BrowseQuery>) -> Response {
```

`Query` is already imported in server.rs (`src/server.rs:8`: `use axum::extract::{Extension, Form, Path, Query, Request, State};`). A `ListQuery { limit: Option<u32>, cursor: Option<String> }` with `#[serde(default)]` semantics via `Option` mirrors this exactly. Default-50/cap-200 clamping is plain code (D-09).

**Response shape precedent (bare JSON, no envelope):** none in server.rs (it is all HTML) — the MCP tool bodies (§3) ARE the shape reference: bare arrays of objects with the MCP field names (D-08). Serve via `axum::Json` (already imported `src/server.rs:14`).

---

## 5. `src/api.rs` — note read: raw markdown + ETag + If-None-Match → 304 (D-06/D-11/D-12)

**Closest analog:** the WebDAV GET branch (Phase 15) — `src/server.rs:887-911`:

```rust
        Method::GET => {
            let full = match webdav::safe_join(&src_dir, &rel) {
                Some(p) => p,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };
            match tokio::fs::read(&full).await {
                Ok(bytes) => {
                    // CONF-01: strong ETag over the raw source bytes, emitted
                    // only after the can_read authz gate above.
                    let etag = webdav::etag_of(&bytes);
                    (
                        [
                            (
                                axum::http::header::CONTENT_TYPE,
                                content_type(&rel).to_string(),
                            ),
                            (axum::http::header::ETAG, etag),
                        ],
                        bytes,
                    )
                        .into_response()
                }
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
```

**Precondition parser (reuse verbatim, do not re-implement):** `webdav::if_none_match_of` — `src/webdav.rs:458-460` returning `Precondition` — `src/webdav.rs:406-416`:

```rust
pub enum Precondition {
    /// Header absent — unconditional request (CONF-05 backward compat).
    Absent,
    /// `*` — the wildcard precondition.
    Any,
    /// One or more quoted strong tags. Quotes are KEPT so tags compare
    /// directly against the quoted [`etag_of`] wire form.
    Tags(Vec<String>),
    /// Unreadable (invalid UTF-8) or invalid syntax — caller returns 400.
    Malformed,
}
```

Fail-closed 400 precedent: `src/server.rs:929-934` (`Malformed` → `StatusCode::BAD_REQUEST` before any work). For REST GET: `Tags` containing the computed `etag_of(&bytes)` → `StatusCode::NOT_MODIFIED` (compare quoted forms directly — 15-01-SUMMARY rule, never re-hash-and-quote); `Malformed` → 400 problem+json (PITFALLS C2). `etag_of` itself: `src/webdav.rs:31-36` (see 15-PATTERNS §11.1). Content-Type is `text/markdown` (D-11), not `content_type(&rel)` sniffing.

---

## 6. RFC 9457 `application/problem+json` 🆕 — NO ANALOG

Grep-verified: **zero** occurrences of `problem+json`, `problem_json`, or any Problem struct in `src/` (only false positives: "3 Body Problem" in `src/obsidian/tree.rs:42,504`). Nothing in the repo sets a custom error content type today — errors are bare status codes (`StatusCode::NOT_FOUND.into_response()`), `error_page()` HTML, or the `/mcp` `Json({"error":...})` (`src/server.rs:296-298`).

New primitive needed: a small `Problem` DTO + helper in `src/api.rs`, e.g. `fn problem(status: StatusCode, ty: &str, title: &str, detail: &str) -> Response` returning `([(CONTENT_TYPE, "application/problem+json")], Json(json!({"type": ..., "title": ..., "status": ..., "detail": ..., "code": ...})))`. serde/serde_json are deps (`Cargo.toml:20,36`); no crate needed. The `type`/`code` string vocabulary is CONTEXT "Claude's Discretion". Every error path (401, 404 invisible-vault D-02, 400 malformed cursor/precondition, 429, 5xx) must route through it — including the auth middleware's 401 (§2), overriding the `/mcp` JSON shape.

---

## 7. Rate limiting (D-13) 🆕 — NO ANALOG; AppState is the DI point

Grep-verified: **no** limiter/throttle/429/`Retry-After` anywhere in `src/` or `Cargo.toml` (tower-http is present but only with feature `["fs"]` — `Cargo.toml:26` — no `limit` feature; D-05 forbids adding one anyway).

**Closest structural analog (in-process keyed state on AppState):** `AppState.vault_locks` — `src/server.rs:36-43` + get-or-insert helper `vault_lock` — `src/server.rs:47-56`:

```rust
    pub vault_locks:
        std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}
```
```rust
fn vault_lock(state: &SharedState, src_dir: &std::path::Path) -> Arc<tokio::sync::Mutex<()>> {
    let key = src_dir.display().to_string();
    let mut map = state
        .vault_locks
        .lock()
        .expect("vault_locks map mutex poisoned");
    map.entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}
```

**Extension note:** `AppState` (`src/server.rs:30-43`, fields `cfg/codec/store/email/vault_locks`) gains e.g. `pub api_rate: std::sync::Mutex<HashMap<String, (u32 /*count*/, u64 /*window_start*/)>>` — same "std Mutex guards only the map, never held across await" contract (comment at `:38-40`). Fixed window 120/min per token: increment + compare window against `unix_now()` (helper exists: `unix_now()` defined at `src/server.rs:126` (used at :372)). Over-limit → 429 problem+json + `Retry-After: <secs-to-window-end>`. **AppState construction sites that must gain the new field (all 4):** `src/server.rs:167-173` (`build_router_with_email`), `src/server.rs:3935-3941` (`test_router`), `src/server.rs:3966-3972` (`test_router_with_email`), `src/server.rs:6778-6784` (`browse_router_named` test helper). `SharedState = Arc<AppState>` at `src/server.rs:58` shares it with every handler.

---

## 8. Opaque base64 cursor pagination (D-09) 🆕 — NO ANALOG (but base64 is already a dep)

Grep-verified: no cursor/pagination/`has_more`/`next_cursor` anywhere in `src/`. **D-05 satisfied without new deps:** `base64 = "0.22"` is already a dependency (`Cargo.toml:35`) with two in-repo usage precedents:
- `src/webdav.rs:16,715` — `use base64::Engine;` … `base64::engine::general_purpose::STANDARD`
- `src/session.rs:5-6` — `use base64::engine::general_purpose::URL_SAFE_NO_PAD;` (URL-safe, no padding — the right engine for a query-string cursor)

Cursor = `URL_SAFE_NO_PAD.encode(last_path)`; decode with `.decode()` → invalid → 400 problem+json (fail closed, same spirit as `Precondition::Malformed`). Algorithm over the sorted `list_md_files` output (§3): decode cursor → `skip_while(|p| p <= cursor)` (or `binary_search` offset) → take `limit+1` to compute `has_more` → response `{ "notes": [...], "has_more": bool, "next_cursor": Option<String> }` (exact key names planner's discretion; D-09 locks `has_more` + `next_cursor`).

---

## 9. `src/mcp.rs` ✏️ — thin-wrapper refactor (D-04)

**Role:** the four `#[tool]` methods keep their signatures and `CallToolResult` wrapping but delegate bodies to the §3 helpers. Current structure to preserve: `MoriMcp { tool_router, store, email }` (`src/mcp.rs:133-138`), `tool_error` (`:140-144`), `log_access` best-effort spawn (`src/mcp.rs:170-180`):

```rust
    fn log_access(&self, slug: &str, path: &str) {
        let store = self.store.clone();
        let email = self.email.clone();
        let slug = slug.to_string();
        let path = path.to_string();
        tokio::spawn(async move {
            let _ = store.log_access(&email, &slug, "mcp", &path).await;
        });
    }
```

**Constraint:** the error STRINGS are asserted verbatim by tests (`"vault not found or access denied"` — `src/mcp.rs:601,626`), so the shared helper must return a typed reason the tool layer maps back to the same strings, while the REST layer maps the same reason to problem+json 404 (D-02). Suggested shape: helper returns `Result<T, ApiError>` where `ApiError::{NotFound, InvalidPath, NoSuchFolder, NoSuchNote, Unreadable}`; `tool_error(...)` strings and problem+json both derive from it.

## 10. `tools_are_acl_scoped` test matrix — exact inventory (D-07 port target)

All in `src/mcp.rs` `mod tests` (`:294-647`). Fixtures: `store()` = `Store::connect_memory()` (`:298-300`); `tmpdir(tag)` (`:302-307`); `share_fixture(tag, slug, role)` (`:527-546`) creates owner+sharee users, seeds vault with `note.md` containing `"# Shared\nsecret needle content"`, `share_vault(slug, sharee, role)`, returns `(Store, MoriMcp)` for the sharee.

| Test | mcp.rs:line | Asserts | REST port (4 endpoints) |
|---|---|---|---|
| `tools_are_acl_scoped` | :349-381 | reader w/ no grants: `list_vaults` → `[]`; guessed slug `"secret"` → tool error (no existence leak) | GET /vaults → `[]`; other 3 endpoints on unknown slug → 404 problem+json |
| `read_note_refuses_traversal_even_with_access` | :384-413 | admin reads real note OK; `path: "../secret.md"` → error | `/notes/../secret.md` (encoded `%2e%2e`) → 404/400, never reads outside source |
| `mcp_sharee_sees_shared_vault` | :549-581 | loop `["reader","writer"]`: list shows `"sh1"` with role `read`/`write`; read contains content; list_notes OK | same on REST with Bearer token of sharee |
| `mcp_hidden_share_invisible` | :584-602 | after `hide_share`: absent from list; read → error text exactly `"vault not found or access denied"` | 404 problem+json, no existence leak |
| `mcp_revoked_share_denied` | :605-628 | access works, `revoke_share`, access gone immediately | same |
| `mcp_search_finds_notes_in_shared_vault` | :631-646 | global search hits `"sh4"` in shared vault | `GET /search?q=…` hits shared vault |
| `mcp_denied_call_logs_nothing` | :498-522 | denied call → zero `access_log` rows (sleep 150ms then query) | if REST logs access at all (see §3 CHECK warning) |
| `mcp_read_note_logs_access` / `mcp_list_and_search_log` | :446-495 | `poll_log` helper (`:425-443`) polls spawned rows | same, if logging |

**REST test harness analogs** (server.rs test module): `test_router()` → `(Router, SharedState)` `src/server.rs:3931-3946`; `req(app, method, uri, headers, body)` oneshot `:3980-3994`; token minting pattern — `verify_login(...).id` then `state.store.create_api_token(uid, "name")` → `(id, plain)` (`src/store.rs:1532-1552`, test usage `src/server.rs:7031-7043`); Bearer header insert pattern `src/server.rs:6997-7002`:

```rust
        if let Some(b) = bearer {
            headers.insert(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {b}").parse().unwrap(),
            );
        }
```

Existing 401/e2e test style to mirror: `mcp_endpoint_requires_a_bearer_token` (`:7018-7023`), `mcp_rejects_an_unknown_or_revoked_token` (`:7025-7044` — forged → 401, live → 200, revoked → 401). New REST tests go in `src/api.rs` `mod tests` (unit, mirroring mcp.rs style) and/or server.rs test module (router e2e via `req`). New per-endpoint tests must also cover: ETag/304 (assert `ETAG` header equals `webdav::etag_of(bytes)`, precedent `src/server.rs:6215,6234`), cursor paging `has_more`/`next_cursor` round-trip, limit default 50/cap 200, rate-limit 429 + `Retry-After` (loop 121 requests against `test_router`), problem+json content-type on every error.

## 11. `src/server.rs` ✏️ — docs page (D-15) + `app_page` visibility

**Closest analog:** `app_page` — `src/server.rs:85-101`:

```rust
/// Render an app page with the embedded stylesheet + standard header/nav.
/// Every handler-generated page MUST go through this — a raw `Html(body)`
/// ships unstyled (verified in the browser: serif default, no card, no nav).
fn app_page(title: &str, body: &str) -> Response {
```

**Flag:** `app_page` is currently **private** (`fn`, not `pub(crate)`). If the `/api/v1/docs` handler lives in `api.rs`, `app_page` must become `pub(crate)` (one-word change at `src/server.rs:85`) — or the docs handler stays in server.rs. Phase 9b rule: no ad-hoc CSS; static HTML in `<section class="panel">`-style markup, `html_escape` (`src/server.rs:1095`) any interpolated values (static content here, so mostly N/A). CONTEXT discretion: docs page may be public (place in the `public` router group `src/server.rs:176-194`, no state needed) or Bearer-gated (inside the api group).

## 12. Cargo / feature flags check

- **No new dependencies required** (D-05): axum 0.8 (`Json`, `Query`, `Path`, middleware — all core), serde + serde_json, base64 0.22, tokio 1.40 **with `sync` feature already present** (`Cargo.toml:22` — added by Phase 15), uuid v4, sha2/hex (used via `webdav::etag_of`).
- tower-http `limit` feature NOT needed (in-process limiter per D-13).
- `schemars` stays MCP-only; REST DTOs need plain `serde::Deserialize` only.
- **Possible migration 0012** ONLY if REST access logging with a new `source` value is chosen (§3 CHECK warning); otherwise zero schema change this phase.

---

## Summary table

| # | Target | Class | Analog |
|---|--------|-------|--------|
| 1 | `src/api.rs` module + router branch + merge | 🆕 new | mcp branch `server.rs:265-274`, `{*rest}` routes `:223-224` |
| 2 | `src/api.rs` Bearer middleware | 🆕 (new fn) | `mcp_entry:281-301` + `require_dav_auth:323-362` |
| 3 | `src/api.rs` shared read helpers | ✏️ extract | `mcp.rs:159-288` bodies verbatim |
| 4 | `src/api.rs` query DTOs / handlers | 🆕 | `BrowseQuery:1820`, `Query(q)` `:1780` |
| 5 | `src/api.rs` note read ETag/304 | 🆕 | WebDAV GET `server.rs:887-911`, `Precondition` `webdav.rs:406-460` |
| 6 | RFC 9457 problem+json | 🆕 **no analog** | — (serde_json only) |
| 7 | Rate limiter + AppState field | 🆕 **no analog** | `vault_locks` map pattern `server.rs:36-56` |
| 8 | base64 cursor pagination | 🆕 **no analog** | base64 usage `session.rs:5-6` (URL_SAFE_NO_PAD), `webdav.rs:715` |
| 9 | `src/mcp.rs` thin wrappers | ✏️ modify | self `mcp.rs:184-288` |
| 10 | ACL test matrix port | ✏️ modify (tests) | `mcp.rs:349-646` + `test_router:3931`, `req:3980`, Bearer tests `:7018-7094` |
| 11 | `src/server.rs` docs page + `app_page` pub(crate) | ✏️ modify | `app_page:85-101` |
| 12 | Cargo.toml | — no change | all deps present |

## No-analog items (grep-verified)

1. **RFC 9457 problem+json** — nothing in `src/` emits `application/problem+json`; new `Problem` DTO + helper in `api.rs`.
2. **Rate limiting** — no limiter/throttle/429/Retry-After anywhere; new fixed-window map on `AppState` (pattern precedent: `vault_locks` std-Mutex-keyed-map).
3. **Cursor pagination** — no cursor/has_more/next_cursor anywhere; `base64 = "0.22"` already a dep (`URL_SAFE_NO_PAD` precedent in `session.rs`).
4. **`access_log` source='api'** — CHECK constraint `('web','mcp')` in migration 0008 blocks a new source value without a table-rebuild migration; planner must choose reuse/skip/migrate.

## Open flags for the planner

- `app_page` is private — needs `pub(crate)` if the docs handler lives in `api.rs` (§11).
- Four `AppState` construction sites must gain the rate-limiter field (§7: :167, :3935, :3966, :6778).
- MCP error strings are test-asserted verbatim (`mcp.rs:601,626`) — the extracted helper's error enum must map back to them (§9).
- `/api/v1` must be a standalone merged router group, never under `require_session` (D-01 / Avoids B4).
