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
| CI → Release (cargo-dist, 16 assets, 5 platforms) → Deploy → restart + healthcheck | every `git push origin v0.1.x` |
| Styled UI (embedded CSS, dark mode) | `/assets/style.css` |
| Email invitations (24h, single-use) + password reset (1h) — SMTP via `[smtp]`, degrades to temp-password flow when unset | `/invite/{token}`, `/reset`, `/reset/{token}` |
| WebDAV: PROPFIND (depth 0/1) + GET + admin PUT on the vault **source** (markdown in, `mdbook build` out) | `/webdav/<slug>/` (HTTP Basic + same ACL) |

**Deploy pipeline** (`.github/workflows/`): `ci.yml` (fmt+clippy+test+build), `release.yml` (cargo-dist — **NEVER hand-edit it**, drift check), `deploy.yml` (staging scp + atomic mv + `sudo systemctl restart morinet` + `/health`).

## Current state at handoff time

- Latest release: **v0.1.8** (Phase 8b obsidian-format — llm-wiki/Obsidian vaults converted & served natively). Live: **https://morinet.treklair.fr**.
- On `main`, **not yet released**: Phase 9 pull-forward (logout + About page + clarified admin nav) — commit `b307e18`. It's tested (167/167) and CI-green; needs a version bump + tag to ship.
- The deploy auto-triggers on tag push AND on release published, but **re-dispatch may be needed right after a release finishes** (the tag-push deploy can race ahead of the release and fall back to "latest release"). Watch for that.
- treklair has **no vaults configured yet** — add one via `/admin/vaults` (kind `github` needs a repo with `book.toml` + `src/SUMMARY.md`; or a local llm-wiki dir, which the v0.1.8 converter now serves natively).

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

1. **cargo-dist drift check**: NEVER hand-edit `.github/workflows/release.yml` — CI fails. Generate via `~/.cargo/bin/dist` (installed as `dist`, not a cargo subcommand). Put custom logic in separate workflows.
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
21. **"The admin button shows for a non-admin" is usually a role-in-DB problem, not a nav bug.** `index` and `require_admin` both gate on `role_for(email) == "admin"` and are consistent. If a user sees Admin, they ARE admin in the DB — MoriNet's bootstrap creates the admin from the `ADMIN_EMAIL` env, so on treklair the `demo` login is the admin. Check the role in the DB before touching the nav logic.

## What's left (ROADMAP)

**Phase 8b (obsidian-format) — DONE, shipped in v0.1.8.** The vault contract
changed: a vault is now an **llm-wiki/Obsidian directory** (see "Vault
contract" above), converted in-pipeline. mdBook-native repos still work.
The two real-mdBook-0.5 bugs are recorded as pitfall #19.

**Still open in the access model (Phase 8, ~60 %)**
- UI to create a **personal** vault (the store method exists; no screen yet)
- "My vaults" / "Shared with me" sections exist on the index but there is no **nav shell** yet (Phase 9)

**Phase 9 pull-forward (2026-10-21, feedback user sur la page admin)** — DONE:
- `POST /logout` (REQ-ACC-003) + bouton Logout dans la nav admin et sur l'index
- Nav admin clarifiée : "My vaults" (landing) vs "Vaults mgmt" (admin) — plus de lien "Vaults" ambigu (REQ-NAV-006)
- Bouton **About** (REQ-ADMIN-007) : page `/admin/about` montrant `morinet <version>` (via `crate::version()`, donc auto à chaque release) + URL du repo GitHub
- Reste de Phase 9 : nav shell complet, menu compte (display name/email), bouton close sur les pages mdBook, menu Admin en dropdown

**Phase 9 — ux-nav** (partiellement pull-forwardé — voir ci-dessus) :
- Shared header on every page, account menu top-right (display name, email, logout)
- `POST /logout` — ✅ DONE (2026-10-21) + bouton Logout dans la nav admin et l'index
- mdBook pages get a discreet close button (top-right) back to `/`
- Admin menu as a dropdown, rendered only for role=admin

**Phase 7 — vault-ux** (bites 2..5 not started):
- The picker modal (list dirs, descend, up, breadcrumb) over the finished `/admin/browse`
- Browser-side import that **creates a new vault** from a local folder (slug = folder name, scaffolds `book.toml` + `src/SUMMARY.md`)

**Next step to resume — group quota (Phase 8, user decision 2026-10-21)** — **IMPLÉMENTÉ (non release)** :

- `groups.personal_vault_limit` (NULL = unlimited) — REQ-GRP-005 → `migrations/0005_group_quota.sql`
- Builtin groups at bootstrap: `free` (limit **2**), `admin` (unlimited) — REQ-GRP-006 → `Store::ensure_builtin_groups()` (called at boot in `main.rs`)
- **`demo` becomes a `reader` in `free`, NOT admin**; the bootstrap admin is in `admin` — REQ-GRP-007 → `Store::set_user_role()` (UI: role switcher on `/admin/users`), `create_user` auto-joins `free`/`admin`, and `Store::ensure_default_memberships()` sweeps groupless accounts at boot
- The quota caps **personal vaults a member can CREATE** — shared/global vaults do NOT count, and visibility is NOT capped — REQ-VLT-008 → enforced inside `create_personal_vault`
- Effective limit for a user = the most permissive across their groups (NULL beats any; otherwise max) — REQ-GRP-008 → `Store::effective_personal_vault_limit()`
- Admin UI: quota form per group on `/admin/groups` (`POST /admin/groups/{id}/limit`, empty = unlimited), role switcher on `/admin/users`
- Verified E2E in a real browser (AD-12): builtin groups seeded, quota form set/restore, role switch reader↔admin, demoted user lands in `free`. 177/177 tests.

**Deploy-side leftover (vops, not code)**: on treklair, `demo` is currently role `admin` because the bootstrap admin env is `demo`. Demote it to `reader` (one click in `/admin/users`, or SQL) — the boot sweep then puts it in `free` with quota 2, and the Admin button disappears for that account. Longer term: set `MORINET_ADMIN_EMAIL` to a real admin address on the server.

**Why the Admin button showed for demo (NOT a code bug — do not re-diagnose):**
`index` (`src/server.rs`) and `require_admin` both gate on `role_for(email) == "admin"`
— they are correct and consistent. The button shows because **`demo` has role `admin`
in the prod DB**: MoriNet's bootstrap creates the admin from the `ADMIN_EMAIL` env,
and on treklair that email is `demo`. The fix is the data/role change above (demo →
reader in `free`), which makes the button disappear — not a change to the nav logic.

## Testing conventions

- TDD strict, `cargo test` (135 tests green at handoff), clippy `-D warnings`, fmt.
- sqlite fixtures: `Store::connect_memory()`, bcrypt cost default (tests take ~15-25s, that's normal).
- e2e via `tower::ServiceExt::oneshot` on the Router — no network.
- UI verification: run `morinet serve` on :8099 with a throwaway config, then drive Chromium — click the real buttons, follow the real links, screenshot each page. Two unstyled-page bugs were only findable this way.
- API verification: `curl -X PROPFIND` against the running server (the roadmap also suggests a `cadaver` walkthrough). This is how the percent-encoded traversal hole was found — unit tests were green.
