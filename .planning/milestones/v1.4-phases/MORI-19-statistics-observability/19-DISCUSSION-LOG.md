# Phase 19: statistics-observability - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-18
**Phase:** 19-statistics-observability
**Areas discussed:** MCP activity content, Search keywords, Visibility, Date range

---

## MCP activity content

| Option | Description | Selected |
|--------|-------------|----------|
| Rich row | Excerpt ~200 chars + size + latency + status; raw truncated args | ✓ |
| Sparing row | Status + size + latency only, no content excerpt, no args | |

**User's choice:** Rich row (excerpt ~200 car. + taille + latence + statut, args bruts tronqués)
**Notes:** Two clarify forms timed out before the user requested a simplified single form; all four final decisions took the recommended options. Scope default applied: detailed activity covers the 3 vault-scoped tools only (`list_vaults` keeps the coarse row).

---

## Search keywords

| Option | Description | Selected |
|--------|-------------|----------|
| Queries + words | Store raw query; display top queries AND top words (tokens ≥ 3 chars), MCP + REST | ✓ |
| Queries only | Top raw queries only | |

**User's choice:** Queries + words
**Notes:** Web/mdBook search is client-side and invisible server-side — accepted, no capture attempted.

---

## Visibility

| Option | Description | Selected |
|--------|-------------|----------|
| Owner + admin | Detailed sections (MCP activity, keywords) gated to owner/admin; aggregates unchanged | ✓ |
| All readers | Anyone with vault read access sees details | |

**User's choice:** Owner + admin only
**Notes:** Actor identity displayed as email (consistent with existing per-user counts on the Stats page).

---

## Date range

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed chips | 7 / 30 / 90 days applied to all sections (90 = retention ceiling) | ✓ |
| No selector | Everything on the 90-day window | |

**User's choice:** Fixed chips 7/30/90

---

## Claude's Discretion

- Storage shape for detailed activity (new table vs extended `access_log`).
- Excerpt truncation mechanics, tokenization details (stopwords), chip UI mechanics, awaited-vs-spawned best-effort writes.

## Deferred Ideas

- OKF conformance trend in Statistics (from Phase 18).
- Admin cross-vault analytics dashboard.
- WebDAV access analytics.
- Custom from/to date picker.
