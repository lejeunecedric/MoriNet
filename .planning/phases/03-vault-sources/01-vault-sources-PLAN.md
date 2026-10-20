# Phase 3 — vault-sources PLAN

**Goal**: vault entries can come from a local folder OR a GitHub repo (clone + periodic sync), with mdbook rebuild after sync (REQ-VAULT-007..010).

**Design (AD-8)**: trait `VaultSource` with `sync() -> PathBuf` (the local book-source dir). Background tokio task re-syncs GitHub sources on an interval (default 300s, `[vaults].sync_interval_secs`). Auth for private repos: optional `token` field injected into the HTTPS URL (`https://x-access-token:<token>@github.com/...`) — documented tradeoff, no keychain in v1.

## Bites

- **T3.1** `src/sources.rs` — trait `VaultSource` + `LocalVaultSource` (no-op sync, validate dir exists) + unit tests
- **T3.2** `GithubVaultSource` — git2: clone if absent, else fetch + hard reset to origin; `file://` fixture tests (no network): clone, then new upstream commit → sync picks it up
- **T3.3** mdbook rebuild after sync: run `mdbook build <dir>` if the binary exists (else warn); book dir = `<dir>/book`
- **T3.4** config: `[[vaults.entries]]` gains `kind = "local" | "github"` (+ `url`, `branch`, `token` for github); serve() resolves entries → sources → sync → seed book_path; background sync task spawned
- **T3.5** fmt/clippy/test all green, commit, push

## Success criteria

- [x] trait VaultSource + Local + Github impls
- [x] offline git2 tests (clone + pull on file:// fixture)
- [x] mdbook build invoked after sync when available
- [x] periodic background sync in `morinet serve`
