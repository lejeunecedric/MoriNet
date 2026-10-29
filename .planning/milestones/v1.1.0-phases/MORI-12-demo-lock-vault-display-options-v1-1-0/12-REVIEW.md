---
phase: MORI-12
status: issues_found
critical: 0
high: 0
medium: 1
low: 6
reviewed_commits: [f5711bd, eae5942, bdf1e7b, 036b327, 98c5c90, d46709d, 3649f16]
date: 2026-10-28
---

# Phase 12 Code Review — demo lock + vault display options

Scope: locked-account gate (12-01), vault display-options columns/edit UI (12-02),
serve-time rewrite (12-03). Cross-checked against the three plans' `threat_model`
blocks and `must_haves`. All 30 phase tests pass locally (`cargo test --lib -- locked
display_options serve_ edit_submit home_page`: 30 passed, 0 failed).

## Gate-completeness verdict (focus area 1)

Route table audited against guarded handlers. The five mutation routes named in
D-02 (`/vaults/create`, `/vaults/{slug}/delete`, `/vault/{slug}/edit` POST,
`/vault/{slug}/share` POST, `/vault/{slug}/share/revoke`) all carry the
`is_locked` guard before any store mutation; `load_personal_vault` checks before
quota/clone (T-12-05 holds). Remaining routes: refresh (open by design, D-02),
`share/hide` (toggles the caller's own hidden flag — not a vault add/suppress),
admin routes (behind `require_admin`; admins exempt by design, D-02), WebDAV
(PUT writes files inside a vault's source dir, never creates/deletes vault rows,
writer/admin-only), MCP (read-only tools: list/read/search). No unguarded vault
mutation route found. `is_locked` fails closed on `role_for` errors (Err → None
→ non-admin → still locked). Case-insensitivity holds on both sides (config
normalized at resolve, `eq_ignore_ascii_case` at compare). No TOCTOU: the guard
precedes the mutation within each handler.

## Rewrite-safety verdict (focus area 2)

The theme rewrite cannot smuggle content: the replacement is a fixed literal
containing only a whitelist-bounded token — no angle brackets, no attacker bytes
(T-12-10 holds for all current write paths). UTF-8 splicing is safe: every
insert/slice index comes from `rfind` of an ASCII needle, which is always a char
boundary. The page-numbers `<style>` is a static raw-string const (no
interpolation) and is never injected when `keep_page_numbers` is false (default),
proven by `serve_page_numbers_off_hides_by_default` and `display_options_off_is_inert`.
The override is inserted after the chrome/hide rule so same-specificity cascade
order wins without `!important`. Migration 0009 is two additive
`ADD COLUMN … NOT NULL DEFAULT` statements (valid for sqlx's multi-statement
migrator); every `Vault` row-mapping site was updated — Rust struct-literal
exhaustiveness makes a silent default impossible.

---

## Findings

### [MEDIUM] Empty `MORINET_LOCKED_ACCOUNTS` env value silently disables the file-section lock list

**File:** `src/config.rs:186-194` (`resolve`)

`std::env::var(ENV_LOCKED_ACCOUNTS).ok().map(normalize…)` treats an env var that
is *set but empty/whitespace-only* as "env wins": it normalizes to an empty list
and shadows `[security].locked_accounts` from the file. A blank `ENV:` entry in
docker-compose/k8s (a classic footgun) therefore fail-opens the demo lock —
exactly the "mistake must never silently un-lock an account" class T-12-03
promises to prevent, just via emptiness instead of case/whitespace.

**Suggested fix:** treat a normalized-empty env result as unset and fall through
to the file section:

```rust
let from_env = std::env::var(ENV_LOCKED_ACCOUNTS)
    .ok()
    .map(|raw| normalize_locked_list(raw.split(',')))
    .filter(|v| !v.is_empty());
let locked_accounts = from_env
    .or_else(|| { /* file section, non-empty */ })
    .unwrap_or_default();
```

### [LOW] Serve-time theme rewrite does not re-validate the whitelist (defense-in-depth)

**File:** `src/server.rs:627-647` (`apply_vault_display_options`)

The rewrite interpolates the stored `theme` into served JS with no whitelist
check; safety rests entirely on `vault_edit_submit` being the only writer
(`store.rs:1323` explicitly documents "the caller is responsible for the
whitelist"). Today that holds, but any future writer (admin tooling, a repair
script, manual DB edit) could put `"; evil()` in the column and get JS executed
on every visitor of the vault. One-line hardening removes the coupling:

**Suggested fix:** early-return from the theme branch unless
`["", "light", "navy", "rust", "coal", "ayu"].contains(&theme)` (mirror the
`THEMES` const at `server.rs:2159` into a shared const used by both sites).

### [LOW] Theme literal replace is document-global — rewrites occurrences inside code blocks

**File:** `src/server.rs:636-644`

`html.replace` rewrites **every** occurrence of the two literals, including ones
a vault page quotes inside `<code>`/`<pre>` (e.g., a book documenting mdBook's
theme config). The plan's T-12-11 claim ("visitor content that merely mentions
the literals is out of scope") only holds for *partial* mentions — a full literal
in a code sample is rewritten. No XSS (replacement carries no attacker bytes),
purely a content-integrity wart.

**Suggested fix:** scope the replace to the inline theme script region (e.g.,
locate the `<script>` block containing both literals and replace within it), or
document the limitation in the function doc-comment.

### [LOW] `rfind("</body>")` insertion point can land inside a script block; uppercase `</BODY>` missed

**File:** `src/server.rs:629-631` (and the chrome pass at `:665`)

The page-numbers override is inserted before the **last** `</body>` occurrence.
A vault page whose trailing script contains the string `"</body>"` (e.g., a JS
snippet or code sample) gets the `<style>` injected inside the `<script>`
element: inert as XSS (the injected text contains no `</script>`), but it breaks
that script with a syntax error. Conversely a page using `</BODY>` gets no
override at all (silent no-op). Both are self-inflicted edge cases for the vault
owner's own content, hence low.

**Suggested fix:** use the first case-insensitive `</body>` occurrence
(`html.rfind` → `to_ascii_lowercase` scan or `memchr`-style search), or accept
and note the mdBook-lowercase assumption in the doc-comment.

### [LOW] `locked_account_refresh_still_allowed` asserts only absence of the lock string

**File:** `src/server.rs:6374-6392`

The test passes if the route 404s, 403s, or the handler is removed entirely —
any non-lock failure body also lacks `LOCK_MSG`. It does not actually prove
refresh is *allowed* (must_have #3). Every other gate test correctly asserts
status + DB state (good overall test quality — DB-state assertions, reject-not-
coerce checks, byte-identity checks throughout).

**Suggested fix:** assert the positive signal — the handler's own "no upstream"
error text (the seeded vault has no upstream) or at minimum
`res.status() != StatusCode::BAD_REQUEST`.

### [LOW] Server accepts `theme` for native vaults whose Edit page hides the select

**File:** `src/server.rs:2159-2162` (validation) vs `:2103-2118` (D-11 render gate)

The theme select renders only for converted vaults (D-11), but `vault_edit_submit`
validates and persists `theme` for **any** vault — a scripted POST can store a
whitelisted theme on a native vault (and `edit_submit_persists_display_options`
enshrines this by persisting `theme=coal` on the native `mine` fixture). Inert
at serve time (native pages lack the literals), but the UI/server contract
mismatch invites confusion and dead DB state.

**Suggested fix:** in `vault_edit_submit`, ignore (or reject) a non-empty `theme`
when the vault is not converted — reuse the `.morinet-build` component check
from `vault_edit_page`.

### [LOW] Transient store error on the Edit page renders defaults that a save then persists

**File:** `src/server.rs:2097-2098`

`keep_page_numbers(&slug).await.unwrap_or(false)` and
`theme(&slug).await.unwrap_or_default()` swallow read errors: the form shows
"off / auto", and the owner's next Save writes those defaults over the real
stored values (silent option reset). Same pattern exists for `reshare`
(pre-existing), but the new fields inherit it.

**Suggested fix:** propagate the error to `error_page` instead of defaulting, or
at least log the swallowed error.

---

## Not findings (checked, clean)

- Migration 0009 shape + defaults verified by `display_options_migration_shape`.
- All `Vault` row-mapping sites updated (`list_vaults_for`, `list_all_vaults`,
  `accessible_vaults` admin/own/globals/shares); compiler-enforced completeness.
- `error_page` html-escapes the echoed theme in the reject message (T-12-06).
- Chrome early-return no longer gates the option pass — proven by
  `display_options_runs_on_chrome_early_return_page` and
  `serve_converted_vault_page_still_gets_options`.
- Non-UTF-8 payloads still pass through untouched (existing behavior preserved).
- Popup script ordering (`removeAttribute` before property assign) is correct.
