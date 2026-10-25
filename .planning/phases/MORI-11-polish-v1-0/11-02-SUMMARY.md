# Summary — Plan 11-02 (MORI-11-polish-v1-0): README slim + ARCHITECTURE + ANNOUNCEMENT

**Status:** complete. Docs-only plan; no Rust files touched.

## What was done

### Task 1 — `docs/ARCHITECTURE.md` (commit `84b05f7`)
- New file carrying over from the README, essentially verbatim: "Architecture & decisions" stack bullets, "How it works" with the mermaid diagram, the two "details worth knowing" paragraphs (vault sources in `config.toml`; browser/WebDAV build-side split), and the full Credits list (Edgar Morin, Karpathy llm-wiki, FRAMES, Réseau Intelligence de la Complexité).
- Header states the doc was split out of the README at v1.0.0 and links back.
- Mermaid caption no longer marks MCP as future work ("lands in phase 10" removed; MCP node no longer dashed — it shipped in Phase 10). MCP bullet notes per-user API-token (Bearer) auth, verified against `src/mcp.rs`.
- Pointer lines to `.planning/STATE.md` (full AD log) and `.planning/HANDOFF.md` (operational view) instead of duplicating what HANDOFF owns (delivery pipeline, pitfalls, per-phase notes). HANDOFF has no Credits section — nothing duplicated.

### Task 2 — `README.md` rewritten (commit `9c17c16`)
- Exactly 5 level-2 sections in order: `## What`, `## Install`, `## Quick start`, `## Status`, `## License` (verified: `grep '^## '` → 5 lines).
- CI + Release badges removed (workflows retired 2026-10-25); License badge kept.
- What: one paragraph + refreshed feature bullets (multi-vault/groups/personal vaults, llm-wiki/Obsidian conversion, auth, WebDAV, MCP + API tokens, admin UI, built-in docs vault) + link to `docs/ARCHITECTURE.md`.
- Install: local-build reality — Rust stable + mdbook CLI on PATH (server shells out to `mdbook build`, fails loudly without it); (a) release asset `morinet-x86_64-unknown-linux-gnu.tar.xz` via `tar xJf`, (b) `git clone` + `cargo install --path .`; plus `cargo install --git …` documented as building `main`. Explicit "not published on crates.io" note; no cargo-dist / retired-workflow references.
- Quick start: interactive `morinet init` (port, data dir, admin email, masked admin password; cookie_secret auto-generated) matching the 11-01 prompts in `src/init.rs` / `src/main.rs`; `--yes` mentioned; `morinet serve --config ./config.toml`; login at http://localhost:8080. No manual `openssl rand` / cookie_secret editing.
- Status: "v1.0.0 stable" + one-line feature list, live demo https://morinet.treklair.fr (credentials not in repo), pointers to ROADMAP + REQUIREMENTS. Old phase table deleted. No host paths / SSH / deploy details.

### Task 3 — `.planning/phases/MORI-11-polish-v1-0/ANNOUNCEMENT.md` (commit `268eab0`)
- Short first-person v1.0.0 draft (~110 words): one-sentence what, headline features, demo URL, repo URL, `cargo install --git` one-liner.
- HTML-comment reminder at top: post only when the release is live (gated on plan 11-03). Agent never posts it — delivery is the file.

## Verification
- `grep '^## ' README.md` → exactly the 5 required headings in order.
- No `cargo-dist`, no `actions/workflows` badge URLs, no crates.io publish claim, no `openssl rand`/`cookie_secret` editing step in README.
- README mentions mdbook CLI prerequisite and the `morinet-x86_64-unknown-linux-gnu.tar.xz` asset name; Status contains "v1.0.0" and the demo URL.
- ANNOUNCEMENT contains "v1.0.0", demo URL, repo URL, install line; no credentials/host paths.
- `cargo fmt --check` clean; `cargo test --lib` → `test result: ok. 270 passed; 0 failed; 4 ignored` (docs-only change, run to confirm nothing else broke).

## Commits (not pushed)
- `84b05f7` docs: move architecture and credits to docs/ARCHITECTURE.md (11-02 Task 1)
- `9c17c16` docs: slim README to 5 sections reflecting local-build reality (11-02 Task 2)
- `268eab0` docs: draft v1.0.0 announcement for the user to post (11-02 Task 3)
- `(this)` docs(11): 02 summary

## Deviations / notes
- Cargo.toml version guard is still **0.1.21**; the README Status says "v1.0.0 stable" per the plan's acceptance criteria — plan 11-03 performs the actual version bump and release, so the claim becomes true at ship time.
- `docs/` already existed (contains `morinet-docs` seed source); `ARCHITECTURE.md` added alongside, nothing overwritten.
- MCP bullet in ARCHITECTURE adds "per-user API tokens (Bearer)" — verified in `src/mcp.rs`, keeps the doc current with Phase 10.
