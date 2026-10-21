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

- Latest release: **v0.1.6** (embedded stylesheet, admin nav on index). If holldemo still serves an older build, re-run the deploy: GitHub → Actions → Deploy → Run workflow → tag `v0.1.6`.
- The deploy auto-triggers on tag push AND on release published, but **re-dispatch may be needed right after a release finishes** (the tag-push deploy can race ahead of the release and fall back to "latest release"). Watch for that.
- holldemo has **no vaults configured yet** — add one via `/admin/vaults` (kind `github` needs a repo with `book.toml` + `src/SUMMARY.md`).

## Vault contract (IMPORTANT)

A vault is a **mdBook-native repo**: `book.toml` + `src/SUMMARY.md` at the root. `build()` runs `mdbook build` and serves `<src>/book`.

An import of *raw* Obsidian wikis (B3Wiki/ProgWiki, 128 .md without book.toml) was validated and **deliberately reverted** (commit `2b924f3`) — it was a format-validation test, not a feature. If the user asks for raw-wiki import again, build it as a proper feature (decisions: `[[wikilinks]]` handling, frontmatter, scaffold regeneration on sync) — do not re-add the hack silently.

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

## What's left (ROADMAP)

- **Phase 7** — vault-ux: Browse button + server-side directory picker for local vaults; URL field for GitHub. (inserted 2026-10-21)
- **Phase 8** — fusion: per-user merged mdbook (sidebar grouped by vault). Biggest change so far — the current `vaults.book_path` model (one compiled book per vault, served at `/vault/<slug>/`) does not fit it.
- **Phase 9** — MCP: StreamableHttp + `list_vaults`/`list_notes`/`read_note`/`search_notes` (crate `rmcp`).
- **Phase 10** — polish: crates.io publish, README final, v1.0.0.

Ops open items: vops owns holldemo provisioning (systemd unit with env creds, nginx, TLS, sudoers `/etc/sudoers.d/morinet-deploy` — verify it's installed, it gates the auto-restart).

## Testing conventions

- TDD strict, `cargo test` (102 tests green at handoff), clippy `-D warnings`, fmt.
- sqlite fixtures: `Store::connect_memory()`, bcrypt cost default (tests take ~15-25s, that's normal).
- e2e via `tower::ServiceExt::oneshot` on the Router — no network.
- UI verification: run `morinet serve` on :8099 with a throwaway config, then drive Chromium — click the real buttons, follow the real links, screenshot each page. Two unstyled-page bugs were only findable this way.
- API verification: `curl -X PROPFIND` against the running server (the roadmap also suggests a `cadaver` walkthrough). This is how the percent-encoded traversal hole was found — unit tests were green.
