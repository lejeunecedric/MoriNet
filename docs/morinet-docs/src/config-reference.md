# Config reference

MoriNet reads a TOML file passed with `--config` (default `./config.toml`).
`morinet init` writes a commented example. Every section is optional unless
marked required; missing values fall back to the defaults below.

## [server]

| Key | Default | Meaning |
| --- | --- | --- |
| `bind_addr` | `127.0.0.1:8080` | Address and port the HTTP server listens on. Use `0.0.0.0:8080` to expose it. |
| `base_url` | `http://127.0.0.1:8080` | The public origin users reach (e.g. `https://notes.example.com`). Redirects and the session cookie are built from it — behind a TLS proxy it must be the `https://` URL, not the bind address. |

## [storage]

| Key | Default | Meaning |
| --- | --- | --- |
| `data_dir` | `./data` | Data directory: the SQLite database (`morinet.db`), cloned GitHub repos, uploaded personal vaults, and the seeded docs vault all live under it. |

## [session]

| Key | Default | Meaning |
| --- | --- | --- |
| `cookie_secret` | *(required)* | HMAC secret for the session cookie. Generate with `openssl rand -hex 32`. Can be overridden by the `MORINET_COOKIE_SECRET` environment variable, which wins. |
| `cookie_name` | `morinet_session` | Name of the session cookie. |

## [smtp]

Optional. An absent or empty section disables all outbound email (invitations,
password resets) — the app stays usable, the admin does those steps manually.

| Key | Meaning |
| --- | --- |
| `host`, `port` | SMTP server, e.g. `smtp.example.com` port `587`. |
| `user`, `pass` | Login credentials (leave empty for a local relay). |
| `from` | Sender address for outgoing mail. |
| `starttls` | `true` to upgrade the connection with STARTTLS. |

## [vaults]

| Key | Default | Meaning |
| --- | --- | --- |
| `sync_interval_secs` | `300` | How often GitHub vaults are re-cloned and rebuilt (minimum 10). |

Each vault is a `[[vaults.entries]]` block:

```toml
[[vaults.entries]]
slug = "handbook"          # URL segment: /vault/handbook/
name = "Team Handbook"     # display name
kind = "github"            # "local" or "github"
path = "./src/handbook"    # local only: mdBook or Obsidian source dir
url = "https://github.com/your-org/handbook"  # github only
token = "ghp_..."          # github only: optional token for private repos
```

`kind = "local"` points at a folder containing a mdBook source (`book.toml` +
`src/SUMMARY.md`) or an Obsidian / llm-wiki vault — both are detected and
built automatically.

## Environment variables

| Variable | Meaning |
| --- | --- |
| `MORINET_COOKIE_SECRET` | Overrides `[session].cookie_secret`. |
| `MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD` | Bootstrap admin, created or updated on every boot. Required to start the server. |

Because the environment wins over the file, you can keep secrets out of
`config.toml` entirely. See also the [Quick start](quickstart.md) and the
[FAQ](faq.md).
