# Requirements: MoriNet

**Defined:** 2026-11-06
**Milestone:** v1.3 OKF Bundle Support
**Core Value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

## v1.3 Requirements

### OKF Bundle Support (issue #11)

- [ ] **OKF-01**: At refresh/register, vault sources are scanned for OKF conformance: per-file non-empty `type` frontmatter, reserved `index.md`/`log.md` shape, version guess (v0.1 vs v0.2); stats persisted per vault.
- [ ] **OKF-02**: Vault page shows an OKF badge: "OKF v0.2 — N/M concepts" (or v0.1 / "not an OKF bundle") with conformance detail.
- [ ] **OKF-03**: "Export as OKF bundle" streams a zip of the vault source (byte-identical files), synthesizing a root `index.md` with `okf_version: "0.2"` in-memory when absent; gated by vault read ACL.
- [ ] **OKF-04**: `/api/v1` note list/read expose parsed frontmatter metadata (`type`, `tags`, `status`, derived trust tier per OKF §5.3); MCP list/search accept optional `type`/`tags` filters; ACL semantics unchanged (404 on inaccessible).

## Backlog / deferred candidates

- **OKF-F01**: trust-aware rendering of OKF families in mdBook pages (status/stale/trust chips, sources footer) — needs Impeccable design pass
- **STAT-01** (issue #10): web + MCP statistics, incl. REST access logging (needs `access_log.source` CHECK widened)
- **API-F01**: write endpoints; **API-F02**: scoped tokens; **API-F03**: webhooks
- **TEST-G1..G3**: composed E2E tests from the v1.2 audit (SSO→API token→read; group-revocation direct test; shared-vault coexistence)
- **Out of scope (standing):** Attested Computation *execution* (spec defers runtime protocol; running bundle code is a security non-starter); real-time collab; mobile apps; encryption at rest

## Traceability

| REQ | Phase | Status |
|-----|-------|--------|
| OKF-01 | 18 | Planned |
| OKF-02 | 18 | Planned |
| OKF-03 | 18 | Planned |
| OKF-04 | 18 | Planned |

---

*Shipped requirements from v1.0–v1.2 archived under `.planning/milestones/`.*
