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

## [security]

Optional. Guards accounts that must never modify vaults (e.g. a public demo
login). A locked account is refused on load, upload, delete, edit and share
mutations — reads and Refresh stay allowed. Admins are never locked.

| Key | Default | Meaning |
| --- | --- | --- |
| `locked_accounts` | `[]` | List of account emails that cannot add or modify vaults. Can be overridden by the `MORINET_LOCKED_ACCOUNTS` environment variable (comma-separated), which wins when set to a non-blank value. |

## [oidc]

Optional. An absent or incomplete section disables SSO entirely — password
login is never affected. The section is enabled only when `issuer`,
`client_id` and `client_secret` ALL resolve to non-blank values (from the
file or the environment below); any partial configuration fails closed and
the `/sso/login` and `/sso/callback` routes answer 404.

| Key | Default | Meaning |
| --- | --- | --- |
| `issuer` | — | The IdP's issuer URL, e.g. `https://id.example.com`. Must match the `iss` claim exactly. |
| `client_id` | — | OAuth client id registered at the IdP. Must match the token `aud` claim. |
| `client_secret` | — | Client secret for the code exchange. Keep it out of the file with the env var below. |
| `allowed_email_domains` | `[]` | Exact domains allowed for auto-provisioning new SSO accounts (e.g. `["corp.example"]` — no subdomain matching). An empty list disables provisioning: only pre-linked accounts can sign in. |
| `groups_claim` | `"groups"` | Name of the groups claim read from the **signature-verified ID token** — claims in the access token are never consulted. |
| `group_map` | `{}` | Table mapping an IdP group name to a local group name, e.g. `{ "dev" = "writers" }`. Only mapped groups are managed; unmapped claims change nothing. |
| `sso_label` | `"SSO"` | Label for the SSO button on the login page. |

| Environment variable | Overrides |
| --- | --- |
| `MORINET_OIDC_ISSUER` | `[oidc].issuer` |
| `MORINET_OIDC_CLIENT_ID` | `[oidc].client_id` |
| `MORINET_OIDC_CLIENT_SECRET` | `[oidc].client_secret` |
| `MORINET_OIDC_ALLOWED_DOMAINS` | `[oidc].allowed_email_domains` (comma-separated) |

The environment wins over the file; a **blank** env value counts as unset
(it does not clear a file value).

Rules that fail closed:

- Partial config (any of `issuer` / `client_id` / `client_secret` missing or
  blank) ⇒ SSO disabled, routes 404, login page unchanged.
- Provisioning requires an **exact** domain match against
  `allowed_email_domains` and a `email_verified: true` claim; an unverified
  or unmatched email is refused (403) and audited.
- An IdP account whose email matches an existing **admin** account is never
  auto-linked — the sign-in is refused and audited. Admins link SSO
  identities only through an admin/DB operation.
- Once an identity is linked, the email claim is inert: the `(issuer, sub)`
  pair is the identity, so the IdP changing a user's email never moves or
  re-links the local account.
- SSO emails are lowercased before lookup/linking. This diverges from
  password login, which matches stored emails case-sensitively.
- Group sync is replace-on-login: an **absent** groups claim ⇒ local managed
  groups are kept untouched; a **present-but-empty** claim ⇒ the user is
  removed from the managed set only (groups named in `group_map` values);
  every add and removal is logged to `auth_events`. The last remaining admin
  is never demoted by group sync.
- `auth_events` rows are retained for 90 days, then pruned automatically.
- There is no self-service unlink: detaching an SSO identity is an admin/DB
  operation.

## Environment variables

| Variable | Meaning |
| --- | --- |
| `MORINET_COOKIE_SECRET` | Overrides `[session].cookie_secret`. |
| `MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD` | Bootstrap admin, created or updated on every boot. Required to start the server. |
| `MORINET_LOCKED_ACCOUNTS` | Comma-separated emails, overrides `[security].locked_accounts` when non-blank. |

Because the environment wins over the file, you can keep secrets out of
`config.toml` entirely. See also the [Quick start](quickstart.md) and the
[FAQ](faq.md).
