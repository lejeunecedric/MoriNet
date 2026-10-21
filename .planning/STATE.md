# MoriNet — STATE (live session state)

> Updated on every delivered bite / decision made / notable error.

---

## Current position (2026-10-21 — v0.1.10: personal vault loading + fallback admin + SUMMARY fix)

- **Latest release**: **v0.1.10** (users load GitHub vaults into their personal space with the group quota; `admin`/`admin` anti-lockout fallback; mdBook-unsafe titles sanitized — the real B3Wiki loads and serves end-to-end)
- **Live instance**: https://morinet.treklair.fr — public demo instance, admin-ui accessible via the **Admin** nav link on the index. Credentials live on the server, not in this repo
- **Phases done**: 00 bootstrap, 01 core-serve, 02 multi-vault, 03 vault-sources, 04 admin-ui, 05 email, 06 webdav, **08b obsidian-format** (08 access-model à 60 %)
- **177/177 tests** (135 → 143 frontmatter+links → 157 tree → 161 convert → 165 wiring → 167 logout/About → **177 group quotas**), clippy `-D warnings`, fmt clean — every push of a tag auto-releases and auto-deploys
- **Phase 6 (WebDAV) shipped**: PROPFIND (depth 0/1) + GET + admin PUT on `/webdav/<slug>/…`. Exposes the vault **source** (`book.toml` + `src/`), not the compiled `book/` — PUT markdown then `mdbook build`. HTTP Basic against the same bcrypt users + the same per-vault ACL
- **Phase 5 (email) shipped**: `EmailSender` trait + lettre SMTP + `InMemorySender` mock, `[smtp]` in config, invitation links (24h, single-use, sha256-hashed) and password reset (1h), all UI pages styled. Admin invite falls back to the old temp-password flow when SMTP is absent/invalid
- **Vault contract (superseded)**: mdBook-native (`book.toml` + `src/SUMMARY.md`) was the v1 contract. **Phase 8b (v0.1.8) replaced it**: MoriNet now loads **llm-wiki/Obsidian** vaults (bare `[[wikilinks]]` + YAML frontmatter, no `book.toml`) and converts them in-pipeline into a generated mdBook tree under `<src>/.morinet-build/` (AD-14). mdBook-native repos still work untouched. The old raw-wiki auto-scaffold stays reverted (`2b924f3`) — the conversion is the proper feature, not a scaffold hack.
- **Deploy quirks**: staging + atomic `mv` (ETXTBSY), re-dispatch may be needed right after a release finishes (tag-push deploy races the release and falls back to "latest release")
- **Planning refreshed (2026-10-21)**: a new **Phase 7 — vault-ux** was inserted BEFORE MCP (MCP → 08, polish → 09). User-driven: adding a local vault should not require typing a server path.
- **Phase 8 — access-model** (user decision 2026-10-21): the merged-book idea is ABANDONED. Replaced by groups + personal vaults: `groups`/`group_members`, `vaults.scope` (global|personal) + `owner_id`, `vault_group_acl(vault, group, role)`. A personal vault is strictly solo (owner always has write). REQ-GRP-001..004, REQ-VLT-001..007.
- **Phase 9 — ux-nav**: new shell — My vaults landing, account menu top-right (name/email/logout), discreet close button on mdbook pages, Admin menu only for role=admin.
- **Email change requires confirmation** at the new address (`pending_emails`); unconfirmed changes never break the existing login.
- **Obsidian format blocker RESOLVED & SHIPPED (2026-10-21)**: in-pipeline converter (AD-14/PRD-6) — llm-wiki vault → generated mdBook tree in `.morinet-build/`, wikilinks à l'Obsidian, frontmatter consommé, SUMMARY régénéré à chaque sync. Corpus réel mesuré en local (ProgWiki) : 131 .md, 812 wikilinks, 542 aliases, 0 book.toml. **E2E vérifié en navigateur** : liens résolus, chips méta, sidebar avec folder notes, PUT WebDAV → note re-convertie et live sans redémarrage.
- **Next**: Phase 8 (écran vault personnel) → Phase 7 bites 2..5 (picker modal, import) → Phase 9 ux-nav (+ `/logout`) → Phase 10 (MCP) → Phase 11 (polish v1.0)
- **Phase 9 pull-forward (2026-10-21, feedback user sur la page admin)**: `POST /logout` + bouton logout, nav admin clarifiée (My vaults vs Vaults mgmt — le lien "Vaults" ambigu), bouton **About** (version installée + repo GitHub, REQ-ADMIN-007). En cours d'implémentation.
- **Not started**: no nav shell yet; no screen to create a personal vault (store method exists). `/logout` now EXISTS (Phase 9 pull-forward, 2026-10-21).
- **Quota par groupe (IMPLÉMENTÉ 2026-10-21, pas encore release)**: migration `0005_group_quota.sql` (`groups.personal_vault_limit`), `create_personal_vault` refuse au-delà de la limite (REQ-VLT-008), groupes builtin `free`(2)/`admin`(∞) semés au boot + auto-join (`create_user`) + sweep `ensure_default_memberships` + bascule `set_user_role` (UI `/admin/users`, formulaire quota `/admin/groups`). **Diagnostic posé** : le bouton Admin visible pour demo n'est PAS un bug de code — `index` et `require_admin` testent bien `role_for(...)=="admin"` (`src/server.rs`). C'est que **demo a le rôle `admin` en base** (le bootstrap MoriNet crée l'admin avec l'env `ADMIN_EMAIL` ; sur treklair cet email est `demo`). **Reste côté deploy** : dégrader demo en reader (UI ou SQL) — la bascule le met automatiquement dans `free` (quota 2). REQ-GRP-005..008 + REQ-VLT-008. 177 tests.
- **v0.1.10 (2026-10-21)**: `POST /vaults/create` (session) loads a GitHub vault as a PERSONAL vault (REQ-VLT-005/008) — quota checked before cloning, slug from the name, llm-wiki conversion shared with the admin path. Load form on the index ("My vaults"). `ensure_fallback_admin()` creates/promotes `admin`/`admin` at boot when NO admin exists (demoting the only admin used to lock the instance out of /admin forever — the treklair incident). `md_link_text()` sanitizes titles used as `[..](..)` link text: `[Rockasteria](http://x/)` H1s crashed the SUMMARY parse on the real B3Wiki ("Expected a start of a link"). `mdbook` missing from PATH is now a LOUD failure (was a silent fallback → undebuggable 404). Server handlers use `cfg.data_dir` (was a CWD-relative "data"). CI installs mdbook 0.5.4. 185 tests.
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
