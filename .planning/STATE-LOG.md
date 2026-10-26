# MoriNet — STATE (live session state)

> Updated on every delivered bite / decision made / notable error.

---

## Current position (2026-10-21 — v0.1.20: anti-clutter UI, design system complete)

- **Latest release**: **v0.1.20** — anti-clutter: a single header control (the Admin entry lives in the account menu), zero duplicated titles on the index, "Shared with me" always sectioned, vaults as rows, rename form on `/account`. Design system stabilized v0.1.16→v0.1.20 (Impeccable passes + Linear template).
- **Live instance**: https://morinet.treklair.fr — public demo instance, admin-ui accessible via the account menu → **Admin**. Credentials live on the server, not in this repo
- **Phases done**: 00 bootstrap, 01 core-serve, 02 multi-vault, 03 vault-sources, 04 admin-ui, 05 email, 06 webdav, 07 vault-ux (picker + import), 08 access-model (groups + personal vaults + quotas), 08b obsidian-format, 09 ux-nav (nav shell + design system)
- **197/197 tests** (… → 177 group quotas → 185 load-vault/anti-lockout → 190 delete/refresh → 192 zoom/close → 193 source_url → 197 nav shell/restyle), clippy `-D warnings`, fmt clean — every push of a tag auto-releases and auto-deploys (Linux-only dist since v0.1.16, zero re-dispatch needed)
- **Phase 6 (WebDAV) shipped**: PROPFIND (depth 0/1) + GET + admin PUT on `/webdav/<slug>/…`. Exposes the vault **source** (`book.toml` + `src/`), not the compiled `book/` — PUT markdown then `mdbook build`. HTTP Basic against the same bcrypt users + the same per-vault ACL
- **Phase 5 (email) shipped**: `EmailSender` trait + lettre SMTP + `InMemorySender` mock, `[smtp]` in config, invitation links (24h, single-use, sha256-hashed) and password reset (1h), all UI pages styled. Admin invite falls back to the old temp-password flow when SMTP is absent/invalid
- **Vault contract (superseded)**: mdBook-native (`book.toml` + `src/SUMMARY.md`) was the v1 contract. **Phase 8b (v0.1.8) replaced it**: MoriNet now loads **llm-wiki/Obsidian** vaults (bare `[[wikilinks]]` + YAML frontmatter, no `book.toml`) and converts them in-pipeline into a generated mdBook tree under `<src>/.morinet-build/` (AD-14). mdBook-native repos still work untouched. The old raw-wiki auto-scaffold stays reverted (`2b924f3`) — the conversion is the proper feature, not a scaffold hack.
- **Deploy quirks**: staging + atomic `mv` (ETXTBSY). Historically the tag-push deploy raced the release ("falling back to latest release") → fixed in v0.1.16 (CI Linux-only, the deploy waits for the release to be published with its assets).
- **Phase 8 — access-model** (user decision 2026-10-21): the merged-book idea is ABANDONED. Replaced by groups + personal vaults: `groups`/`group_members`, `vaults.scope` (global|personal) + `owner_id`, `vault_group_acl(vault, group, role)`. A personal vault is strictly solo (owner always has write). REQ-GRP-001..004, REQ-VLT-001..007.
- **Phase 9 — ux-nav: DONE** (v0.1.14-v0.1.20) — shared header (1 single control: account menu with identity/`/account`/Admin/logout), admin pill nav with active state, Close vault button + zoom on mdBook pages (top-right bar below the Print button), `/account` page (rename).
- **Email change requires confirmation** at the new address (`pending_emails`); unconfirmed changes never break the existing login.
- **Obsidian format blocker RESOLVED & SHIPPED (2026-10-21)**: in-pipeline converter (AD-14/PRD-6) — llm-wiki vault → generated mdBook tree in `.morinet-build/`, Obsidian-style wikilinks, frontmatter consumed, SUMMARY regenerated on every sync. Real corpus measured locally (ProgWiki): 131 .md, 812 wikilinks, 542 aliases, 0 book.toml. **E2E verified in the browser**: links resolved, meta chips, sidebar with folder notes, WebDAV PUT → note re-converted and live without a restart.
- **Next**: Phase 10 (MCP StreamableHttp on `/mcp`) → Phase 11 (polish v1.0). Everything else is shipped — see ROADMAP.
- **Per-group quota (shipped v0.1.9, see the versioned entry)**: migration `0005_group_quota.sql` (`groups.personal_vault_limit`), `create_personal_vault` refuses beyond the limit (REQ-VLT-008), builtin groups `free`(2)/`admin`(∞) seeded at boot + auto-join (`create_user`) + sweep `ensure_default_memberships` + `set_user_role` toggle (UI `/admin/users`, quota form `/admin/groups`). **Diagnosis established**: the Admin button visible for demo is NOT a code bug — `index` and `require_admin` correctly test `role_for(...)=="admin"` (`src/server.rs`). It is that **demo has the `admin` role in the DB** (the MoriNet bootstrap creates the admin from the `ADMIN_EMAIL` env var; on treklair that email is `demo`). **Remaining on the deploy side**: demote demo to reader (UI or SQL) — the toggle automatically puts it in `free` (quota 2). REQ-GRP-005..008 + REQ-VLT-008. 177 tests.
- **v0.1.10 (2026-10-21)**: `POST /vaults/create` (session) loads a GitHub vault as a PERSONAL vault (REQ-VLT-005/008) — quota checked before cloning, slug from the name, llm-wiki conversion shared with the admin path. Load form on the index ("My vaults"). `ensure_fallback_admin()` creates/promotes `admin`/`admin` at boot when NO admin exists (demoting the only admin used to lock the instance out of /admin forever — the treklair incident). `md_link_text()` sanitizes titles used as `[..](..)` link text: `[Rockasteria](http://x/)` H1s crashed the SUMMARY parse on the real B3Wiki ("Expected a start of a link"). `mdbook` missing from PATH is now a LOUD failure (was a silent fallback → undebuggable 404). Server handlers use `cfg.data_dir` (was a CWD-relative "data"). CI installs mdbook 0.5.4. 185 tests.
- **v0.1.11 (2026-10-21)**: `POST /vaults/{slug}/delete` (owner-only, frees the quota slot — without it a `free` member could exhaust their 2 slots for life) + `POST /vaults/{slug}/refresh` (re-pull upstream + rebuild, so pages added after the load appear without reloading). `vaults.source_url` (migration 0006) remembers where a vault came from. Refresh/Delete buttons on the index personal rows. 190 tests.
- **v0.1.12 (2026-10-21, UX feedback)**: (1) **mdBook zoom** — −/100%+/↺ bar bottom-right, ported from oscbrain.hollitech.fr (`assets/zoom.js` + `zoom.css` → `theme/` injected via `additional-js`/`additional-css` in the generated `book.toml`); it zooms the content AND the sidebar in lockstep (`--sidebar-width` follows) and persists in `localStorage` (`morinet-zoom`). (2) **"Close vault" button** top-right on mdBook pages, relative path computed from `location.pathname` (works at any depth). (3) **the vault name autofills** with the repo directory name when you paste the GitHub URL (a manually entered name is never overwritten) — also server-side if the field is empty. 192 tests.
- **v0.1.13 (2026-10-21)**: **refresh** now recovers the upstream URL from the clone's git remote when `source_url` is empty (vaults loaded before v0.1.11) — otherwise "no upstream to refresh". The URL is remembered at load time from now on.
- **v0.1.14 (2026-10-21)**: **Phase 9 nav shell** — shared header on every page (brand + My vaults + Admin menu `role=admin` only + account menu: display name editable via `POST /account/name`, email, logout) — the dropdowns are native `<details>`, zero JS. + **hardened deploy workflow**: the tag push uses `github.ref_name` (instead of falling back to "latest release" = the previous version) and **waits** for the release to be published with its assets before downloading.
- **v0.1.15 (2026-10-21)**: **Phase 7 vault-ux** — the server-side **folder picker** (REQ-UX-001): native `<dialog>` on `/admin/browse`, descent into folders + breadcrumb/Up, Select that fills path/slug/name (the folder name). The vault form no longer requires typing a server path. Pitfall found: global `[name="name"]` selectors intersect the account menu rename field → scoped to the form; and `root.join(path)` with a path starting with `/` **replaces** the root (Path::join) → `trim_start_matches('/')`.
- **v0.1.16 (2026-10-21, feedback "big look problem")**: complete restyle of the app UI — consistent light AND dark theme variables (--bg/--card/--border/--accent…), card header with brand-mark, **Admin** ⚙ menu (accent badge) DISTINCT from the account menu (avatar + initial + caret) — the "Admin/admin" visual duplicate was the feedback's #1 defect; panels/cards, stacked forms (labels above), slim tables, admin pill nav with **active state**, reworked empty states ("No personal vaults yet."), drop-downs with themed panel (no more white in dark). **CI Linux only** (dist-workspace.toml: 2 linux targets, install shell only) — flaky Mac runners caused a release failure.
- **v0.1.17 (2026-10-21, Impeccable pass + popular-web-designs)**: repetition removal and ergonomic overhaul. **A single menu** (account: identity / Display name / Log out, separators, closes on outside click + Escape via MENU_JS); the Admin control is a **single link** `/admin` — the sections live in the tab bar, no more duplicated link lists (Users/Groups/Vaults/About were in both the dropdown AND the tab bar). Consolidated CSS: radius scale (--r-1/2/3/pill), `--on-accent` (1 rule instead of 3 dark overrides), grouped surfaces, forms flat in the panels (no more card-in-card), deduplicated menu identity (the email only shows if it differs from the name). Linear vocabulary (luminance stepping, whisper-thin borders, 1 accent).
- **v0.1.18 (2026-10-21, vault chrome bugfix)**: the Close vault button pointed at `/vault/<slug>/` (the vault root) instead of `/` → "didn't work"; it was tiny (63×18px) and overlapped mdBook's Print button (y≈14-36). Fix: **a single fixed bar top-right** (`top: 44px`, below Print) = zoom controls + "✕ Close vault" pill (110×28px, href `/`). Assets moved out of baked constants: `tree.rs` does `include_str!` on `assets/zoom.js|css` (editable source of truth). ⚠️ mdBook themes are written at **build** time: any already-built vault must be **Refresh**ed to serve the new assets (v0.1.13 pitfall).
- **v0.1.19 (2026-10-21, Impeccable critique of mdBook formatting)**: findings from the `npx impeccable detect` detector (61 rules) + review: `note-meta` chips at **8px + opacity .85** (tiny text + fade), body line-height **1.45** (tight), `.wikilink-missing` at **opacity .55** (low-contrast), `--icons` dark at 2:1, Open Sans (overused font). Fixes: meta 13px without fade, line-height **1.65**, wikilink without fade (dotted underline kept), `--icons` dark overridden to `#7a8290` (≈4:1), body font = system stack (consistent with the app). ⚠️ The detector is static: it keeps flagging `--icons` because it sees mdBook's original declaration, not the cascade (verified at runtime: `#7a8290`). Critique method: HTML snapshots with ALL CSS inlined (otherwise the typography/contrast rules don't run).
- **v0.1.20 (2026-10-21, anti-clutter index + header)**: user screenshot ("big duplication problem + cluttered", "the 2 admin buttons are a showstopper"). Fixes: (1) **a single header control** — the "⚙ Admin" pill next to the account menu (which showed "admin") = 2 admin buttons → the Admin entry lives INSIDE the account menu; (2) end of the "My vaults" title duplicate (H1 + panel) — the H1 carries the list; (3) "Shared with me" = always a real section (the orphan "No shared vaults yet." floated without a title); (4) vaults as **rows** (name left, actions right, ghost buttons — the accent is reserved for the primary action); (5) the rename form leaves the dropdown → **`/account`** page (a menu = navigation, a form = editing). Design rule applied (ui-design-passes skill): same links ONCE only, identity rendered once, no form inside a menu.
- **Agent handoff notes**: `.planning/HANDOFF.md` (pitfalls, conventions, open items)

## Architectural Decisions (AD)

> Recorded here so we can revisit them later. Each AD = a decision + rationale + date.

- **AD-1** *(2026-10-20)* — **Cargo workspace MONO-CRATE** (no multi-crates).
  - Rationale: MoriNet is a single binary, no need to split into crates. We avoid multi-crate workspace complexity for a YAGNI win. If we ever split, we do it once the code reaches ~3000 LOC.
- **AD-2** *(2026-10-20)* — **mdbook UPSTREAM + custom "mori" theme injected** (no fork).
  - Rationale: mdbook is a stable tool, we don't need to modify it. We inject our CSS/JS assets (zoom.css, fold.css, etc.) via `additional-css` / `additional-js` in the `book.toml` generated for each vault. If upstream mdbook breaks a compat, we patch `additional-css` rather than the binary.
- **AD-3** *(2026-10-20)* — **SQLite by default**, Postgres = feature flag `db = "postgres"`.
  - Rationale: SQLite is portable, zero-setup for a single-instance MoriNet. Postgres supported later if we need multi-instance / HA. sqlx abstracts the layer.
- **AD-4** *(2026-10-20)* — **Auth via HttpOnly signed cookies** (HMAC key in config).
  - Rationale: no JWT (XSS + revocation complexity). Server-side signed HttpOnly cookie → trivial revocation via Redis blacklist (out of scope v1) or short expiry. bcrypt for passwords.
- **AD-5** *(2026-10-20)* — **WebDAV = custom axum handlers** (no crate).
  - Rationale: the WebDAV spec we support is minimal (PROPFIND, GET, optional PUT). A crate would add ~10 deps and extra state. We write 3 axum handlers in ~200 lines, full control.
- **AD-6** *(2026-10-20)* — **MCP = `rmcp` crate** (official Rust MCP).
  - Rationale: rmcp is the official crate, StreamableHttp is supported, JSON schemas auto-generated. No need to reinvent. Transport: StreamableHttp (simplest for HTTP, slowly deprecating SSE). Phase 1 tools: list_vaults, list_notes, read_note, search_notes.
- **AD-7** *(2026-10-20)* — **SMTP = `lettre` crate**, TOML config.
  - Rationale: lettre supports SMTP + STARTTLS + SMTPS, has a clean async API, multi-TLS backends. Config TOML for host/port/user/pass/from/tls. Tested via an in-memory mock.
- **AD-8** *(2026-10-20)* — **Vault source = `VaultSource` trait**, with `Local` + `Github` impls.
  - Rationale: the admin table stores `vaults(id, slug, source_kind, source_config_json, book_path)`. Sync is periodic (background task), not per-request. Github uses `git2` (libgit2 binding) rather than shelling out — more deterministic and auditable. Default periodic sync = 5 minutes, configurable.
- **AD-9** *(2026-10-20)* — **Server-side HTML templates via `askama`** (no SPA, no JS framework).
  - Rationale: admin UI is minimalist; Askama = Jinja-like templates compiled in Rust, zero runtime overhead, type-safe. No React/Vue/Svelte = no frontend toolchain to maintain. JS only for the mdbook customs (zoom, fold).
- **AD-10** *(2026-10-20)* — **mdbook build triggered after sync**, not per request.
  - Rationale: `mdbook build` takes ~1-3s for a 100-page vault. Rebuilding on every GET is wasteful. Sync rebuilds on change; hot-reload disabled in prod.
- **AD-11** *(2026-10-20)* — **Deploy pipeline: each published GitHub release auto-deploys to holldemo** (`morinet.treklair.fr`). Server host/IP intentionally not recorded here (public repo).
  - Rationale: user wants a staging target to test every release. Implemented as a SEPARATE `.github/workflows/deploy.yml` (NOT in `release.yml` — cargo-dist drift check forbids hand-editing that file, see Notable errors). Trigger: `release: published` + `workflow_dispatch` for manual runs. Phase 0 scope: download release tarball, sha256-verify, scp the binary to `/opt/morinet/bin/`, smoke-check `morinet --version` over SSH. Phase 1+ adds systemd restart + `/health` check. Server-side provisioning (systemd unit, nginx vhost, TLS) is ops-side.

- **AD-12** *(2026-10-20)* — **Generated pages MUST go through `app_page()`**, never a raw `Html(body)`.
  - Rationale: a browser pass caught that every handler-generated page (`/invite/{token}`, `/reset/{token}`, error pages) shipped WITHOUT the stylesheet link — default serif, no card, no layout. Unit tests were green the whole time. Lesson recorded: server-rendered HTML needs a rendering check, not just an HTTP status + body-substring assertion. `app_page()` is the single choke point so a new page cannot forget the CSS again.

- **AD-13** *(2026-10-20)* — **WebDAV exposes the vault SOURCE, not the compiled book**, and the handler takes a single `Request` extractor instead of stacked extractors.
  - Rationale: (1) the useful direction is PUT-markdown-then-render, so serving `book/` HTML would be pointless; the store keeps `<src>/book` so the source is its parent. (2) `.route(..., get(h))` matches GET only — PROPFIND/PUT fell through to the router's 405. (3) stacking `Method`/`Uri`/`HeaderMap`/`Body`/`Path` extractors made axum return 500 *before* the handler ran; one `Request` + manual extraction is robust and easier to read.

- **AD-14** *(2026-10-21)* — **Obsidian/llm-wiki support = in-pipeline converter**, not an mdBook preprocessor, not a custom renderer.
  - Rationale: mdBook preprocessors receive the book AFTER `SUMMARY.md` is parsed, so they can never fix the missing structure — and the missing structure IS the problem. The converter (`sync → .morinet-build/` mirror with generated `book.toml` + `SUMMARY.md` + rewritten wikilinks + consumed frontmatter) keeps mdBook upstream (`AD-2` respected), is pure Rust and unit-testable, and never writes into the user's source vault. A real-corpus oracle (`MORINET_CORPUS=… cargo test -- --ignored`) validates the parser against 127 living pages.

## Product Decisions (PRD)

- **PRD-5** *(2026-10-20)* — Staging target = `morinet.treklair.fr` on holldemo, DNS A record set by user. Host/IP kept out of this public repo.
  - User decided. Deploys happen on every GitHub release publication.

- **PRD-1** *(2026-10-20)* — Stack 100% Rust.
  - User decided: `cargo install morinet` (static binary).
- **PRD-2** *(2026-10-20)* — Repo at github.com/`lejeunecedric`/MoriNet (public).
  - Username confirmed.
- **PRD-3** *(2026-10-20)* — Multi-vault = one mdbook per vault, sub-paths `/vaultA/`, `/vaultB/`.
  - User decided. No sidebar switcher (possible later).
- **PRD-4** *(2026-10-20)* — SMTP = generic configurable adapter (no Brevo integration).
  - User decided. We code a generic `lettre` adapter with host/port/user/pass.

- **PRD-6** *(2026-10-21)* — **MoriNet only loads llm-wiki (Obsidian-format) vaults.** Detection is inverted: the absence of `book.toml`/`src/SUMMARY.md` selects the llm-wiki pipeline; a vault matching *neither* format is flagged in admin with `LLM-Wiki not detected: this vault might not work as expected`. User decided (wording theirs).

## Notable errors & learnings

- **2026-10-20 (Phase 6)**: **percent-decode BEFORE splitting a URL path.** `PUT /webdav/demo/..%2fescaped.md` returned 201 and wrote a literal file named `..%2fescaped.md`. `rel_from_uri` read `uri.path()` raw, so `%2f` was not a separator and the traversal check never saw it. Found by a real curl pass, NOT by the 96 unit tests that were green. Decoding first makes `%2e%2e` / `%2f` visible to `safe_join`. Any parser that normalizes one way and checks another is a hole.
- **2026-10-20 (Phase 6)**: `days % 7` gives the wrong weekday — the Unix epoch (1970-01-01) was a **Thursday**, so the Sunday-first table needs a `+4` offset. A hand-rolled RFC 7231 date formatter is a trap; verify against a reference (Python) for several instants.
- **2026-10-20 (Phase 6)**: WebDAV methods are NOT in `http::Method` — `Method::PROPFIND` does not exist. They arrive as extension methods; match `method.as_str()`.
- **2026-10-20 (Phase 6)**: a route declared `.route(p, get(h))` answers 405 to PROPFIND/PUT before the handler is ever called. WebDAV routes need `any(h)` + in-handler dispatch.

- **2026-10-20 (Phase 5)**: **the patch tool's linter compiles without the Cargo edition** → dozens of bogus `E0670: async fn is not permitted in Rust 2015` on healthy files. Do NOT "fix" them. Use `cargo clippy` for the real verdict.
- **2026-10-20 (Phase 5)**: lettre 0.11 renamed `MessagePart` → `SinglePart`, and the builder method is `.singlepart(...)`, not `.multipart(...)` for a single HTML part. Verified by reading the crate source instead of guessing.
- **2026-10-20 (Phase 5)**: green unit tests hid a completely unstyled UI. Every page built with `format!` returned `Html(body)` without the `<link rel=stylesheet>`. Fixed by funnelling all of them through `app_page()` (AD-12). Lesson: for server-rendered HTML, assert on the rendered page (browser), not only on status codes and body substrings.

- **2026-10-20**: cargo-dist's generated tag pattern `**[0-9]+.[0-9]+.[0-9]+*` doesn't match `v0.1.0` because GitHub tag patterns treat `+` as a literal character (fnmatch-style, no quantifier). The release workflow never auto-triggered on the tag push. Fix: changed pattern to `'v[0-9]*.[0-9]*.[0-9]*'` and added `workflow_dispatch:` so we can manually trigger via API. Lesson: when using tool generators, always sanity-check the trigger pattern with a real test push.

- **2026-10-25 (delivery)** — **Compilation removed from GitHub, everything is local** (user decision). `ci.yml` and `release.yml` removed; replaced by `scripts/check.sh` (fmt+clippy+test+build), `scripts/release.sh` (gate → bump → tag → GitHub release draft → upload asset `morinet-x86_64-unknown-linux-gnu.tar.xz` + `.sha256` → publish; GITHUB_TOKEN required) and `scripts/deploy.sh` (build + scp staging + atomic mv + restart + /health; DEPLOY_HOST required). `deploy.yml` is KEPT unchanged (triggered by release:published, same asset names as cargo-dist). The tag↔release race pitfall is gone: assets are attached before publication.

- **2026-10-25 (v0.1.21)** — first 100% local deploy: `scripts/deploy.sh` (SSH to the demo server — host in the gitignored local note, staging ~ + sudo mv + restart + /health). Pitfall: the local binary was linked to `libssl.so.4` (Ubuntu 26.10) absent from the server -> `git2` with `vendored-openssl`, no more dynamic openssl deps (verified with ldd). `version_matches_cargo_pkg_version` guard bumped at every release (lib.rs).

- **2026-10-25 (10.1)** — Home UX delivered locally: index with 3 sections (single vault_row helper = structural parity), Edit (rename + re-source + allow_reshare), direct sharing by email (vault_shares reader/writer, hidden per-sharee, pending unknowns, re-share gated), Stats page (owner panel vs personal counter for shared vaults), best-effort access_log web+MCP (90-day purge at boot), global vault "MoriNet Documentation" seeded at boot (6 embedded EN pages, idempotent marker, grant every boot). 262 tests. E2E fresh-install verified (3 boots, operator edits preserved). Executor pitfall: read server.rs/store.rs in windows of ≤100 lines (3 agents died at 262k tokens); one run = one task.

- **2026-10-25 (v1.0.0)** — RELEASE v1.0.0: scripts/release.sh fixed (lib.rs guard bumped atomically, no-op tolerated), check.sh gate, tag v1.0.0, GitHub release draft→assets→publish (id 423655496, x86_64 asset + sha256), direct deploy to holldemo → server on 1.0.0, /mcp 401, docs vault seeded on the demo, home 3 sections live. Frictions: fine-grained PAT created read-only (403 until Contents=Read and write); --follow-tags does not push lightweight tags.

## Stats

- Planning files: 5 (ROADMAP, REQUIREMENTS, STATE, index, Phase 0 PLAN)
- Locked ADs: 10
- REQ catalog: 48 (5 INFRA + 5 AUTH + 10 VAULT + 6 ADMIN + 5 EMAIL + 5 WEBDAV + 7 MCP + 5 OPS + 5 OBS approx)
- Phases defined: 8
- Commits on origin main: 11
- CI: green ✅

## Useful links

- ROADMAP: `.planning/ROADMAP.md`
- REQUIREMENTS: `.planning/REQUIREMENTS.md`
- Phase 0 PLAN: `.planning/phases/00-bootstrap/01-bootstrap-PLAN.md`
- Human index: `.hermes/plans/00-index.md`
- Reference: `https://oscbrain.hollitech.fr/` (for Phase 8 — zoom/fold assets to clone)
