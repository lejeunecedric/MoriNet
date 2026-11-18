# Draft: `[OKF Tool] MoriNet` — issue to post on github.com/fabricioctelles/skills

Process: https://okf.md/contribute/ — title `[OKF Tool] Your Tool Name`, manual review, response in a few days.

## Title

[OKF Tool] MoriNet

## Body

**Repository:** https://github.com/lejeunecedric/MoriNet

**One-line description:** Self-hosted, single-binary Rust server that hosts llm-wiki / Obsidian vaults as shared wiki sites and native OKF bundles — conformance badge, one-click bundle export (zip), and concept metadata (`type`/`tags`/`status`/trust tier) served to AI agents over REST (`?meta=1`) and MCP (`type`/`tags` filters).

**Category:** Publisher / Server (self-hosted OKF-native host — closest gap on the map: there is no self-hosted open-source OKF server listed)

**Language / Platform:** Rust (static Linux binary; also builds anywhere Rust does)

**OKF version supported:** v0.2 (conformance scan per SPEC §11, trust tiers per §5.3, root `index.md` synthesis with `okf_version: "0.2"` on export)

**Notes for review:**
- Conformance detection runs at vault refresh/register; vault page shows an `OKF v0.2 — N/M concepts` badge
- "Export OKF bundle" streams byte-identical sources as a portable zip
- MCP server (StreamableHttp) + read-only REST API with per-user API tokens and ACL parity
- Live demo: https://morinet.treklair.fr · Docs: README "OKF bundle support" section
- MIT OR Apache-2.0

## GitHub topics to add on MoriNet (repo → About → Topics)

okf, open-knowledge-format, llm-wiki, mdbook, obsidian, webdav, mcp, rust, self-hosted, knowledge-base, wiki-server

## Blocked on (2026-11-18)

`~/.config/morinet-gh-token` is a fine-grained PAT scoped to MoriNet with read-only admin:
- `PUT /repos/lejeunecedric/MoriNet/topics` → 403 (needs Administration: write)
- `POST /repos/fabricioctelles/skills/issues` → 403 (token not scoped to other repos; needs Issues: write, or post manually)
