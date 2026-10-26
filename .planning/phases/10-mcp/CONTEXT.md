# CONTEXT — Phase 10: mcp

## Locked decisions (discuss 2026-10-25)

- **Auth: dedicated API token** (user decision). The user generates tokens on `/account`
  (name + plaintext value shown ONCE), revocable. Stored **sha256-hashed** (same
  convention as `invitations`/`reset_tokens`, 0003_tokens.sql). MCP NEVER uses
  the password or the session cookie.
- **Transport: StreamableHttp** on `/mcp` via `rmcp` 3.7 (`transport-streamable-http-server`),
  stateless mode + `with_json_response(true)` — a `StreamableHttpService` is a Tower
  service, mounted with `nest_service("/mcp", …)` on the existing axum router.
- **Tools scope** (ROADMAP) : `list_vaults`, `list_notes(slug, path?)`,
  `read_note(slug, path)`, `search_notes(query, slug?)`. Resource template `vault://<slug>/…`
  optional — outside the criteria, to do if time allows.
- **ACL identical to WebDAV** : the tools only see `list_vaults_for(email)` /
  `acl_allows(email, slug)` ; a guessed slug → tool error, no existence leak.
- **Read from the vault SOURCE** (markdown, not the compiled `book/`) — same choice as
  WebDAV (AD-13) ; paths are validated by a safe_join (percent-decode BEFORE the
  split — Phase 6 pitfall).

## Constraints

- `cargo clippy -D warnings` + fmt + green suite (197 tests) on every commit.
- Strict TDD : failing test first, in vertical slices.
- Do not touch the CSS/design (frozen at v0.1.20).
