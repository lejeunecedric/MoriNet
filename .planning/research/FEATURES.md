# Feature Research — v1.2 Team Access

**Domain:** self-hosted team content servers (Nextcloud, GitLab, Keycloak/Authentik-integrated apps, Dropbox-class sync)
**Researched:** 2026-10-30
**Scope:** OIDC SSO UX · read-only REST API conventions · ETag/If-Match conflict semantics + upload conflict UX
**Not re-researched (already shipped):** cookie login + bcrypt, invitations/reset email, API tokens at /account, per-vault ACL (groups/shares/personal), MCP tools, WebDAV PROPFIND/GET/PUT, wiki-changes log, Statistics.

---

## Feature Area 1 — Generic OIDC SSO (authorization-code flow)

### How it typically works (state of the art)

Reference implementations: Nextcloud `user_oidc`, GitLab OIDC (`allow_single_sign_on` / `auto_link_user` / `groups_attribute`), mlflow-oidc-auth, PowerDNS-AuthAdmin OIDC docs. The converged pattern:

1. **Discovery from issuer URL.** App is given `issuer` + `client_id` + `client_secret` and fetches `/.well-known/openid-configuration` (JWKS, endpoints). Modern default is **authorization-code flow with PKCE (S256)** even for confidential clients. ID token validated for `iss`/`aud`/`exp` + signature via JWKS; `state` + `nonce` stored short-lived (cookie) for CSRF/replay.
2. **SSO button on login page.** A "Sign in with <IdP name>" button beside the password form; password login stays as fallback (critical when IdP is down — matches MoriNet's "coexists with password login" requirement).
3. **Account linking by email.** On callback, if the IdP's `email` claim matches an existing local account → link (store `oidc_sub` on the user). Security nuance every serious implementation now enforces: **only auto-link on a *verified* email** (`email_verified: true` or trusted-internal IdP), otherwise an attacker who can register an email at the IdP takes over accounts. Unmatched email + allowed domain → auto-provision ("JIT provisioning"); unmatched + disallowed → reject before any user row is created.
4. **Allowed-domain auto-provisioning.** `allowed_email_domains` allow-list applied **to new users only**; existing users keep signing in regardless (PowerDNS-AuthAdmin semantics). Some apps add a group gate: only users in one of a set of IdP groups may sign in at all (mlflow-oidc-auth admission gate).
5. **Groups claim → app roles.** Claim name is configurable (`groups` for Keycloak/Authentik, `roles` for Nextcloud; expose `claim_email`/`claim_name`/`claim_groups` overrides). Nextcloud-style group provisioning: claim list → add/remove user from local groups, auto-creating unknown groups; a designated IdP group maps to admin (e.g. Authentik property-mapping `nc_groups` → Nextcloud `admin`). Group sync is re-applied at every login (claim is the source of truth).
6. **Nice-to-haves:** RP-initiated logout (end_session_endpoint) so sign-out ends the IdP session; "SSO account" marker so password-reset emails aren't sent to federated users.

### Table stakes (must ship)
- SSO button on `/login`, coexisting with password form; one IdP configured in `config.toml` (`[oidc]` section: issuer, client_id, client_secret, scopes).
- Auth-code + PKCE flow, discovery doc, JWKS validation of ID token, `state`+`nonce` CSRF protection.
- Account linking by email (verified-email only), `oidc_sub` column on users.
- Allowed-domain auto-provisioning (new users only; existing users unaffected).
- Groups claim → existing MoriNet `groups` table membership sync at each login, with a configurable claim name and an admin-group mapping (claim group → `admin` role).
- Scopes `openid profile email` + groups scope; claim overrides in config.

### Differentiators (where MoriNet can be better than average)
- **Claim → groups → per-vault ACL for free.** MoriNet already has `groups`, `vault_group_acl`, and per-group quotas; syncing the IdP groups claim into `groups` means IdP group membership *directly* governs vault access — most self-hosted apps only map admin/non-admin. This is the highest-leverage differentiator and nearly free given existing schema.
- **Group allow-list:** only users whose claim contains an allowed group may sign in (prevents "everyone in the corp IdP gets an account" surprise).
- **SSO + invitation interplay:** an invited-but-never-activated user whose invitation email matches the IdP email can activate via SSO directly (skip password set).
- SSO-linked accounts marked in admin UI (source column) so admins see who is federated vs local.

### Anti-features (deliberately NOT doing)
- **Implicit flow / ROPC** — deprecated; auth-code + PKCE only.
- **Auto-link on unverified email** — account-takeover vector; reject instead.
- **SCIM server / full user lifecycle sync** — overkill for a single-binary tool; login-time claim sync is enough.
- **Multiple concurrent IdPs** — one `[oidc]` block per instance; multi-provider config is mlflow-scale complexity with no MoriNet demand.
- **Refresh tokens / `offline_access`** — MoriNet sessions are its own cookies; no need to hold IdP tokens.
- **IdP as sole admin path** — `ensure_fallback_admin()` / ADMIN_EMAIL bootstrap must keep working (treklair incident lesson); IdP outage must never lock out admins.

### Complexity: MEDIUM-HIGH
New crate dependency (`openidconnect` — covers discovery/JWKS/validation/PKCE in one; alternative `oauth2` + manual ID-token claims validation is more code for less correctness). New `users` columns (`oidc_sub`, provider marker), new `[oidc]` config section, callback route, claim→group sync logic, and careful session handoff. The flow itself is well-trodden; the risk is in security details (nonce, aud, email_verified) and group-sync edge cases (claim removes a user's last group; claim grants admin).

### Dependencies on existing capabilities
- **Cookie session machinery** — OIDC callback must mint the same HttpOnly+HMAC session as password login (no parallel session system).
- **`users` table + bcrypt** — linking adds columns; password login untouched.
- **`groups` table + `ensure_builtin_groups()`** — claim mapping writes membership rows; quota logic (`free`=2) applies to provisioned users automatically.
- **Invitation flow** — decide precedence: SSO-provisioned user with pending invitation should consume/ignore the invitation.
- **`ensure_fallback_admin()`** — must remain the safety net independent of IdP.

---

## Feature Area 2 — Read-only REST API

### How it typically works (state of the art)

Converged 2024–2026 REST conventions (Stripe, GitHub, Google AIP-158, RFC 9457):

1. **Auth:** `Authorization: Bearer <token>` header — for MoriNet this is exactly the existing `/account` API token (same one MCP uses via `Store::user_for_api_token`). Never token-in-query-param.
2. **ACL scoping:** identical to MCP — personal + group-shared vaults; admin sees all. A vault the caller cannot see should return **404, not 403** (don't leak existence of private vaults); 401 reserved for missing/invalid token.
3. **Pagination:** cursor/keyset is the modern default for anything that grows (Stripe `starting_after`, opaque `page_token` per AIP-158 — "if users can deconstruct cursors, your internals become API surface"). Offset is acceptable for small bounded sets (vault list). Always include `has_more` + `next` link; GitHub exposes pagination via `Link` headers.
4. **Error shape:** RFC 9457 Problem Details (`application/problem+json`: `type`, `title`, `status`, `detail`, optional `instance`). This obsoleted RFC 7807 in 2023 and is the accepted standard — no bespoke `{error: {...}}` shapes.
5. **Rate limiting:** `429` + `Retry-After` header (RFC 9110/6585); optional `X-RateLimit-Limit/Remaining/Reset` so clients self-throttle. For a self-hosted single-team server a modest per-token token-bucket is enough (nginx may already cover gross abuse).
6. **Versioning:** `/api/v1` prefix — cheap insurance for future write endpoints.

### Table stakes (must ship)
- `GET /api/v1/vaults`, `GET /api/v1/vaults/<slug>/notes`, `GET /api/v1/vaults/<slug>/notes/<path>`, `GET /api/v1/search?q=` — the four MCP tools mirrored as REST.
- Bearer auth via existing API tokens; 401 vs 404-vs-403 semantics above.
- Pagination on list endpoints: `limit` + opaque cursor (or offset + `has_more` for v1 — vault/note counts are small; document the choice).
- RFC 9457 `application/problem+json` errors on all endpoints.
- JSON content type, UTF-8, note read returns raw markdown + metadata (title, path, size, updated-at).

### Differentiators
- **Exact parity with MCP tool semantics** (same store queries, same ACL, same search ranking) — scripts and LLM agents see identical data through both doors; one test suite can cover both.
- **ETag + `Cache-Control` on note reads** — the same content hash used for conflict detection (Area 3) and the wiki-changes log; a script can poll cheaply with `If-None-Match` → 304.
- **Changes endpoint** (`GET /api/v1/vaults/<slug>/changes`) exposing the existing `vault_changes` table — near-free and genuinely useful for sync/automation clients; most self-hosted tools don't expose a machine-readable changelog.
- Per-token rate limit with `Retry-After` — table stakes for public APIs, a differentiator at self-hosted scale (protects mdBook builds from runaway scripts).

### Anti-features
- **Write endpoints** — explicitly out of scope (management stays WebDAV + admin UI); don't let "read-only" drift.
- **Becoming an OAuth2 authorization server** (client-credentials, token issuance) — API tokens already exist; minting tokens stays on `/account`.
- **GraphQL / HATEOAS hypermedia** — no demand at this scale.
- **Client-deconstructable cursors** — opaque base64 at minimum, or plain offset honestly documented.
- **Leaking stack traces / internal paths in error `detail`.**

### Complexity: LOW-MEDIUM
Axum routes + a bearer-auth extractor (factor the one MCP already has) + handlers reusing the store queries MCP calls. Pagination and RFC 9457 response types are small hand-rolled pieces. Rate limiting is the only new mechanism (in-memory token bucket per token is sufficient for one process).

### Dependencies on existing capabilities
- **API tokens (`/account`, `user_for_api_token`)** — zero new auth surface.
- **ACL resolution** — the vault-access logic already shared by WebDAV/MCP.
- **MCP store queries** (list/read/search) — direct reuse; keep semantics in lockstep.
- **`vault_changes` table** — optional changes endpoint feeds off it for free.

---

## Feature Area 3 — Conflict-safe async co-editing (ETag / If-Match + upload check)

### How it typically works (state of the art)

**WebDAV/HTTP semantics (RFC 4918 §9.9, RFC 9110 §13.2 — normative):**
- Server returns a **strong ETag** on GET (and `getetag` in PROPFIND). ETag must change whenever content changes; content hash (sha256) is the natural strong validator for markdown.
- `PUT` + `If-Match: "<etag>"` → server applies only if current ETag matches; mismatch → **412 Precondition Failed**, nothing written. `If-Match: *` = "only if resource exists". This is *the* lost-update guard for parallel clients.
- `PUT` + `If-None-Match: *` → **create-only**: if the resource already exists → **412** (for non-GET methods, an If-None-Match match MUST yield 412). This is how a client avoids accidental overwrite when creating.
- RFC 4918's richer `If:` header exists (lock tokens + ETags), but with no-locks scope, plain If-Match/If-None-Match is the correct subset.
- Ordering rule: **authorization checks MUST run before conditional-header evaluation** (RFC 4918).
- Success: 201 (created) / 200 or 204 (updated), response carries the **new ETag** so the client can chain the next edit. Weak ETags must not satisfy If-Match.
- Standard DAV clients (Obsidian sync plugins, rclone, Cadence) already understand 412 and surface "file changed on server" — correct 412s *are* the UX for the WebDAV path; no client work needed.

**Upload conflict UX (browser path).** Industry convention for stale uploads:
- **Dropbox / Nextcloud / Sync.com:** never silently overwrite — create a **"conflicted copy"** (`name (conflicted copy 2026-10-30 0936).md`), keep server version canonical, notify the user with a visible badge/list, and let the human merge manually. Conflict copies are kept *out* of what other users see by default.
- **GitHub force-push pattern:** reject with a clear error listing what changed, offer an explicit "overwrite anyway" escape hatch.
- For MoriNet's whole-vault browser upload (re-upload is the update path, `vault_source = None`), the realistic UX: at upload time, hash each incoming file and compare against the current server file. Files that differ = conflicts. **The browser user sees a conflict page: "N files changed on the server since you downloaded — [file list]. Cancel and merge, or Upload anyway (overwrites)."** Force flag required; no silent last-write-wins.

### Table stakes (must ship)
- Strong ETag (content hash) on WebDAV GET responses + `getetag` in PROPFIND output.
- `If-Match` on PUT → 412 on mismatch (no write, no rebuild trigger).
- `If-None-Match: *` on PUT → 412 if exists (create-only guard).
- New ETag returned on successful PUT (201/200).
- Authz-before-precondition ordering.
- Upload-time conflict check: per-file hash compare against current source; conflict list page; explicit force-overwrite flag.

### Differentiators
- **One hash, three consumers:** the content hash powering ETags also feeds the `vault_changes` log (already records changed paths) and the upload conflict check — conflict detection, change history, and cache validation share one mechanism. Few self-hosted tools close that loop.
- **REST reads expose the same ETag** (Area 2) → a script can `GET` a note, edit locally, `PUT` back with `If-Match` — full read-modify-write safety across both programmatic doors.
- **Force-overwrite events recorded in `vault_changes`** ("overwrote 3 conflicting files") — audit trail of who clobbered what, visible on Statistics.
- Conflict page links each conflicting file to its wiki-changes entries (who changed it, when) — turns a dead-end error into an actionable diff-against-history prompt.

### Anti-features
- **Locks (LOCK/UNLOCK)** — explicitly out of scope ("no locks"); If-Match covers the realistic race.
- **CRDT / OT / real-time merge** — out of scope; conflict *detection*, not auto-merge.
- **Silent conflicted copies inside the vault source** — a `(conflicted copy).md` in the source tree becomes a published mdBook chapter; if conflict copies are ever stored, they must live outside the build tree. Default to reject+list, not copy.
- **Last-write-wins without warning** (today's behavior — the exact bug being fixed).
- **Weak ETags (`W/"..."`) for write validation** — must be strong for If-Match.
- **Evaluating conditionals before authz** — spec violation and info leak.

### Complexity: MEDIUM
ETag computation is trivial (sha256 of file, markdown is small). Conditional-header parsing in `webdav.rs` is a small hand-rolled parser (axum gives headers but no WebDAV conditional semantics; `*`, quoted tags, comma-lists). The upload conflict check adds a per-file hash-compare pass in the existing sanitized multipart pipeline plus a conflict-listing response and force flag. Edge cases: concurrent PUTs between check and write (acceptable residual race at team scale — document it), ETag stability across Refresh/rebuild (ETag must hash the *source* file, never the built output).

### Dependencies on existing capabilities
- **`webdav.rs`** — add conditional handling to existing PUT/GET/PROPFIND; PUT stays admin-gated.
- **Upload pipeline (Phase 14)** — insert conflict gate before the convert/build/register step; reuse `vault_source = None` re-upload semantics.
- **`vault_changes` table** — already fed by WebDAV PUT + Refresh; store/derive content hashes there to seed conflict checks and force-overwrite audit.
- **Per-vault ACL + API tokens** — authz gate unchanged, runs first.
- **mdBook rebuild trigger** — must fire only on *successful* writes (412 = no rebuild).

---

## Cross-cutting summary

| Feature area | Complexity | Biggest dependency | Biggest risk |
|---|---|---|---|
| OIDC SSO | Medium-high | Cookie session + groups table | Security details (email_verified, nonce, aud); group-sync edge cases |
| Read-only REST API | Low-medium | API tokens + MCP store queries | Scope creep into writes / OAuth server |
| Conflict-safe co-editing | Medium | webdav.rs + upload pipeline + vault_changes | ETag stability across rebuilds; residual check-write race |

**Build order implied by dependencies:** (3) ETag/conflict work is self-contained and improves WebDAV immediately → (2) REST API reuses MCP queries and can expose the ETags from (3) → (1) OIDC is the largest new surface but touches none of the other two; it depends only on session/groups plumbing.

**Sources:** RFC 4918 §9.8.5/9.9 (PUT conditionals, 412 rules), RFC 9110 §13.2 (If-Match/If-None-Match normative), draft-reschke-http-etag-on-write (412 → client merge), RFC 9457 (problem+json), Google AIP-158 / Stripe cursor conventions, Nextcloud user_oidc group provisioning (#653), GitLab OIDC docs (`allow_single_sign_on`, `groups_attribute`), mlflow-oidc-auth config (domain ownership, group gates), PowerDNS-AuthAdmin OIDC (PKCE S256, allowed_email_domains new-users-only), Nextcloud/Dropbox conflicted-copy UX docs.
