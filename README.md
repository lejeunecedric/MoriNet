# MoriNet

> Self-hosted mdBook server that exposes llm-wiki / Obsidian vaults as shared wiki sites, with multi-user auth, WebDAV, and MCP.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

## What

MoriNet hosts one or more **llm-wiki / Obsidian vaults** — plain markdown with `[[wikilinks]]` and YAML frontmatter — converts them to mdBook sites, and serves them to your team or your AI agents. Everything runs in a single Rust binary.

- 📚 **llm-wiki / Obsidian conversion**: wikilinks, frontmatter, and auto-generated `SUMMARY.md` — no `book.toml` hand-editing
- 📂 **Multi-vault** with groups, per-group quotas, and personal vaults (load your own from GitHub)
- 🔐 **Email + password auth** (HttpOnly signed cookies), `admin` / `reader` roles, invitations and password reset by email
- 📡 **WebDAV** access — put a note, it goes live
- 🤖 **MCP server** (StreamableHttp) with per-user **API tokens** — for AI agents
- 🎛️ **Admin UI**: invite users, manage vaults, grants, and groups
- 🏠 Built-in **docs vault** served out of the box

Architecture and design decisions: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Install

**Prerequisites:** Rust (stable) and the **mdbook CLI on your PATH** (`cargo install mdbook`). The server shells out to `mdbook build` to compile vaults and fails loudly if `mdbook` is missing.

**Prebuilt Linux binary** — grab the release asset [`morinet-x86_64-unknown-linux-gnu.tar.xz`](https://github.com/lejeunecedric/MoriNet/releases) and extract it:

```bash
tar xJf morinet-x86_64-unknown-linux-gnu.tar.xz
sudo mv morinet /usr/local/bin/
```

**Build from source:**

```bash
git clone https://github.com/lejeunecedric/MoriNet
cd MoriNet
cargo install --path .
```

You can also let cargo build the main branch directly:

```bash
cargo install --git https://github.com/lejeunecedric/MoriNet morinet
```

(Note: this builds the current `main`, not necessarily the latest release. MoriNet is not published on crates.io.)

## Quick start

```bash
# 1. Interactive setup — asks for port, data dir, admin email,
#    and admin password (masked). The cookie secret is generated for you.
morinet init

# Non-interactive (defaults + generated secret) for scripts:
#   morinet init --yes

# 2. Start the server
morinet serve --config ./config.toml

# 3. Log in at http://localhost:8080 with the admin account you just chose
```

## Status

**v1.0.0 stable.** Shipped: multi-vault hosting with groups and personal vaults, llm-wiki/Obsidian conversion, admin UI, email invites and reset, WebDAV, MCP server with API tokens, and a built-in docs vault.

**Live demo**: [morinet.treklair.fr](https://morinet.treklair.fr) — public demo instance, no real data; credentials are not in this repo.

Roadmap: [`.planning/ROADMAP.md`](.planning/ROADMAP.md) · Requirements catalog: [`.planning/REQUIREMENTS.md`](.planning/REQUIREMENTS.md)

## License

MIT OR Apache-2.0 — see [LICENSE](LICENSE).
