# Quick start

MoriNet is a self-hosted server for your notes. It takes mdBook projects and
Obsidian / llm-wiki vaults, compiles them with
[mdBook](https://rust-lang.github.io/mdBook/), and serves them to your team
over the web — with accounts, sharing, WebDAV and an MCP endpoint for AI
agents. This documentation vault itself is built and served by MoriNet.

## Install

MoriNet ships as a single Rust binary. Until the crates.io release, install it
from the repository:

```bash
cargo install --git https://github.com/lejeunecedric/MoriNet morinet
```

You also need the `mdbook` binary available to the **server** process — every
vault is compiled with `mdbook build` before it is served:

```bash
cargo install mdbook
```

## Generate and edit the config

```bash
morinet init --out ./config.toml
$EDITOR ./config.toml
```

The two values you must set are `cookie_secret` in `[session]` (generate one
with `openssl rand -hex 32`) and the admin bootstrap credentials, which are
read from the environment. See the [Config reference](config-reference.md) for
every option.

## Start the server

```bash
export MORINET_ADMIN_EMAIL="you@example.com"
export MORINET_ADMIN_PASSWORD="choose-a-strong-password"
morinet serve --config ./config.toml
```

Open the address from `[server].bind_addr` (by default
`http://127.0.0.1:8080`) and log in with the admin email and password you just
exported. The admin account is created on first boot; see
[Config reference](config-reference.md) for the environment variables.

## Add a vault from GitHub

Add an entry to `config.toml` and restart — MoriNet clones the repo, builds
the book, and re-syncs it in the background:

```toml
[[vaults.entries]]
slug = "team-handbook"
name = "Team Handbook"
kind = "github"
url = "https://github.com/your-org/team-handbook"
```

Local folders work the same way with `kind = "local"` and a `path`.

## Refresh semantics

Pages are compiled HTML: a vault is built once, and the served copy only
changes when it is rebuilt. Push to a GitHub vault is picked up automatically
by the background sync; for local vaults (or after editing files through
[WebDAV](webdav.md)), press **Refresh** on the vault card to rebuild it from
its source. Themes and assets are also applied at build time, so a theme
change needs a Refresh too.

Once logged in, share a vault with your teammates from the **Share** button —
see [Sharing & roles](sharing-and-roles.md).
