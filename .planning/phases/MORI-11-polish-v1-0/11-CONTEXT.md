# Phase 11: polish (v1.0) - Context

**Gathered:** 2026-10-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Ship v1.0.0: the repo is installable, documented, and released with zero GitHub-side
compilation. Deliverables (from ROADMAP, updated for the local-build migration of
2026-10-25):
- `cargo install` path works end-to-end (README instructions verified by actually running them)
- Minimal/elegant README — exactly 5 sections: What / Install / Quick start / Status / License
- `morinet init` becomes interactive (asks questions, writes config.toml)
- Cargo.lock committed (ALREADY DONE — verify only)
- Tag `v1.0.0` + GitHub release created via `scripts/release.sh` (NOT cargo-dist — retired)
- Announcement text drafted for the user to post on their channel

Out of scope (ROADMAP "Out of scope (v1)"): collab editing, mobile apps, SSO,
webhooks/public API, encryption at rest.
</domain>

<decisions>
## Implementation Decisions

### `morinet init` (interactive)
- Interactive prompts: **port**, **data_dir**, **admin email**, **admin password** (masked input via `rpassword` crate).
- Defaults offered on each prompt (current `default_config()` values: port 8080, data_dir ./data, etc.) — Enter accepts the default.
- `cookie_secret`: generated automatically (random 32-byte hex) and written into the config — the user is NOT asked.
- Non-interactive fallback: **`--yes` flag** keeps today's behavior (writes `default_config()` verbatim, no prompts) — usable in scripts/CI. Existing `--out` and `--force` flags preserved.
- Written config must remain a valid, commented `config.toml` (parses with the existing config loader; keep the `default_config_contains_required_sections`/`parses_as_valid_toml` test guarantees).
- Admin password in config: stored as the plaintext bootstrap value in config.toml (same env-var semantics the server already uses: server bootstraps admin from config/env only when users table is empty). Comment in the generated file must say so.

### README (minimal, English)
- Language: **English** (public international project).
- Exactly 5 sections: `## What`, `## Install`, `## Quick start`, `## Status`, `## License`.
- Current 140-line README's `Architecture & decisions` and `Credits` content: move to `docs/ARCHITECTURE.md` (and keep Credits there or drop if duplicated in .planning/HANDOFF.md) — README itself must stay lean.
- Install section must reflect reality post-cargo-dist: build from source with cargo (mdbook CLI dependency noted), NOT `cargo install` via cargo-dist assets. If `cargo install --git` works, document it; verify by running.
- Quick start uses the new interactive `morinet init` flow.
- Status section: v1.0.0, feature list one-liners (multi-vault, WebDAV, MCP server, API tokens, admin UI), link to live demo https://morinet.treklair.fr.

### Release v1.0.0
- `scripts/release.sh v1.0.0` is the release path (local build → GitHub release with `morinet-x86_64-unknown-linux-gnu.tar.xz` asset). **GITHUB_TOKEN will be provided by the user at release time** — the plan must treat the token as a human-action gate at the end, not a blocker for the other work.
- Version guard `src/lib.rs::version_matches_cargo_pkg_version` must be bumped to "1.0.0" alongside Cargo.toml (test deliberately fails otherwise).
- Post-release: deploy to demo via `DEPLOY_HOST=<see .planning/DEPLOY-LOCAL.md, gitignored> scripts/deploy.sh`.

### Announcement
- The agent DRAFTS the announcement text (short, for the user's channel); the USER posts it. Deliver as a file (e.g. `.planning/phases/MORI-11-polish-v1-0/ANNOUNCEMENT.md`), not a posted message.

### Already done (verify only, do not re-implement)
- Cargo.lock committed ✓
- `morinet init` exists (non-interactive) — this phase ENHANCES it
- Local delivery pipeline scripts/check.sh + release.sh + deploy.sh ✓ (Phase 10 delivery work)
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/ROADMAP.md` — Phase 11 criteria (note: cargo-dist mention is obsolete, replaced by scripts/release.sh per this CONTEXT)
- `.planning/REQUIREMENTS.md` — REQ-INFRA-002/003 now point at scripts/check.sh + local release
- `.planning/HANDOFF.md` — repo conventions, pitfalls list (esp. version guard, mdbook build-time themes)
- `scripts/release.sh` — the actual release path (asset names matter for deploy.yml)
- `src/main.rs` — existing `Command::Init` to enhance
- `src/lib.rs` — `default_config()` + version guard test
- `README.md` — current 8-section version to slim down
- No external specs — decisions above are complete.
</canonical_refs>

<specifics>
## Specific Ideas

- Interactive prompts should feel like `cargo new`-style minimalism: one line per question, defaults in parentheses, e.g. `Port [8080]:`.
- README "What" = one tight paragraph + feature bullets; no marketing fluff.
</specifics>

<deferred>
## Deferred Ideas

- Publish on crates.io (not requested; cargo-install-from-git is enough for v1.0)
- Announcement posting itself = user action (agent only drafts)
</deferred>

---

*Phase: MORI-11-polish-v1-0*
*Context gathered: 2026-10-25 via /gsd:discuss-phase*
