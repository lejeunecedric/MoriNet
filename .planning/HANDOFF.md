# MoriNet — HANDOFF (for the next agent)

> Read this first. Then `.planning/STATE.md` (decisions) and `.planning/ROADMAP.md` (what's left).

## What MoriNet is

A Rust server that exposes Obsidian second brains as mdBook sites with multi-user auth, WebDAV and MCP. Single static binary: `cargo install morinet`.

**Live instance**: https://morinet.treklair.fr — public demo. Credentials are NOT stored in this repo; they live in the systemd unit env (`MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD`) on the server. Server IP intentionally omitted from this public repo.

## What works right now (shipped, deployed)

| Feature | Where |
|---------|-------|
| Login (cookie HttpOnly + HMAC), users in SQLite (bcrypt) | `/login` |
| Multi-vault: local dirs + GitHub repos (git2 clone/sync, mdbook build) | `/vault/<slug>/` |
| ACL per vault (admin sees all, readers need grant) | enforced on every `/vault/<slug>/` |
| Admin UI: invite users, add/delete vaults (local+github), grant/revoke ACL | `/admin/*` (gate: role=admin) |
| Quality + release locally (`scripts/check.sh`, `scripts/release.sh`) → GitHub Deploy or `scripts/deploy.sh` → restart + healthcheck | on demand |
| Styled UI (embedded CSS, dark mode) | `/assets/style.css` |
| Email invitations (24h, single-use) + password reset (1h) — SMTP via `[smtp]`, degrades to temp-password flow when unset | `/invite/{token}`, `/reset`, `/reset/{token}` |
| WebDAV: PROPFIND (depth 0/1) + GET + admin PUT on the vault **source** (markdown in, `mdbook build` out) | `/webdav/<slug>/` (HTTP Basic + same ACL) |
| Nav shell: one header control (account menu: identity → `/account`, Admin entry, logout), pill nav with active state | every page |
| Personal vaults: load from GitHub (quota-capped), owner Refresh/Delete, name auto-filled from the repo | `/` (index) |
| Group quotas (`free`=2, `admin`=∞), role switcher, per-group quota form | `/admin/groups`, `/admin/users` |
| Server-side directory picker (`<dialog>` over `/admin/browse`) + browser-side import | `/admin/vaults` |
| mdBook pages: zoom toolbar + "Close vault" (top right, below the print button), book formatting (1.65 line-height, full-contrast wikilinks) | `/vault/<slug>/` |
| Account page (display-name editing — NOT inside the dropdown) | `/account` |

**Delivery pipeline (since 2026-10-25: EVERYTHING builds HERE, no more GitHub CI)**: `scripts/check.sh` (fmt+clippy+test+build release), `scripts/release.sh vX.Y.Z` (gate + bump + tag + GitHub release with the `morinet-x86_64-unknown-linux-gnu.tar.xz` asset + `.sha256`, draft→assets→publish), `scripts/deploy.sh` (local build + scp staging + atomic mv + restart + /health, env `DEPLOY_HOST`). Only remaining GitHub workflow: `deploy.yml` (unchanged — it consumes the assets of the release created locally, or `workflow_dispatch`).

## Current state at handoff time

- Latest release: **v0.1.20** (anti-clutter UI; design system shipped v0.1.16→v0.1.20). Live: **https://morinet.treklair.fr**.
- `main` is clean and every phase through 09 (ux-nav + design system) is **shipped and deployed**. What is genuinely left: **Phase 10 (MCP)** and **Phase 11 (polish v1.0)** — see ROADMAP.
- The local release publishes the asset BEFORE publishing the release (draft → assets → publish): no more tag/deploy race. If a deploy falls back to "latest release", re-trigger `gh workflow run deploy.yml -f tag=<v>` or `scripts/deploy.sh`.
- treklair has B3Wiki loaded (demo's personal vault). **mdBook themes are written at build time** — after any change to `assets/zoom.*` or `MORINET_CSS`, existing vaults serve old assets until someone clicks **Refresh** (pitfall #24).

## Vault contract (IMPORTANT)

A vault is an **llm-wiki / Obsidian directory**: markdown pages with bare
`[[wikilinks]]` (with `|alias` and `#section`), YAML frontmatter, and a
Karpathy-ish layout (`entities/`, `concepts/`, `comparisons/`, `queries/`,
`raw/`, `index.md`). No `book.toml` needed.

MoriNet **converts it in-pipeline** (AD-14): `sync` → mirror into
`<source>/.morinet-build/` (frontmatter consumed, wikilinks resolved to
relative md links, `SUMMARY.md` + `book.toml` generated, folder landing
pages added) → `mdbook build` on that generated tree → the compiled book is
served at `/vault/<slug>/`. **The source vault is never modified.**

mdBook-native repos (`book.toml` + `src/SUMMARY.md`) still work untouched —
they are detected as `Native` and built directly. A directory matching
neither format is flagged in admin: `LLM-Wiki not detected: this vault might
not work as expected` (PRD-6).

The old raw-wiki auto-scaffold stays reverted (`2b924f3`): the conversion is
the feature, not a scaffold hack.

**Reusable oracles** (need `MORINET_CORPUS=<real wiki>` + `mdbook` in PATH):

```bash
MORINET_CORPUS=/path/to/wiki cargo test -- --ignored --nocapture
```

## Phase 5 (email) — how it works

- `src/email.rs` — `EmailSender` async trait. Prod impl `SmtpSender` (lettre), test impl `InMemorySender`.
- `AppState.email: Arc<dyn EmailSender>` — injected via `build_router_with_email`; `build_router` picks SMTP from `[smtp]` and falls back to a no-op recorder.
- Tokens are stored **sha256-hashed**; the plaintext exists only inside the emailed URL.
- The invite form's `email` field is parsed but **ignored** — identity and role come from the stored invitation, so a tampered field cannot escalate to admin.
- `POST /reset` answers identically for known and unknown logins (no account enumeration) and sends nothing in the unknown case.
- A rejected short password does **not** consume the token.
- Every generated page goes through `app_page()` (AD-12) — a raw `Html(body)` ships unstyled.

## Phase 6 (WebDAV) — how it works

- `src/webdav.rs` is pure logic (no axum); `src/server.rs` holds the handlers. Split keeps the XML/date/joining rules unit-testable.
- Exposes the vault **source** dir = `book_path.parent()` (mdBook always writes `<src>/book`). PUT a `.md`, and `mdbook build` re-runs so the change goes live.
- Auth: HTTP Basic against the same bcrypt users table, then the same per-vault ACL as the web UI. 401 carries `WWW-Authenticate: Basic realm=...`.
- **PUT is admin-only** — readers get 403 even with an ACL grant. No MKCOL/DELETE (out of scope v1).
- Paths are **percent-decoded before** the traversal check. Never parse-then-check with different normalizations.

```bash
curl -u admin@example.com:PASS -X PROPFIND http://localhost:8080/webdav/demo -H 'Depth: 1'
curl -u admin@example.com:PASS http://localhost:8080/webdav/demo/src/note.md
curl -u admin@example.com:PASS -X PUT http://localhost:8080/webdav/demo/src/new.md --data-binary '# hi'
```

## Known pitfalls (all hit, all documented)

1. **cargo-dist removed (2026-10-25)**: `ci.yml`/`release.yml` deleted, builds are local (`scripts/`). The release asset MUST keep the name `morinet-x86_64-unknown-linux-gnu.tar.xz` (+ `.sha256`, `sha256sum` format) and the `<dir>/morinet` directory layout (`--strip-components=1` on the deploy side).
2. **Tag must equal package version**: `dist` refuses to release otherwise. `src/lib.rs` has a test guard (`version_matches_cargo_pkg_version`) — bump both on every release.
3. **ETXTBSY**: never scp/overwrite a running binary — deploy uses `/opt/morinet/staging/` + atomic `mv`.
4. **Deploy can fall back to "latest release"**: if a tag-push deploy races the release, it deploys the previous version. Re-dispatch with `inputs.tag=<tag>` after the release completes.
5. **Env races in tests**: config tests mutate process env — they hold a static `ENV_LOCK` mutex. Keep that pattern when adding env-dependent tests.
6. **git2 needs openssl**: build host requires `libssl-dev` + `pkg-config` (or use `vendored-openssl` feature).
7. **axum 0.8**: path syntax is `/{param}` and `{*wildcard}`; `HeaderMap::from_iter` needs `HeaderValue` (`.parse().unwrap()`); handlers extracting `Extension<Claims>` need `Claims: Clone`.
8. **The patch tool's linter has no Cargo edition** → dozens of fake `E0670 async fn is not permitted in Rust 2015`. Ignore them; `cargo clippy` is the source of truth.
9. **lettre 0.11**: HTML bodies use `SinglePart::html(..).singlepart(..)`. `MessagePart` no longer exists, and `.multipart(..)` is wrong for a single HTML part.
10. **Green tests do not mean a styled UI.** A full browser pass is required for server-rendered HTML (see AD-12).
11. **WebDAV routes need `any(handler)`, not `get(handler)`** — otherwise PROPFIND/PUT get the router's 405 before the handler runs.
12. **Decode the URL path before splitting it.** `%2f` hiding a separator let a `PUT ..%2fescaped.md` through (201 + literal file). Caught by curl, not by the 96 green unit tests.
13. **A hand-rolled HTTP date formatter will be wrong** — the epoch was a Thursday, so `days % 7` misses by 4. Verified against a reference.
14. **Rewiring a data model is not done until the read paths use it.** The group model was fully written and tested while `/vault/<slug>/` still called the old `acl_allows` — every member got a 403. Wiring `access_for()` / `accessible_vaults()` into the index, file serving and WebDAV is what made it real. Test the *route*, not just the store.
15. **axum's `Query` extractor does NOT percent-decode values** (only the path). `?path=/my%20notes` arrives as a literal `%20`. Decode exactly once, before any traversal check — decoding twice makes the checked string differ from the path you build.
16. **Parallel tests sharing a temp-dir name delete each other's fixtures.** Derive it from the test name, not `process::id()` alone; failures then look random.
17. **`git add -A` will commit nested git repos** (e.g. a `.worktrees/` checkout). Check `git status` before committing; `.worktrees/` is in `.gitignore` now.
18. **The remote moves while you work.** Fetch and diff before merging — do not force-push. Several PRs landed mid-session (README description, credits, phase table, a deleted docs file).
19. **Your model of a tool's schema can be out of date — the tool is the oracle.** `mdbook` 0.5 REMOVED `multilingual` from `[book]` ("unknown field") and REJECTS a draft SUMMARY parent that has children ("link items for nested chapters must only contain a hyperlink"). Both bugs passed my unit tests because my fixtures encoded my (wrong) model of mdBook. Only running the real binary on a real corpus found them. Same class of bug as the WebDAV date formatter (pitfall #13) — the fix is the same: an `#[ignore]`d oracle against the real tool + a real corpus.
20. **The demo fixture can be a legitimately-flagged vault.** The `demo` test vault is a bare compiled-HTML dir → it matches NEITHER format, so the PRD-6 warning applies to it too. Don't write a test that assumes "the demo vault is never flagged" — assert per-row instead.
21. **A title is not plain text.** Real wikis put markdown IN titles (`# [Rockasteria](http://…/)`); using a title as `[title](target)` link text then breaks mdBook's parser in ways unit tests with plain titles never see. Sanitize at the boundary (`tree::md_link_text`) and test with a bracketed/link title — same lesson as pitfall #19: the real corpus is the oracle.
22. **"The admin button shows for a non-admin" is usually a role-in-DB problem, not a nav bug.** `index` and `require_admin` both gate on `role_for(email) == "admin"` and are consistent. If a user sees Admin, they ARE admin in the DB — MoriNet's bootstrap creates the admin from the `ADMIN_EMAIL` env, so on treklair the `demo` login is the admin. Check the role in the DB before touching the nav logic.

23. **Shared temp dirs + parallel tests = flaky clone races.** Several route tests deriving the same vault slug (`b3wiki`) into ONE shared `data_dir` clobbered each other's git clones (pitfall #16 family). Give each test its own vault NAME (the slug derives from it) so the clone dirs differ.
24. **mdBook themes are written at BUILD time, and asset constants are compile-time.** After editing `assets/zoom.*` / `MORINET_CSS` (now `include_str!`'d in `tree.rs`), a running server keeps serving the OLD assets — it holds the constants it was compiled with. Order of operations: rebuild → **restart the server** → **Refresh the vault** (only then does `<build>/theme/` regenerate). Fingerprinted URLs (`theme/zoom-<hash>.css`) mean the plain filename 404s — resolve the hash from the served HTML before fetching.
25. **JS/CSS braces inside a Rust `format!` break the string.** A `<script>` block with `{ ... }` inlined in an HTML `format!` produces "invalid format string". Put brace-bearing content in a raw-string const or `include_str!`, never inline.
26. **Fuzzy patch tools corrupt large HTML-template regions** (an eaten closing `"#,`, merged indentation) and the file stops compiling. For big template edits use exact scripted replacement (python, match the current text verbatim) and run `cargo check` immediately after. Also note `write_file` refuses stale content: re-read any file modified through `terminal` before overwriting it.

## What's left (ROADMAP)

Everything through Phase 9b (design system) is done — v0.1.20 is the current
release. The roadmap body keeps the phase history; this section lists only
what is genuinely open:

**Phase 10 — MCP**: `/mcp` StreamableHttp, Bearer auth (token derived from the
password), tools `list_vaults` / `list_notes` / `read_note` / `search_notes`
scoped to the accessible vaults. Not started.

**Phase 11 — polish (v1.0)**: `cargo install morinet` end-to-end check, README
5 sections, `morinet init`, Cargo.lock committed, `v1.0.0` tag, announcement.

**Design**: the UI is in its designed state (v0.1.16→v0.1.20). The rules that
keep it consistent are in the `ui-design-passes` skill (no faded text, body
≥ 13px / line-height 1.55–1.7, no card-in-card, nav links exactly once,
identity strings rendered once, fixed overlays never cover incumbent controls,
one accent, close/back → the parent surface). Run an Impeccable pass
(`/extract`, `/critique`) when the user flags repetition or clutter; the
detector recipe (CSS-inlined snapshots) is in the skill's
`references/detector.md`.

**Deploy-side leftover (vops, not code)**: `demo` is a `reader` in `free`
(quota 2) on treklair; the anti-lockout fallback `admin`/`admin` exists. Set
real `MORINET_ADMIN_EMAIL`/`MORINET_ADMIN_PASSWORD` on the server long-term.

## Testing conventions

- TDD strict, `cargo test` (197 tests green at handoff), clippy `-D warnings`, fmt.
- sqlite fixtures: `Store::connect_memory()`, bcrypt cost default (tests take ~15-25s, that's normal).
- e2e via `tower::ServiceExt::oneshot` on the Router — no network.
- UI verification: run `morinet serve` on :8099 with a throwaway config, then drive Chromium — click the real buttons, follow the real links, screenshot each page. Two unstyled-page bugs were only findable this way. **Measure, don't impress**: compare `getBoundingClientRect()` of competing controls (this is how the Close-vault-over-Print overlap was proven) and read computed styles for typography claims.
- Design checks: `npx impeccable detect <snapshot.html>` on snapshots with every `<link rel="stylesheet">` inlined (the detector does not fetch them). It is static — verify custom-property findings in the browser before "fixing" them (pitfall #24 family).
- API verification: `curl -X PROPFIND` against the running server (the roadmap also suggests a `cadaver` walkthrough). This is how the percent-encoded traversal hole was found — unit tests were green.
