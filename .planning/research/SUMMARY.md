# Project Research Summary

**Project:** MoriNet — v1.2 "Team Access" (OIDC SSO + read-only REST API + ETag conflict detection)
**Domain:** self-hosted team content server (Rust axum + SQLite + mdBook vault; comparable to Nextcloud/GitLab-class self-hosted apps with IdP integration)
**Researched:** 2026-10-30
**Confidence:** HIGH

## Executive Summary

v1.2 adds three features to an already-shipped Rust axum/SQLite mdBook-vault server: generic OIDC SSO coexisting with password login, a read-only REST API under `/api/v1`, and ETag/If-Match conflict detection on WebDAV PUT plus browser upload. The research converges on a remarkably low-footprint implementation: **exactly one new dependency tree** (`openidconnect` 4.0.1 with rustls), **zero new crates** for the REST API and ETag work (axum `Json` + serde and `sha2` + `hex` are already in the repo), and **one small migration** (0011, users `provider` column). Everything else is reuse of existing machinery — the HMAC session cookie, `user_for_api_token`, `accessible_vaults`/`access_for` ACL, and the `pub(crate)` read helpers in `mcp.rs`.

The recommended build order is **A) ETag/conflict → B) REST API → C) OIDC**: ETag is self-contained and feeds the REST read responses; the REST API is pure reuse of MCP store queries; OIDC is the largest new surface, the only new dependency, and the only change that can break login for existing users — so it lands last on a stable base.

The dominant risk is security, not plumbing. The three repeatedly-CVE'd traps are: (1) OIDC email auto-link account takeover — gate auto-link on `email_verified`, treat `(iss, sub)` as the authoritative identity, never auto-link admin accounts; (2) REST ACL bypass — reuse the exact MCP scoping on *every* endpoint including search and path-based reads, Bearer-only (no cookie fallback); (3) ETag correctness — a strong sha256 of the raw **source** markdown bytes owned by the WebDAV/source layer, never mtime, never `W/`, never the mdBook build output. Each has a concrete, cheap mitigation documented in PITFALLS.md.

## Key Findings

### Recommended Stack

See [STACK.md](STACK.md). One meaningful addition: `openidconnect = "4.0.1"` with `features = ["reqwest", "rustls-tls"]` — it covers discovery, JWKS caching, PKCE, and ID-token validation in one maintained crate; the alternatives (`oauth2` alone, or manual reqwest + `jsonwebtoken`) re-implement the security-critical 30% with more risk. Caveat: `oauth2` 5.0.0 pins transitive `reqwest` 0.12.x — do not add a direct `reqwest = "0.13"` dep or two copies compile. No breaking upgrades: `axum 0.8` stays; **do not bump `tower-http` 0.6 → 0.7.1** this milestone.

**Core technologies:**
- `openidconnect` 4.0.1 (reqwest + rustls-tls): OIDC discovery/JWKS/PKCE/ID-token validation — only new dep tree; rustls keeps the static-binary `cargo install` story (git2 already vendors OpenSSL)
- axum 0.8 `Json` + serde DTOs: REST API responses — already in repo; proper `#[derive(Serialize)]` DTOs in new `src/api.rs`, not ad-hoc `json!` macros
- `sha2` 0.10 + `hex` 0.4: strong ETags — already direct deps (API-token/session hashing); one `etag_of(bytes)` helper serving WebDAV, upload, and REST
- Existing session cookie (`SessionCodec`): OIDC callback mints the *same* signed cookie as password login — zero middleware change

### Expected Features

See [FEATURES.md](FEATURES.md). All three areas have well-converged industry patterns (Nextcloud `user_oidc`, GitLab OIDC, RFC 9457/9110/4918).

**Must have (table stakes):**
- OIDC: auth-code + PKCE (S256) only, discovery + JWKS validation, `state`+`nonce`; SSO button beside password form; email linking gated on `email_verified`; allowed-domain auto-provisioning (new users only); groups claim → MoriNet `groups` sync with configurable claim name and admin-group mapping
- REST: four endpoints mirroring MCP tools (`/api/v1/vaults`, notes list, note read, search); Bearer via existing `/account` tokens; 401 vs 404-not-403 semantics; cursor/offset pagination with `has_more`; RFC 9457 `application/problem+json` errors; per-token rate limit with `Retry-After`
- Conflict: strong content-hash ETag on WebDAV GET + `getetag` in PROPFIND; `If-Match` → 412 on mismatch; `If-None-Match: *` create-only → 412; new ETag on successful PUT; authz-before-precondition ordering; upload-time per-file hash compare with conflict-list page + explicit force-overwrite flag

**Should have (differentiators):**
- IdP groups claim → groups → per-vault ACL *for free* (existing `vault_group_acl` schema) — most self-hosted apps only map admin/non-admin
- One hash, three consumers: ETags + `vault_changes` log + upload conflict check share one mechanism; REST reads expose the same ETag so scripts can do safe read-modify-write via `If-Match`
- Group allow-list admission gate; SSO activation of pending invitations; force-overwrite events recorded in `vault_changes` (audit trail)

**Defer (anti-features, deliberately out):**
- Implicit flow / ROPC; SCIM / full lifecycle sync; multiple concurrent IdPs; refresh tokens
- REST write endpoints, OAuth2 authorization-server behavior, GraphQL/HATEOAS
- WebDAV locks (LOCK/UNLOCK), CRDT/OT auto-merge, silent conflicted copies inside the vault source (they'd become published chapters — reject+list instead)

### Architecture Approach

See [ARCHITECTURE.md](ARCHITECTURE.md). The design slots into the existing five-branch router without disturbing it: OIDC routes go on the `public` branch and end by signing the same session cookie password login uses (session claims carry only email+expiry, so SSO and password logins are indistinguishable downstream — `require_session`/`require_admin`/`require_dav_auth` untouched). The REST API is a new router branch with its own `require_api_token` middleware cloned from the MCP self-auth pattern, calling read helpers extracted from `mcp.rs`. ETag is a helper in `webdav.rs` with no new tables — derived from the filesystem at request time.

**Major components:**
1. `src/oidc.rs` (new) — discovery fetch, JWKS cache, auth-URL builder, code exchange, ID-token validation, state/nonce codec (signed short-lived cookie, no server store); `[oidc]` config section following the env-wins pattern; `migrations/0011` adds `users.provider` with the unguessable-bcrypt-placeholder trick for `password_hash NOT NULL` (avoids a SQLite table rebuild)
2. `src/api.rs` (new) — REST handlers + shared vault-read logic extracted from `mcp.rs` bodies (`list_md_files`, `search_md`, `accessible_vaults`, `source_dir_of`, `safe_join`); `require_api_token` middleware; `mcp.rs` tools become thin wrappers so ACL logic stays single-sourced
3. `src/webdav.rs` (modified) — `etag_of()` strong-hash helper; `If-Match`/`If-None-Match` parsing in the PUT path before `fs::write`; `getetag` in PROPFIND; same helper gates the upload pipeline
4. `src/store.rs` (modified, additive) — `create_sso_user` / link-or-create fns; group sync as a single transaction, skipped when the membership set is unchanged

### Critical Pitfalls

See [PITFALLS.md](PITFALLS.md) for all 18. Top items by priority ranking:

1. **OIDC email auto-link account takeover** (A1, repeatedly CVE'd) — gate auto-link on `email_verified=true`, treat `(iss, sub)` as the authoritative identity (never email as the join key), never auto-link an `admin` row, log every link, fail closed on unmatched email
2. **REST ACL bypass** (B1) — reuse the exact MCP scoping on every endpoint including search and path-based reads; canonicalize note paths against vault escape; Bearer-only (cookie never valid on `/api/v1`); port `tools_are_acl_scoped` to a REST test matrix
3. **ETag layer/validator correctness** (C1/C2) — strong sha256 of raw **source** bytes owned by the WebDAV/source layer; never mtime (git-refresh resets mtimes → phantom conflicts), never `W/` (never satisfies If-Match), never build output; 412 before touching the file, 400 on malformed headers
4. **Group-claim privilege escalation / staleness** (A5) — read groups only from the signature-verified ID token; allow-list mapping, admin mapping opt-in; distinguish absent vs empty claim and fail closed for admin
5. **Write races + SQLite contention** (C3/C4/C5) — per-vault in-process lock + temp-file/atomic-rename around check→write; one shared "write source file" helper for WebDAV PUT, upload, and Refresh; WAL + `busy_timeout`, group sync in one transaction

## Implications for Roadmap

Based on research, suggested phase structure (matches the build order all three researchers independently derived):

### Phase 1: ETag / Conflict Detection
**Rationale:** Smallest, zero auth coupling, no new deps, independently shippable; its hash helper becomes a shared primitive for Phases 2–3. Fixes the exact bug being targeted (silent last-write-wins) earliest.
**Delivers:** `etag_of()` strong-hash helper; ETag on WebDAV GET + `getetag` in PROPFIND; `If-Match`/`If-None-Match` → 412 on PUT (unconditional PUT still works — backward compat with rclone/Obsidian); upload-time conflict list + force-overwrite flag; per-vault write lock + atomic rename
**Addresses:** Feature Area 3 (conflict-safe co-editing)
**Avoids:** C1 (ETag on wrong layer / mtime), C2 (weak ETags), C3 (TOCTOU race), C6 (designing for clients that never send If-Match)

### Phase 2: Read-only REST API
**Rationale:** No new deps; pure reuse of `user_for_api_token` + MCP store queries; ships ETag headers on reads, so Phase 1 feeds it directly.
**Delivers:** `src/api.rs` with shared read helpers extracted from `mcp.rs`; `require_api_token` middleware; `/api/v1/{vaults, vaults/{slug}/notes, .../notes/{path}, search}`; RFC 9457 errors; pagination; per-token rate limit; ACL test matrix ported from MCP
**Uses:** axum 0.8 Json + serde DTOs, existing sha256 ETag helper
**Implements:** Architecture component 2 (`src/api.rs` + api router branch)
**Avoids:** B1 (ACL bypass), B2 (token leakage), B3 (serving build output), B4 (cookie/Bearer auth mixing)

### Phase 3: OIDC SSO
**Rationale:** Largest new surface, only new dependency tree, only migration, and the only change that can break login for existing users — lands last on a stable base. Within the phase: password-login regression tests first (coexistence is the acceptance bar), then provisioning/linking, then group sync last (additive-only in v1.2).
**Delivers:** `src/oidc.rs`; `[oidc]` config section (env-wins, disabled by default); migration 0011 (`users.provider` + placeholder bcrypt hash); `/auth/oidc/start` + `/auth/oidc/callback` on the public router minting the existing session cookie; SSO button on login; verified-email-gated linking + allowed-domain auto-provision; groups-claim sync with admin-group mapping
**Uses:** `openidconnect` 4.0.1 (reqwest + rustls-tls)
**Implements:** Architecture components 1 and 4 (`src/oidc.rs`, store SSO fns)
**Avoids:** A1 (email auto-link takeover), A2 (state/nonce/PKCE gaps), A5 (group-claim escalation), A6 (session fixation), A7 (gates bypassed by auto-provision), X1 (hand-rolled OIDC)

### Phase Ordering Rationale

- Dependencies discovered: ETag hash → reused by REST reads and the upload gate; REST reuses MCP helpers independent of OIDC; OIDC touches neither but is the riskiest change, so it goes last (ARCHITECTURE §5, FEATURES cross-cutting summary agree)
- Architecture grouping: each phase maps to one new file (`webdav.rs` change, `api.rs`, `oidc.rs`) with additive-only store changes — clean phase boundaries
- Pitfall avoidance: shipping ETag first means the conflict model is proven before REST exposes the same ETag; shipping OIDC last means login-breaking risk lands on a stable base with regression tests already green

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3 (OIDC):** IdP-specific claim shapes (Keycloak vs Authentik vs Authelia groups claim, `memberOf` drift) and the group-sync semantics decision (additive vs replace-on-login) need a concrete decision per deployment target; A5 recommends replace-on-login with fail-closed admin mapping while ARCHITECTURE recommends additive-only for v1.2 — resolve explicitly in the phase plan
- **Phase 1 (conflict):** residual check-write race and Refresh-path interaction (C4) — decide whether Refresh also goes through the shared write helper in v1.2 or is documented as a known gap

Phases with standard patterns (skip research-phase):
- **Phase 2 (REST API):** fully conventional — RFC 9457, cursor pagination, Bearer auth all well-documented; the only MoriNet-specific work is the ACL test matrix
- **Phase 1 ETag mechanics:** RFC 9110 §13.2 / RFC 4918 §9.9 are normative and small

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Versions verified live against crates.io 2026-10-30; dep diff explicit; rejected alternatives justified |
| Features | HIGH | Grounded in RFCs (9457, 9110, 4918) and reference implementations (Nextcloud user_oidc, GitLab, mlflow-oidc-auth, PowerDNS-AuthAdmin) |
| Architecture | HIGH | Checked against actual source (server.rs 8033 L, store.rs 3327 L, migrations 0001–0010); integration points cite line numbers |
| Pitfalls | HIGH | Anchored in named CVEs/advisories (Wekan GHSA, AshAuthentication, better-auth) and code-verified gaps (zero ETag handling today) |

**Overall confidence:** HIGH

### Gaps to Address

- **Group-sync semantics conflict:** ARCHITECTURE recommends additive-only sync for v1.2; PITFALLS A5 recommends replace-on-login with fail-closed admin handling. Decide in the OIDC phase plan; whichever is chosen, absent-vs-empty claim must be distinguished and demotions logged (STACK pitfall #3 flags the same decision)
- **`(iss, sub)` storage:** PITFALLS A1 wants `(iss, sub)` as the authoritative identity, but ARCHITECTURE's migration 0011 only sketches `users.provider`. The migration must also carry the OIDC subject (e.g. `oidc_iss`/`oidc_sub` columns or a small `user_identities` table) — settle during phase planning
- **ETag hash vs mtime trade-off:** STACK and PITFALLS mandate content hash (sha256); ARCHITECTURE §2.3 mentions `mtime+size` as an option — PITFALLS C1 explicitly rules mtime out (git-refresh resets mtimes). Go with content hash; do not revisit
- **Demo-lock guard on REST reads:** whether the v1.1.0 demo-account lock applies to read-only API access is an explicit decision, not an oversight (PITFALLS X3)
- **Real-client ETag validation:** UAT must include at least one real WebDAV client (rclone `--vfs-cache-mode writes` or Cadence/obsidian-webdav), not just curl (PITFALLS C6)
- **openidconnect maintenance cadence:** 4.0.1 released 2025-07, slow-moving; oauth2 pins reqwest 0.12.x. Fine today — revisit if a CVE lands (STACK pitfall #1)

## Sources

### Primary (HIGH confidence)
- crates.io (verified 2026-10-30) — `openidconnect` 4.0.1, `oauth2` 5.0.0, `axum` 0.8.9, `tower-http` 0.7.1, `sha2` 0.10, `jsonwebtoken` 11.1.0
- MoriNet source tree — `src/server.rs`, `src/store.rs`, `src/config.rs`, `src/session.rs`, `src/webdav.rs`, `src/mcp.rs`, `migrations/0001..0010` (all architecture claims line-cited)
- RFC 4918 §9.8.5/9.9, RFC 9110 §13.1.1/13.2, RFC 9457 — normative WebDAV/HTTP conditional and error semantics

### Secondary (MEDIUM confidence)
- Nextcloud `user_oidc`, GitLab OIDC docs (`allow_single_sign_on`, `groups_attribute`), mlflow-oidc-auth, PowerDNS-AuthAdmin OIDC — converged SSO/group-provisioning patterns
- Stripe / Google AIP-158 — cursor pagination conventions; Nextcloud/Dropbox — conflicted-copy UX conventions
- CVE/advisory history — Wekan GHSA-mp7g-hj5q-gxhq, AshAuthentication CVE-2026-49757, better-auth CVE-2026-53516 (email auto-link takeover class)

### Tertiary (LOW confidence)
- rclone/Finder/Obsidian WebDAV If-Match behavior — community consensus, validate with a live client during UAT

---
*Research completed: 2026-10-30*
*Ready for roadmap: yes*
