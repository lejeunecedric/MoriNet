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
| Admin UI: invite users (temp password shown once), add/delete vaults (local+github), grant/revoke ACL | `/admin/*` (gate: role=admin) |
| CI → Release (cargo-dist, 16 assets, 5 platforms) → Deploy → restart + healthcheck | every `git push origin v0.1.x` |
| Styled UI (embedded CSS, dark mode) | `/assets/style.css` |

**Deploy pipeline** (`.github/workflows/`): `ci.yml` (fmt+clippy+test+build), `release.yml` (cargo-dist — **NEVER hand-edit it**, drift check), `deploy.yml` (staging scp + atomic mv + `sudo systemctl restart morinet` + `/health`).

## Current state at handoff time

- Latest release: **v0.1.6** (embedded stylesheet, admin nav on index). If holldemo still serves an older build, re-run the deploy: GitHub → Actions → Deploy → Run workflow → tag `v0.1.6`.
- The deploy auto-triggers on tag push AND on release published, but **re-dispatch may be needed right after a release finishes** (the tag-push deploy can race ahead of the release and fall back to "latest release"). Watch for that.
- holldemo has **no vaults configured yet** — add one via `/admin/vaults` (kind `github` needs a repo with `book.toml` + `src/SUMMARY.md`).

## Vault contract (IMPORTANT)

A vault is a **mdBook-native repo**: `book.toml` + `src/SUMMARY.md` at the root. `build()` runs `mdbook build` and serves `<src>/book`.

An import of *raw* Obsidian wikis (B3Wiki/ProgWiki, 128 .md without book.toml) was validated and **deliberately reverted** (commit `2b924f3`) — it was a format-validation test, not a feature. If the user asks for raw-wiki import again, build it as a proper feature (decisions: `[[wikilinks]]` handling, frontmatter, scaffold regeneration on sync) — do not re-add the hack silently.

## Known pitfalls (all hit, all documented)

1. **cargo-dist drift check**: NEVER hand-edit `.github/workflows/release.yml` — CI fails. Generate via `~/.cargo/bin/dist` (installed as `dist`, not a cargo subcommand). Put custom logic in separate workflows.
2. **Tag must equal package version**: `dist` refuses to release otherwise. `src/lib.rs` has a test guard (`version_matches_cargo_pkg_version`) — bump both on every release.
3. **ETXTBSY**: never scp/overwrite a running binary — deploy uses `/opt/morinet/staging/` + atomic `mv`.
4. **Deploy can fall back to "latest release"**: if a tag-push deploy races the release, it deploys the previous version. Re-dispatch with `inputs.tag=<tag>` after the release completes.
5. **Env races in tests**: config tests mutate process env — they hold a static `ENV_LOCK` mutex. Keep that pattern when adding env-dependent tests.
6. **git2 needs openssl**: build host requires `libssl-dev` + `pkg-config` (or use `vendored-openssl` feature).
7. **axum 0.8**: path syntax is `/{param}` and `{*wildcard}`; `HeaderMap::from_iter` needs `HeaderValue` (`.parse().unwrap()`); handlers extracting `Extension<Claims>` need `Claims: Clone`.

## What's left (ROADMAP)

- **Phase 5** — email: SMTP adapter (`lettre`), invitations + password reset (the temp-password-once flow is the interim).
- **Phase 6** — WebDAV on `/webdav/<slug>/` (PROPFIND/GET, admin PUT).
- **Phase 7** — MCP: StreamableHttp + `list_vaults`/`list_notes`/`read_note`/`search_notes` (crate `rmcp`).
- **Phase 8** — polish: crates.io publish, README final, v1.0.0.

Ops open items: vops owns holldemo provisioning (systemd unit with env creds, nginx, TLS, sudoers `/etc/sudoers.d/morinet-deploy` — verify it's installed, it gates the auto-restart).

## Testing conventions

- TDD strict, `cargo test` (54 tests green at handoff), clippy `-D warnings`, fmt.
- sqlite fixtures: `Store::connect_memory()`, bcrypt cost default (tests take ~15-25s, that's normal).
- e2e via `tower::ServiceExt::oneshot` on the Router — no network.
