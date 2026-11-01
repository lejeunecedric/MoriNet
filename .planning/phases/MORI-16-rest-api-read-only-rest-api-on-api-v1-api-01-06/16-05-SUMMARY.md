---
phase: 16
plan: 16-05
name: docs-page-and-acl-matrix-port
status: complete
one_liner: Public human-readable /api/v1/docs page in the app_page shell + the full MCP ACL test matrix ported 1:1 to the four REST endpoints (six api_* ports, all green).
requirements_completed: [API-05]
commits: [a1263c5, 45ef246]
completed: 2026-11-01
---

# 16-05: docs-page-and-acl-matrix-port — SUMMARY

## What was built

- **`src/server.rs`** —
  - `app_page` made `pub(crate)` (one-word visibility change at `:92`; no
    behavior change) so the docs handler in `api.rs` reuses the shell.
  - `GET /api/v1/docs` registered in the **public** router group
    (`build_router_with_email`, `:191`), outside the api group's
    `route_layer` — public static HTML, no Bearer, never counted against the
    per-token rate limiter. Registered as
    `get(|| async { crate::api::docs_page() })` because axum 0.8 requires an
    async handler; `docs_page` itself stays sync per the plan contract.
- **`src/api.rs`** —
  - `pub(crate) fn docs_page() -> Response`: fully static HTML body (intro,
    five-route table, Auth / Pagination / Errors / Rate-limit sections)
    rendered through `crate::server::app_page("REST API", &body)`. Zero
    interpolation of user-controlled data (no html_escape surface, threat
    B2/B4 satisfied), zero `<style>` tags (D-15 / Phase 9b rule — grep
    `-n "<style" src/api.rs` is empty).
  - Six ACL matrix ports in `mod tests`, named 1:1 after their MCP sources
    (16-PATTERNS §10 mapping), reusing the existing `api_app`/`share_app`/
    `mint_token`/`get` harness (fresh token per test = fresh rate bucket):
    `api_tools_are_acl_scoped`, `api_read_note_refuses_traversal_even_with_access`,
    `api_sharee_sees_shared_vault`, `api_hidden_share_invisible`,
    `api_revoked_share_denied`, `api_search_finds_notes_in_shared_vault`.
    Every invisible-vault assertion is 404 + `application/problem+json` +
    code `not_found` — never 403 (D-02/D-07).
  - A comment in the tests module records the three deliberately-skipped
    logging ports (`mcp_denied_call_logs_nothing`, `mcp_read_note_logs_access`,
    `mcp_list_and_search_log`) and the migrations/0008 CHECK-constraint reason
    (planner decision, 16-01) — the gap is visibly intentional.

## Tests (TDD)

- **RED-equivalent (matrix):** all six `api_*` port tests were written first
  and **passed on first run** — no endpoint code needed fixing. That IS the
  parity evidence: waves 16-02..16-04 were built to this matrix and the port
  confirms it at the REST level (D-07, ROADMAP criterion 7). No assertion was
  weakened.
- **RED → GREEN (docs):** `api_docs_page_public` failed first (401 — the
  gated `/api/v1/{*rest}` catch-all answered); after registering the public
  route it passes: 200 `text/html` without any Bearer header, body contains
  `/api/v1/vaults`, `Bearer`, `problem+json`, `Retry-After`, `120`.
  `api_docs_page_no_vault_data` asserts two sequential responses are
  byte-identical (fully static page).
- Targeted: `cargo test --lib api_` → **36 passed / 0 failed**.
- **Full suite**: `cargo test --lib` → **403 passed / 0 failed / 4 ignored**
  (baseline 395 + 8 new). MCP originals and REST ports green together.
- `cargo clippy --all-targets` clean; `cargo fmt --check` clean;
  `git diff Cargo.toml Cargo.lock` empty (D-05: zero new dependencies).

## Deviations

- One mechanical necessity: axum 0.8's `get()` requires an async handler, so
  the public route wraps the sync `docs_page()` in a `|| async { ... }`
  closure at registration. The plan's `docs_page()` signature is unchanged.

## Decisions

- Docs page kept in the **public** group per the locked planner decision;
  matchit resolves static `/api/v1/docs` ahead of `/api/v1/{*rest}` — proven
  by `api_docs_page_public` (public 200) while `api_unknown_path_is_problem_not_found`
  (16-02) still proves the catch-all gates at 401 without a token.
- The traversal port keeps the raw + `%2e%2e%2f` encoded variants on the
  note-read sub-path (the notes-LIST route takes no sub-path parameter in
  REST; the encoded-variant 404-not-403 assertion is preserved).
- Hidden/revoked ports assert the `not_found` code slug as the REST analog of
  the verbatim MCP string "vault not found or access denied".

## Lessons

- The `api_*` matrix ports are now the greppable parity contract: if anyone
  changes ACL behavior in `api.rs` helpers, both the MCP originals and these
  REST ports fail together — keep them 1:1 with `mcp.rs` sources.
- A sync `fn() -> Response` is NOT an axum 0.8 handler; wrap it in
  `|| async { ... }` at the route rather than making every page fn async.
- ROADMAP criteria now all have named green tests: 1-2 (16-02), 3-4 (16-03),
  5 (16-02/03/05), 6 (16-04), 7 (this plan) — phase 16 exit state reached.
