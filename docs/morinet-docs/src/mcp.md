# MCP

MoriNet exposes your vaults to AI agents (Claude, IDE assistants, or any
MCP client) through the **Model Context Protocol** over a StreamableHttp
endpoint.

## Endpoint

```text
https://your-morinet.example.com/mcp
```

## Authentication

MCP does not use your session cookie. Create a dedicated API token:

1. Open **Account** (`/account`) in the web UI.
2. Create a token — the secret is shown **once**; copy it now.
3. Tokens are individually revocable from the same page.

Send the token as a bearer header on every request:

```text
Authorization: Bearer <your-token>
```

A token acts as you: it can only reach the vaults your account can access, and
sharing a vault away or revoking the token immediately changes what the agent
sees.

## Tools

The endpoint offers four tools:

| Tool | What it does |
| --- | --- |
| `list_vaults` | Lists the vaults this token's account can access, with slugs and names. |
| `list_notes` | Lists the notes (pages) inside one vault, given its slug. |
| `read_note` | Returns the content of one note from a vault. |
| `search_notes` | Full-text search across a vault's notes. |

Typical agent flow: `list_vaults` → pick a slug → `search_notes` or
`list_notes` → `read_note` on the hits.

## Example session

```bash
curl -s https://your-morinet.example.com/mcp \
  -H "Authorization: Bearer $MORINET_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

Most MCP clients only need the endpoint URL and the bearer token in their
server configuration.

## Access model

MCP respects exactly the same permissions as the web UI: reader access lets an
agent read, and reads are recorded in the vault's statistics like any other
read. If a tool call comes
back empty or refused, the token's account likely lacks access to that vault.
