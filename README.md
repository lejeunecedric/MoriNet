# MoriNet

> Self-hosted mdBook server that exposes llm-wiki / Obsidian vaults as shared wiki sites and portable [OKF bundles](https://github.com/GoogleCloudPlatform/open-knowledge-format), with multi-user auth, WebDAV, and MCP.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

## What

MoriNet hosts one or more **llm-wiki / Obsidian vaults** — plain markdown with `[[wikilinks]]` and YAML frontmatter — converts them to mdBook sites, and serves them to your team or your AI agents. Every vault is also a native **[OKF (Open Knowledge Format)](#okf-bundle-support)** bundle — detected, exportable, and served with concept metadata to agents. Everything runs in a single Rust binary.

- 📦 **OKF-native vault host**: detects [Open Knowledge Format](https://github.com/GoogleCloudPlatform/open-knowledge-format) bundles with a conformance badge, exports any vault as a portable OKF bundle (zip), and serves concept metadata (`type`/`tags`/`status`/trust tier) to agents via API + MCP
- 📚 **llm-wiki / Obsidian conversion**: wikilinks, frontmatter, and auto-generated `SUMMARY.md` — no `book.toml` hand-editing
- 📂 **Multi-vault** with groups, per-group quotas, and personal vaults — load your own from GitHub **or upload a local folder** from the browser
- 🎛️ **Per-vault display options**: keep/hide sidebar page numbers, mdBook theme select (light/navy/rust/coal/ayu)
- 📈 **Per-vault statistics** including a **Wiki changes** log: what each Refresh added/modified/removed
- 🔒 **Locked accounts** (e.g. a public demo login) can read but never add or modify vaults
- 🔐 **Email + password auth** (HttpOnly signed cookies), `admin` / `reader` roles, invitations and password reset by email
- 📡 **WebDAV** access — put a note, it goes live
- 🤖 **MCP server** (StreamableHttp) with per-user **API tokens** — for AI agents
- 🌐 **Read-only REST API** on `/api/v1` (same API tokens, ACL parity with MCP, ETag/304 caching, RFC 9457 errors) — browsable docs at `/api/v1/docs`
- 🛡️ **Conflict-safe co-editing**: strong ETags + `If-Match`/`If-None-Match` on WebDAV, upload conflict detection with explicit force-overwrite (audited)
- 🔑 **Generic OIDC SSO** (Keycloak / Authentik / Authelia): authorization-code + PKCE, verified-email account linking, domain-gated auto-provisioning, group-claim sync — coexists with password login
- 🎛️ **Admin UI**: invite users, manage vaults, grants, and groups
- 🏠 Built-in **docs vault** served out of the box

<img width="1139" height="554" alt="image" src="https://github.com/user-attachments/assets/910c7110-1ee7-4eac-b4ce-71f0cb9070e5" />

MoriNet Documentation as a Vault in MoriNet

<img width="759" height="648" alt="image" src="https://github.com/user-attachments/assets/78860577-55b9-49d0-9d5c-d86492f98fa6" />

Main menu


Architecture and design decisions: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## OKF bundle support

**OKF — Open Knowledge Format** is an open, vendor-neutral spec ([Google Cloud](https://github.com/GoogleCloudPlatform/open-knowledge-format), currently v0.2) that formalizes the llm-wiki / Obsidian pattern MoriNet already hosts. A **bundle** is a directory of markdown files — one **concept** per file, YAML frontmatter + free-form body, standard markdown links forming a traversable graph. "If you can `cat` a file, you can read OKF; if you can `git clone` a repo, you can ship it."

The hard rules are deliberately small:

- **Required frontmatter:** `type` only — a concept with just `type` is already conformant. Recommended: `title`, `description`, `resource`, `tags`.
- **Reserved filenames** at any level: `index.md` (directory listing for progressive disclosure) and `log.md` (dated changelog with `## YYYY-MM-DD` headings).
- **v0.2 trust signals** in frontmatter: `sources:` provenance entries, `generated: {by, at}` / `verified: [{by, at}]` attribution, `stale_after` freshness, `status: draft | stable | deprecated`. Consumers derive **trust tiers** from them: no `verified` → *unverified*; machine-only → *machine-confirmed*; a `human:<id>` verifier → *human-reviewed*.
- **Permissive conformance:** unknown types, keys and broken links must be tolerated — a bundle is never rejected for missing optionals.

MoriNet is an OKF-native host:

- 🔎 **Conformance detection** — vault sources are scanned at refresh/register; the vault page shows an `OKF v0.2 — N/M concepts` badge (or "not an OKF bundle").
- 📦 **Bundle export** — "Export OKF bundle" streams a zip of the vault source, files byte-identical, synthesizing a root `index.md` with `okf_version: "0.2"` when absent.
- 🤖 **Agent-ready metadata** — `/api/v1` note list/read accept `?meta=1` to return parsed frontmatter (`type`, `tags`, `status`, derived trust tier), and MCP `list_notes` / `search_notes` accept optional `type` / `tags` filters — agents consume your vault as a knowledge graph, not a pile of raw files.

The human-facing mdBook rendering is untouched: the converter still strips frontmatter from the rendered pages.

Spec, quickstart and community ecosystem map: [okf.md](https://okf.md).

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

**v1.4.0 stable.** v1.3 "OKF Bundle Support" shipped: OKF conformance detection with a vault-page badge, one-click "Export OKF bundle" (zip, byte-identical sources), and concept metadata on `/api/v1` (`?meta=1`) plus `type`/`tags` filters on MCP `list_notes`/`search_notes` — see [OKF bundle support](#okf-bundle-support).

Earlier: v1.2 "Team Access" shipped: conflict-safe co-editing (strong ETags, conditional WebDAV writes, upload conflict list + force-overwrite audit), a read-only REST API on `/api/v1` (Bearer API tokens, cursor pagination, search, ETag/304, per-token rate limiting), and generic OIDC SSO (auth-code + PKCE against any compliant IdP, verified-email linking, domain-gated reader auto-provisioning, replace-on-login group sync) — fully coexisting with password login, bootstrap admin always available as safety net.

Earlier: v1.1.0 added per-vault display options, the wiki-changes log, and locked-account guards on top of v1.0: multi-vault hosting with groups and personal vaults (GitHub load + local folder upload), llm-wiki/Obsidian conversion, admin UI, email invites and reset, WebDAV, MCP server with API tokens, and a built-in docs vault.

**Live demo**: [morinet.treklair.fr](https://morinet.treklair.fr) — public demo instance, no real data; credentials are not in this repo.

Roadmap: [`.planning/ROADMAP.md`](.planning/ROADMAP.md) · Shipped requirements: [`.planning/milestones/`](.planning/milestones/)

## License

MIT OR Apache-2.0 — see [LICENSE](LICENSE).
