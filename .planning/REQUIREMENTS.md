# Requirements: MoriNet

**Defined:** 2026-11-18
**Milestone:** v1.4 Statistics & Observability
**Core Value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

## v1.4 Requirements

### Statistics & observability (issue #10)

- [ ] **STAT-01**: Owner/admin can trust that every read of their vaults is counted: access events logged across ALL surfaces — web page views, REST API note reads/searches, and MCP tool invocations (`access_log.source` CHECK widened to include `api`; migration 0014 rebuilds the table, preserves history).
- [ ] **STAT-02**: Owner can see the **top viewed pages** of each vault on the Statistics page (ranked by views, per configurable date range, web source).
- [ ] **STAT-03**: Owner can browse recent **MCP activity** per vault: each request (tool, arguments summary, actor) paired with its outcome (status, latency, response size + short excerpt) — requests left, answers right, newest first.
- [ ] **STAT-04**: Owner can see **top search keywords** per vault, aggregated from MCP `search_notes` and REST `search` queries.
- [ ] **STAT-05**: Logging is best-effort and bounded: failures never break the request path, entries pruned to 90 days at boot (same convention as `vault_changes`), full response bodies never stored (excerpt + size only), API/MCP surfaces keep 404-not-403 ACL semantics (no stats leak for inaccessible vaults).

### OKF ecosystem quick wins (research: `.planning/research/OKF-ECOSYSTEM.md`)

- [ ] **OKFE-01**: Built-in docs vault (and demo vault) pass external OKF validators **okflint** and **OKForest CLI** with zero errors; conformance claim + badge published in the README; validation runnable locally (optional `scripts/check.sh` step — no GitHub CI).
- [ ] **OKFE-02**: "Export OKF bundle" zip ships the official **viz.html** static graph visualizer at the bundle root, working offline on the exported files (no network, no build step).

## Backlog / deferred candidates

- **OKF-F01**: trust-aware rendering of OKF families in mdBook pages (status/stale/trust chips, sources footer) — needs Impeccable design pass
- **API-F01**: write endpoints; **API-F02**: scoped tokens; **API-F03**: webhooks
- **TEST-G1..G3**: composed E2E tests from the v1.2 audit (SSO→API token→read; group-revocation direct test; shared-vault coexistence)
- **OKF ecosystem (later)**: JSON-LD/schema.org bridge (listed ecosystem gap), Obsidian companion plugin collaboration, registry submissions
- **Out of scope (standing):** Attested Computation *execution* (spec defers runtime protocol; running bundle code is a security non-starter); real-time collab; mobile apps; encryption at rest

## v1.3 Requirements (shipped 2026-11-06, released v1.4.0)

- [x] **OKF-01**: At refresh/register, vault sources are scanned for OKF conformance: per-file non-empty `type` frontmatter, reserved `index.md`/`log.md` shape, version guess (v0.1 vs v0.2); stats persisted per vault.
- [x] **OKF-02**: Vault page shows an OKF badge: "OKF v0.2 — N/M concepts" (or v0.1 / "not an OKF bundle") with conformance detail.
- [x] **OKF-03**: "Export as OKF bundle" streams a zip of the vault source (byte-identical files), synthesizing a root `index.md` with `okf_version: "0.2"` in-memory when absent; gated by vault read ACL.
- [x] **OKF-04**: `/api/v1` note list/read expose parsed frontmatter metadata (`type`, `tags`, `status`, derived trust tier per OKF §5.3); MCP list/search accept optional `type`/`tags` filters; ACL semantics unchanged (404 on inaccessible).

## Traceability

| REQ | Phase | Status |
|-----|-------|--------|
| OKF-01..04 | 18 | Complete |
| STAT-01..05 | 19 | Planned |
| OKFE-01..02 | 20 | Planned |

---

*Shipped requirements from v1.0–v1.2 archived under `.planning/milestones/`.*
