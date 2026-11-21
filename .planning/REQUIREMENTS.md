# Requirements: MoriNet

**Defined:** 2026-11-20
**Milestone:** v1.5 Offline Reading & Polish
**Core Value:** Load an Obsidian/llm-wiki vault and serve it instantly as a navigable, multi-user, secured mdBook site — without the user ever having to touch mdBook themselves.

Scope source: all open GitHub issues at milestone start — #12, #13, #14, #15, #16, #17, #18, #19 (#10 verified delivered and closed 2026-11-20).

## v1.5 Requirements

### Offline reading (issue #15)

- [ ] **OFFLINE-01**: A reader who has opened pages online can reopen those pages and their same-origin assets (CSS, JS, images, fonts) when the network is unavailable — automatic cache of visited reading routes only (no crawling, no depth-1/2 prefetch); online always serves the live response; uncached pages show a clear offline-unavailable page.
- [ ] **OFFLINE-02**: A reader can download the complete currently accessible second brain from a compact save/download control in the book top chrome — all files required for faithful offline rendering (HTML, CSS, JS, images, fonts, theme assets) — with progress, meaningful final states (not downloaded / downloading / available offline / failed), cancellation and retry, and an explicit user action to remove the local snapshot. Accessible name "Enregistrer ce second brain pour une consultation hors ligne".
- [ ] **OFFLINE-03**: The visited-page cache never touches non-reading routes: account, admin, login, password reset, invitation, API, MCP, WebDAV responses are never cached or served from cache.
- [ ] **OFFLINE-04**: Offline snapshots respect ACLs: a snapshot is built only from files the reader can access at download time; a later server-side ACL change or logout does NOT silently delete an existing snapshot, and no vault content leaks across vaults or readers.

### Vault operations (issues #12, #14)

- [ ] **OPS-01**: An owner can enable per-vault auto-refresh in the Vault Edit page — a value box plus a minutes/hours/days/weeks selector — and the server then syncs and rebuilds that vault on that schedule, best-effort (failures logged, never wedge the vault), visible in the wiki-changes log like a manual Refresh.
- [ ] **OPS-02**: A Refresh reports dead links instead of silently building: malformed URLs (e.g. the B3Wiki `log.md` bad link) and links to missing pages are listed per vault after build — a non-blocking warning report, never a build failure.

### UI polish (issues #13, #16, #17)

- [ ] **UI-01**: Clicking the OKF badge on a vault page opens the conformance detail: which concepts pass/fail and why (not-conformant reasons, unfulfilled checks), instead of a static label.
- [ ] **UI-02**: The account menu gains a "Looks" entry before "About" letting the reader pick a color scheme matching the mdBook theme palette, persisted per reader and applied across the app and served books.
- [ ] **UI-03**: The Vault Edit page menu is realigned: the Theme selector sits below the "allow…" line and above the Save button.

### Docs & ecosystem (issues #18, #19)

- [ ] **DOCS-01**: The built-in docs vault documents the v1.1–v1.5 feature set (statistics & observability, OKF export + viewer, SSO, REST API, offline reading) and the bundle is packaged to OKF 0.3 — passing the ecosystem validators (okflint + OKForest) at the zero-errors/zero-warnings bar.
- [ ] **DOCS-02**: The README reflects the current version and feature set (no stale "1.4" claims): features list, version references, and the OKF conformance claim updated for the shipped state.

## Future Requirements (deferred)

- **OKF-F01**: trust-aware rendering of OKF families (needs Impeccable design pass)
- **API-F01** write endpoints; **API-F02** scoped tokens; **API-F03** webhooks
- **TEST-G1..G3**: composed E2E tests from the v1.2 audit
- PROPFIND trailing-slash 404 routing quirk fix (documented in 15-HUMAN-UAT.md)
- IdP docs note: claims must be in the ID token (`conformIdTokenClaims`, 17-HUMAN-UAT.md)

## Out of Scope

- Depth-1/depth-2 offline prefetching — explicitly excluded by issue #15 spec
- Native mobile apps / PWA store packaging — service-worker offline within the browser is enough
- Real-time collaborative editing; encryption at rest; Attested Computation execution (standing exclusions)

## Traceability

| REQ | Phase | Status |
|-----|-------|--------|
| (filled by roadmap) | | |
