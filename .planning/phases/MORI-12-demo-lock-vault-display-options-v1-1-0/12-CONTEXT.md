# Phase 12: demo-lock + vault display options (v1.1.0) - Context

**Gathered:** 2026-10-28
**Status:** Ready for planning

<domain>
## Phase Boundary

The public demo cannot be defaced, and each vault owner controls how its pages
render (ROADMAP Phase 12, REQ-DEMO-001..002, REQ-VOPT-001..002):

1. **Locked accounts**: a configurable list (demo instance: `demo`); any
   attempt to load or delete a vault is refused server-side (clear error page)
   AND the home page shows a popup when Load vault / Delete is clicked.
2. **Per-vault display options** on the Edit page: "keep page numbers" toggle
   and mdBook theme select (light, navy, rust, coal, ayu).
3. Closes issue #7: the hide-numbers chrome CSS rule from `2f1bcf1` becomes
   the toggle's "off" state.

Out of scope: wiki-changes log (Phase 13), local vault upload (Phase 14),
admin-side vault management gates (admins keep full power), per-user theme
preferences (this is per-VAULT, owner-set).
</domain>

<decisions>
## Implementation Decisions

### Locked accounts (REQ-DEMO-001..002)
- Config: new `[security]` section in config.toml — `locked_accounts = ["demo"]`
  (`FileConfig` gets `security: SecuritySection` with `#[serde(default)]`),
  PLUS env override `MORINET_LOCKED_ACCOUNTS` (comma-separated emails) which
  wins when set. Resolved into `RuntimeConfig.locked_accounts: Vec<String>`.
  Comparison case-insensitive on email.
- Scope (user decision 2026-10-28, WIDER than ROADMAP): a locked account is
  refused on **load + delete + edit + share** mutations:
  `POST /vaults/create` (load_personal_vault), `POST /vaults/{slug}/delete`
  (delete_my_vault), `POST /vault/{slug}/edit` (vault_edit_submit),
  `POST /vault/{slug}/share*` (share add/revoke). Reads and Refresh stay
  allowed. Admins are never locked (the demo user is a `reader` anyway).
- Server refusal: the existing `error_page(...)` pattern with a clear message.
- Home popup (REQ-DEMO-002): when a locked account is logged in, the home page
  JS intercepts the Load-vault submit and the Delete confirm and shows
  `alert("demo account can't add or suppress vaults")` — exact string from
  REQUIREMENTS. The popup is UX only; the server gate is the security boundary
  (a scripted POST must still be refused).
- The home page must know "current user is locked" — render a flag server-side
  into the index page (no new endpoint needed); the JS reads it.

### Per-vault display options (REQ-VOPT-001..002)
- Storage: migration `migrations/0009_vault_display_options.sql` adds to
  `vaults`: `keep_page_numbers INTEGER NOT NULL DEFAULT 0` and
  `theme TEXT NOT NULL DEFAULT ''` ('' = mdBook default/auto behavior).
  `Vault` struct in store.rs gains both fields; store setters next to
  `set_allow_reshare`.
- Edit page (`vault_edit_page`/`vault_edit_submit`): add the toggle (checkbox,
  same present/absent semantics as `allow_reshare`) and a `<select name="theme">`
  with options auto (value ""), light, navy, rust, coal, ayu. Server validates
  the value against that whitelist — reject anything else.
- **No rebuild, ever** (user decision 2026-10-28): both options apply at SERVE
  time inside `inject_vault_chrome` (server.rs), which must become
  vault-aware (pass the two option values in from `serve_vault_file` — it
  already has the `vault` row).
- **Keep page numbers** (REQ-VOPT-001): default OFF = numbers hidden (the
  global rule in assets/zoom.css stays). When ON, inject a serve-time override
  `<style>.chapter-item a > strong[aria-hidden="true"]{display:inline;}</style>`
  before `</body>`. This works for converted AND native vaults (the rule
  targets mdBook's toc.js-injected `<strong aria-hidden="true">`).
- **Theme** (REQ-VOPT-002): mdBook 0.5 ships all five themes in
  `css/variables-*.css` and picks at runtime: visitor's
  `localStorage['mdbook-theme']` wins, else "auto" resolves via
  `default_light_theme = "light"` / `default_dark_theme = "navy"` — two
  literals present in EVERY generated page's inline theme script (verified on
  a real built book). Serve-time rewrite: when `theme` is set, replace those
  two assignments with the chosen theme — the owner's pick replaces BOTH the
  light and the dark auto default (fixes "auto goes to navy" on dark-system
  visitors). Visitor menu choice still wins (user decision: do NOT force).
  Do this rewrite for any vault whose page contains the literals (native
  books included — the rewrite is inert when the literals are absent).
  The ROADMAP's book.toml `default-theme` approach is SUPERSEDED by this
  serve-time rewrite: no rebuild, no Refresh needed, applies to already-built
  books immediately.
- Edit-page visibility: show display options only where they make sense —
  theme select for converted vaults (llm-wiki pipeline; native mdBook owners
  edit their own book.toml), page-numbers toggle for all personal vaults.
  (Claude's discretion, recorded here.)

### Issue #7 closure
- `zoom_css_hides_mdbook_chapter_numbers` (tree.rs) stays green — the global
  hide rule remains the default; the toggle only adds the serve-time override.
- Close GitHub issue #7 when 1.1.0 ships (post-deploy, not at plan time).
</decisions>

## Code Anchors (scouted 2026-10-28)

- `src/server.rs`: routes at ~183-188; `inject_vault_chrome` ~610 (called from
  `serve_vault_file` ~630-690, both HTML branches); `delete_my_vault` ~1908;
  `can_edit_vault` ~1929; `EditForm` ~1947; `vault_edit_page` ~1955;
  `vault_edit_submit` ~2000; `load_personal_vault` ~2413; share handlers
  ~2100-2160; home page vault rows markup ~475 (Delete form with inline
  `confirm(...)`), Load-vault form ~574.
- `src/config.rs`: `FileConfig` ~16, `RuntimeConfig` ~112, `resolve` precedence
  pattern already used for admin/smtp.
- `src/store.rs`: `Vault` struct ~15; migration runner + `migrations/` dir
  (next number: **0009** — 0008_shares_stats.sql is the last).
- `src/obsidian/tree.rs`: `ZOOM_CSS` (include_str! of assets/zoom.css), hide
  rule at assets/zoom.css:68; `book_toml()` ~231 (NOT touched by this phase).
- Verified on a real built book (`data/repos/b3wiki/.morinet-build/book/index.html`):
  `default_light_theme = "light";` and `default_dark_theme = "navy";` appear
  verbatim in the inline theme script; `<html class="light ...">`.

## Existing Patterns to Reuse

- Checkbox semantics: `EditForm.allow_reshare: Option<String>` (present = on).
- Refusal UX: `error_page(&str)`; owner gate: `can_edit_vault`.
- Config env-override precedence: admin email/password pattern in `resolve`.
- Tests: route e2e via `tower::ServiceExt::oneshot`; neutral fixture creds
  (test-pass-9f3a); each test its own data_dir/vault name (pitfalls #16/#23).
- 276 tests green at context time; gate `./scripts/check.sh`.

## Pitfalls Carried Into This Phase

- #24/#27: chrome CSS is serve-time (good — no Refresh needed for these
  options); toc.js numbering is client-side, grep the book dir not index.html.
- #25: no `{ }` braces inside `format!` HTML — use raw-string consts for the
  injected `<style>`/rewrite snippets.
- `inject_vault_chrome` early-returns when the page already references
  zoom.css/morinet.js (converted vaults) — the new serve-time option rewrites
  must run for ALL HTML pages, so restructure carefully: chrome injection and
  option rewrite are two distinct passes.
- Version bump discipline: Cargo.toml + lib.rs guard + Cargo.lock in ONE
  commit right before the release gate, never mid-run.

## Claude's Discretion (planner must decide + record)

- Exact popup mechanism (inline `<script>` flag on home page vs morinet.js
  data attribute) — must not regress non-locked users.
- Where the locked check lives (helper fn on AppState vs store) — keep it
  cheap, it runs on 4+ routes.
- Whether the theme rewrite is a string replace of the two literal
  assignments or a regex — must be exact-match safe (no false rewrite on
  visitor content); unit-test with a real mdBook page fixture.
