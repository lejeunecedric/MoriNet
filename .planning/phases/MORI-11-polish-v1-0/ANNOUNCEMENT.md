<!-- DRAFT — do not post until the v1.0.0 release is live (gated on plan 11-03).
     The user posts this themselves; the agent never posts it. -->

**MoriNet v1.0.0 is out.**

MoriNet is a self-hosted server that turns your llm-wiki / Obsidian vaults into shared wiki sites — with multi-user auth, WebDAV, and an MCP server for AI agents.

Highlights:

- llm-wiki / Obsidian conversion: `[[wikilinks]]`, frontmatter, auto-generated summaries
- Multi-vault with groups, quotas, and personal vaults (pull your own from GitHub)
- WebDAV — drop a note in, it goes live
- MCP over StreamableHttp with per-user API tokens
- Admin UI, email invites and password reset
- Single Rust binary

Try the live demo: https://morinet.treklair.fr

Source: https://github.com/lejeunecedric/MoriNet

Install:

    cargo install --git https://github.com/lejeunecedric/MoriNet morinet
