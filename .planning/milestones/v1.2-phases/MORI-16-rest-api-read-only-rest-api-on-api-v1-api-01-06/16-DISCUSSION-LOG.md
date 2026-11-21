# Phase 16: rest-api - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-30
**Phase:** 16-rest-api — Read-only REST API on `/api/v1` (API-01..06)
**Areas discussed:** Response shapes & pagination, Rate-limit policy, Caching, API discoverability

Mode note: the interactive form timed out once; the workflow's plain-text fallback was used and the user answered "all a except 8b" in one reply.

---

## Response shapes & pagination

| Option | Description | Selected |
|--------|-------------|----------|
| Bare arrays/objects matching MCP field names | `[{slug,name,scope,role}]`, no envelope | ✓ |
| Envelope `{"data":[...],"meta":{...}}` | on every response | |
| Envelope only where pagination metadata is needed | notes list only | |

**User's choice:** 1a — bare arrays/objects matching MCP field names.

| Option | Description | Selected |
|--------|-------------|----------|
| Opaque base64 cursor + has_more + next_cursor | `?limit=` default 50, cap 200 | ✓ |
| Visible cursor `?after=<path>` | plain last-path param | |
| Offset `?offset=&limit=` | simpler, unstable under edits | |

**User's choice:** 2a — opaque base64 cursor.

| Option | Description | Selected |
|--------|-------------|----------|
| Reuse MCP search_md hit shape, top 50, no pagination | mirrors MCP cap | ✓ |
| Same hits, cursor-paginated | like notes | |
| Richer hits | snippet + etag + last-modified | |

**User's choice:** 3a — MCP hit shape, top 50.

| Option | Description | Selected |
|--------|-------------|----------|
| Raw markdown `text/markdown` + ETag header | curl-friendly | ✓ |
| JSON `{"path","content","etag"}` | wrapped | |
| Content negotiation | text default, JSON on Accept | |

**User's choice:** 4a — raw text/markdown + ETag.

---

## Rate-limit policy

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed window 120 req/min per token, in-process | no new deps | ✓ |
| Token bucket | burst allowance | |
| Per-endpoint budgets | search stricter | |

**User's choice:** 5a — fixed window 120/min.

| Option | Description | Selected |
|--------|-------------|----------|
| All /api/v1 endpoints count equally | | ✓ |
| Exclude discovery root from counting | | |

**User's choice:** 6a — all endpoints count equally.

---

## Caching

| Option | Description | Selected |
|--------|-------------|----------|
| If-None-Match → 304 (reuse Phase 15 parser) | free win | ✓ |
| ETag header only in v1 | 304 later | |

**User's choice:** 7a — 304 support via `if_none_match_of`.

---

## API discoverability

| Option | Description | Selected |
|--------|-------------|----------|
| JSON index at /api/v1 root only | no docs page | |
| JSON index + human-readable /api/v1/docs page in app shell | | ✓ |
| README/docs-vault documentation only | no endpoint surface | |

**User's choice:** 8b — JSON root index **plus** a `/api/v1/docs` page in the `app_page()` shell.

---

## Claude's Discretion

- Docs page public vs Bearer-gated (recommend public static HTML — no vault data)
- Cursor encoding internals, rate-limiter data structure (dependency-free), problem+json `type`/`code` vocabulary, `{path}` nested-slash route style

## Deferred Ideas

- API-F01 write endpoints, API-F02 scoped tokens, API-F03 webhooks (already Future Requirements)
- Issue #10 web+MCP stats (backlog; rate-limiter counters may later feed it)
