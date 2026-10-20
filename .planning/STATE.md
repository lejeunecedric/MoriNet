# MoriNet — STATE (live session state)

> Updated on every delivered bite / decision made / notable error.

---

## Current position (2026-10-20, Phase 4 done — handoff to next agent)

- **Latest release**: v0.1.6 (embedded stylesheet + admin nav on index) — releasing at handoff time; if holldemo lags behind, re-dispatch Deploy with `inputs.tag=v0.1.6`
- **Live instance**: https://morinet.treklair.fr — public demo instance, admin-ui accessible via the **Admin** nav link on the index. Credentials live on the server, not in this repo
- **Phases done**: 00 bootstrap, 01 core-serve, 02 multi-vault, 03 vault-sources, 04 admin-ui
- **54/54 tests** (44 → 54 after the Phase 5 token-layer test pass), clippy `-D warnings`, fmt clean — every push of a tag auto-releases and auto-deploys
- **Auto-scaffold of raw wikis REVERTED** (`2b924f3`): vault contract is mdBook-native (`book.toml` + `src/SUMMARY.md`). The B3Wiki/ProgWiki import was a format-validation test, not a feature — see HANDOFF.md before re-attempting
- **Deploy quirks**: staging + atomic `mv` (ETXTBSY), re-dispatch may be needed right after a release finishes (tag-push deploy races the release and falls back to "latest release")
- **Next**: Phase 5 (email via lettre) → Phase 6 (WebDAV) → Phase 7 (MCP) → Phase 8 (polish v1.0)
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

## Notable errors & learnings

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
