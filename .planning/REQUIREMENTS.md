# Requirements: MoriNet

**Defined:** 2026-10-30
**Core Value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

## v1.2 Requirements — Team Access

Requirements for milestone v1.2. Each maps to roadmap phases.

### SSO (OIDC)

- [ ] **SSO-01**: Login page shows an SSO button when `[oidc]` is configured; the flow is OIDC authorization-code + PKCE against any OIDC-compliant IdP (Keycloak / Authentik / Authelia)
- [ ] **SSO-02**: A user whose **verified** IdP email (`email_verified`) matches an existing local account signs in through SSO and links to that account — never auto-linked to an admin account
- [ ] **SSO-03**: A first-time SSO user whose email is on an allowed domain (`allowed_email_domains`) gets an account auto-created with the `reader` role
- [ ] **SSO-04**: A configurable OIDC groups claim maps to MoriNet groups at each login (claim name + mapping configurable); mapping to the `admin` group is opt-in and demotions are logged
- [ ] **SSO-05**: `[oidc]` section in config.toml (issuer, client id/secret, allowed domains, claim mapping) with env overrides following the env-wins pattern; SSO can be disabled without affecting password login
- [ ] **SSO-06**: Password login keeps working for every account (coexistence); the bootstrap admin always remains available (IdP-outage safety net)

### API (read-only REST)

- [ ] **API-01**: Programmatic clients authenticate to `/api/v1` with existing `/account` API tokens (Bearer header only; session cookies are rejected on this surface)
- [ ] **API-02**: `GET /api/v1/vaults` lists the vaults the token's user can access
- [ ] **API-03**: `GET /api/v1/vaults/{slug}/notes` lists markdown notes with cursor pagination; `GET /api/v1/vaults/{slug}/notes/{path}` returns the raw markdown source
- [ ] **API-04**: `GET /api/v1/search?q=` searches notes across the token's accessible vaults
- [ ] **API-05**: ACL parity with MCP — a vault the user cannot see answers 404 (not 403); all errors use RFC 9457 `application/problem+json`
- [ ] **API-06**: Per-token rate limiting with `Retry-After` on 429

### CONF (conflict-safe async co-editing)

- [ ] **CONF-01**: WebDAV GET and PROPFIND expose strong ETags (sha256 of the raw source markdown bytes)
- [ ] **CONF-02**: WebDAV PUT with an `If-Match` precondition that does not match answers 412 Precondition Failed — nothing written, no rebuild, no vault_changes row
- [ ] **CONF-03**: WebDAV PUT with `If-None-Match: *` creates the file only if absent (412 if it exists)
- [ ] **CONF-04**: A local upload whose tree is stale versus the server is rejected with a per-file conflict list and can be re-submitted with an explicit force-overwrite flag; force-overwrites are recorded in vault_changes
- [ ] **CONF-05**: Unconditional writes (no precondition headers, plain uploads) keep working exactly as before — backward compatible last-write-wins

## Future Requirements

Deferred to a later release. Tracked but not in the v1.2 roadmap.

### Admin / identity

- **SSO-F01**: SCIM provisioning/deprovisioning from the IdP
- **SSO-F02**: Multiple IdP configurations per instance
- **SSO-F03**: Removal-on-absence group sync (v1.2 sync is additive; see research Gaps to Address)

### API

- **API-F01**: Write endpoints (upload/PUT via REST) — management stays on WebDAV + admin UI
- **API-F02**: Scoped tokens (`read:notes` vs `read:search`) instead of the all-or-nothing API token
- **API-F03**: Webhooks on vault events (refresh, wiki-changes, shares)

### Collaboration

- **CONF-F01**: Explicit file/folder locks chosen by the owner
- **CONF-F02**: Real-time collaborative editing (CRDT)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Real-time collaborative editing (CRDT/shared editor) | v1.2 ships conflict detection; CRDT complexity still not justified |
| MoriNet acting as an OAuth/OIDC **provider** | MoriNet is an OIDC client only; the IdP is external infra |
| Native mobile apps | Responsive web is enough |
| Encryption at rest | Relying on the server's FS |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| SSO-01 | Phase 17 | Pending |
| SSO-02 | Phase 17 | Pending |
| SSO-03 | Phase 17 | Pending |
| SSO-04 | Phase 17 | Pending |
| SSO-05 | Phase 17 | Pending |
| SSO-06 | Phase 17 | Pending |
| API-01 | Phase 16 | Pending |
| API-02 | Phase 16 | Pending |
| API-03 | Phase 16 | Pending |
| API-04 | Phase 16 | Pending |
| API-05 | Phase 16 | Pending |
| API-06 | Phase 16 | Pending |
| CONF-01 | Phase 15 | Pending |
| CONF-02 | Phase 15 | Pending |
| CONF-03 | Phase 15 | Pending |
| CONF-04 | Phase 15 | Pending |
| CONF-05 | Phase 15 | Pending |

**Coverage:**
- v1.2 requirements: 17 total
- Mapped to phases: 17 (Phase 15: CONF-01..05 · Phase 16: API-01..06 · Phase 17: SSO-01..06)
- Unmapped: 0 ✅

---
*Requirements defined: 2026-10-30*
*Last updated: 2026-10-30 for milestone v1.2 Team Access*
