# PITFALLS — v1.2 Team Access (OIDC SSO · Read-only REST API · Conflict-safe co-editing)

Research dimension: common mistakes when adding these three features to MoriNet's *existing* auth/ACL/storage stack. Grounded against current code: HMAC-signed HttpOnly session cookie (`src/session.rs`), bcrypt users + sha256-hex API tokens (`src/store.rs`, plaintext shown once at `/account/tokens`), per-vault ACL + `vault_group_acl` + groups (`src/server.rs` middleware layer, `src/mcp.rs` ACL-scoped tools), WebDAV PUT on the vault **source** with `mdbook build` out (`src/webdav.rs`), no ETag/conditional logic anywhere today (`rg -i etag src/` → zero hits).

Each pitfall: **warning signs** → **prevention** → **phase** (SSO / API / conflict).

---

## A. OIDC SSO

### A1. Account takeover via email auto-linking when the IdP doesn't verify email — THE classic OIDC bug
This exact bug produced a long line of CVEs/advisories (Wekan GHSA-mp7g-hj5q-gxhq, AshAuthentication CVE-2026-49757, better-auth CVE-2026-53516, and others): auto-provision "by email match" merges the incoming OIDC identity into any local account with that email, and on self-hosted Keycloak/Authentik the `email` claim is often admin-/self-settable and `email_verified` may be absent or attacker-influenced. Attacker pre-registers (invitation flow!) an account carrying the victim's email, or gets their IdP profile email set to the victim's, and the victim's — or their own — next SSO login lands in the wrong account.
- **Warning signs:** linking code reads `claims.email` and `SELECT ... WHERE email = ?` with no other check; no `email_verified` check; linking can attach to an `admin` row; no log line when a link happens.
- **Prevention:** store `(iss, sub)` as the authoritative identity in a `user_identities` table — never email as the join key. Email auto-link only when (a) the IdP asserts `email_verified=true` (config flag `require_verified_email`, default **true** for MoriNet since the IdP is customer-managed and may be sloppy), (b) the matched local account is not `admin`, (c) the email on the local account was itself set by a verified path (invitation acceptance counts; self-service PATCH does not). Log every link at warn. Fail closed: unmatched email → "account exists, link it explicitly via admin", never silent merge.
- **Phase:** SSO (highest-severity item in the milestone).

### A2. Missing or unvalidated `state` / `nonce` / PKCE on the authorization-code flow
- **Warning signs:** callback handler that never compares a stored `state` to the query param; no nonce check against the ID token; session/nonce stored in a global `HashMap` (leaks across restarts, never expires); accepting the code→token exchange without checking `iss`, `aud`, `exp`, `nonce`, and signature alg (accepting `alg=none` or HS256-with-public-key confusion).
- **Prevention:** use a maintained crate (`openidconnect` or `oauth2`) rather than hand-rolling; `state` = random, single-use, bound to the browser session (signed cookie, same HMAC secret as sessions), TTL ≤ 10 min; `nonce` embedded in the auth request and required to match the ID token; validate `iss` + `aud` (client_id) + `exp` + signature on the ID token; require PKCE (S256) — cheap and Keycloak/Authentik/Authelia all support it. Reject `alg=none` explicitly.
- **Phase:** SSO.

### A3. `redirect_uri` mismatch and open-redirect on the post-login return
- **Warning signs:** "redirect_uri mismatch" errors that only appear in prod (staging URL registered in IdP, prod not); the callback honoring an attacker-supplied `?next=` / `?redirect=` param without allow-listing → open redirect that can steal the session cookie or an auth code.
- **Prevention:** `redirect_uri` is built from config (`config.toml` external base URL), exact string match against what's registered at the IdP — never derived from the `Host` header (nginx `X-Forwarded-Host` spoofing). Post-SSO return target restricted to relative paths on the same origin. Document in the setup guide that each deployment must register its exact URI at the IdP.
- **Phase:** SSO.

### A4. JWKS fetched per request / never cached / never rotated
- **Warning signs:** latency spike on every login; login outage when the IdP rotates keys (unknown `kid` → 500); or the opposite — a DoS amplifier hammering the IdP's JWKS endpoint on every token.
- **Prevention:** cache JWKS with the IdP's `Cache-Control`/`expires` (typical 24h), refresh-on-unknown-`kid` with a rate limit (e.g. min 5 min between forced refreshes) so a forged token with a bogus `kid` can't hammer the IdP. The `openidconnect` crate does this; hand-rolled fetches usually don't.
- **Phase:** SSO.

### A5. Group-claim → MoriNet group mapping = privilege escalation vector
Two failure modes: (1) **spoofing** — if the groups claim is read from a place the user can influence (userinfo from a sloppy IdP, or worse, accepted from the request), a user grants themselves `admin`; (2) **staleness** — groups are synced only at login, so a user removed from the IdP `admins` group keeps MoriNet admin until their session expires (HMAC cookie TTL!). Also: an empty/missing groups claim silently demoting everyone, or claim-name drift (`groups` vs `memberOf` vs `roles`) misconfigured per IdP.
- **Warning signs:** admin role derived from a claim without pinning which token (ID vs access) and which source; no config validation that the claim exists; group sync silently wiping local group memberships an admin set by hand; demotion invisible in logs.
- **Prevention:** read groups **only** from the signature-verified ID token (or userinfo fetched with the access token, config-selected, documented); claim path configurable (`groups_claim` in `config.toml`); mapping table claim-value → MoriNet group, **allow-list only** — unknown claim groups are ignored, never auto-created as privileged; admin mapping opt-in (`admin_group = "morinet-admins"`). Full replace-on-login semantics for OIDC-managed groups, logged on change (grant/demote at info level), and cap SSO session TTL so revocation propagates. Missing claim ≠ "remove all groups" if the claim is absent vs empty — distinguish, and fail closed for the admin mapping.
- **Phase:** SSO (with an ACL touch-point shared with API).

### A6. Session fixation / privilege change when mixing SSO and password login
MoriNet's cookie is a stateless HMAC token carrying the email — fixation risk is lower than server-side session stores, but the *privilege-change* rule still applies: a password-authenticated session that completes an OIDC callback must not keep the pre-auth cookie; and a user whose groups/admin changed via SSO sync keeps the old cookie's claims until re-sign.
- **Warning signs:** the OIDC callback reuses whatever session cookie was present before login; no cookie replacement at the moment identity is established; two users' cookies interchangeable across a shared browser.
- **Prevention:** always issue a **fresh** session cookie at the end of the OIDC callback (overwrite + rotate), clear any pre-existing session before starting the flow; bind the flow's `state` to the pre-flow browser session so a planted cookie can't harvest the code. Since the cookie embeds identity not role (verify!), re-check role/groups from DB on each request rather than trusting cookie claims.
- **Phase:** SSO.

### A7. Auto-provisioned accounts bypass the existing gates (invitations, quotas, demo lock)
- **Warning signs:** domain-allowlist auto-provision creates users that skip the invitation flow's email verification, quota checks, or the demo-account lock added in v1.1.0; email case/whitespace variants (`Alice@X.fr` vs `alice@x.fr`) create duplicate accounts.
- **Prevention:** normalize email (lowercase, trim) at every entry point — one shared helper used by password, invitation, and OIDC paths; allowed-domain check on the exact registered domain (suffix match on `.` boundary — `evilcorp.com.attacker.tld` must fail); auto-provisioned users go through the same `create_user` store path with role default `reader`, never a shortcut insert.
- **Phase:** SSO.

---

## B. Read-only REST API

### B1. REST endpoints bypassing the vault ACL that the web layer enforces
The web layer gates via router middleware (`/vault/{slug}/...` + `require_admin`), and MCP re-scopes per tool (`tools_are_acl_scoped` test). A new `/api/v1/...` router mounted outside the protected layer — or scoping "list vaults" but not re-checking on "read note" — leaks every vault to any token holder. Search is the classic hole: searching across *all* vault paths then filtering results after the fact (or forgetting to) leaks content snippets from private vaults.
- **Warning signs:** new router not under the same `route_layer`/auth extractor as MCP's scoping; a search implementation that iterates `vaults()` instead of `accessible_vaults(user)`; ACL checked on list but path-based read (`GET /api/v1/vaults/{slug}/notes/{path}`) resolving paths without an ACL re-check; tests only cover the happy single-vault case.
- **Prevention:** reuse the exact ACL function MCP uses (single source of truth), apply it per-request to *every* endpoint including search and read; canonicalize the note path and reject anything escaping the vault root (`..`, absolute paths, symlinked chapters); port `tools_are_acl_scoped` to a REST test matrix (reader/admin/hidden-share/no-access × each endpoint).
- **Phase:** API.

### B2. API token leakage in logs, errors, and query strings
Tokens are sha256-hashed at rest (good), but the plaintext rides in `Authorization: Bearer` on every request. axum tracing middleware, `Debug` impls on request state, panic dumps, and nginx access logs all happily record headers. Query-param auth (`?token=*** → browser history, Referer leakage, access logs.
- **Warning signs:** any tracing/logging layer that serializes headers; a `token=` query param accepted "for convenience"; token echoed in a 401 body or error page; `#[derive(Debug)]` on a struct holding the bearer value.
- **Prevention:** Authorization header only, never query params; redact `Authorization`/`Cookie` in any request logging (and check nginx logs on staging once); never echo the token after creation (existing "shown once" pattern at `/account/tokens` — keep it); constant-time compare is unnecessary when looking up by sha256 hex (the hash lookup is the compare), but do not add a fallback that compares plaintext.
- **Phase:** API.

### B3. Read-only API accidentally becoming a write path / serving built output instead of source
- **Warning signs:** reusing the WebDAV or `vault_file` handlers under the API auth (they carry different semantics — `vault_file` serves mdBook **build output**, WebDAV reads the **source**); "read note" returning converted HTML when the caller asked for markdown; search hitting the `.morinet-build/` mirror and returning generated SUMMARY artifacts.
- **Prevention:** dedicated handlers that read the vault source tree (same root WebDAV exposes), return raw markdown + frontmatter untouched; exclude `.morinet-build/` from listing and search (WebDAV already distinguishes source vs build dir — reuse that logic).
- **Phase:** API.

### B4. Token auth and cookie auth fighting over the same routes
- **Warning signs:** `/api/v1` accepting both cookie and Bearer, so a CSRF'd browser request can hit it with the ambient cookie; or Bearer requests still tripping the login-redirect middleware and getting HTML back instead of 401 JSON.
- **Prevention:** `/api/v1` = Bearer-only (401 JSON, never redirect); cookie session never valid on `/api/v1`; if CSRF protection exists for cookie routes, it stays irrelevant because the API rejects cookies outright.
- **Phase:** API.

---

## C. Conflict detection (ETag / If-Match)

### C1. Which layer owns the ETag? Source vs mdBook build output — get this wrong and the whole feature is theater
WebDAV PUT writes **source** markdown; the build pipeline regenerates `.morinet-build/` + served HTML. An ETag on the *served* page is useless for conflict detection on the *source*: mdBook rebuilds change output bytes even when the source didn't (build timestamps, generated anchors), and the client editing via WebDAV never sees the source's ETag if only the HTML layer emits one.
- **Warning signs:** ETag logic added to `vault_file` (served output) instead of the WebDAV/GET path on source; ETag derived from mtime (1s resolution, survives `git clone`/sync rewrites — GitHub-source vaults get their mtimes reset on every Refresh, producing phantom conflicts or phantom "unchanged"); hash computed on the converted mirror instead of the raw source bytes.
- **Prevention:** the **source file's content hash (sha256, strong)** is the single canonical validator, computed on raw bytes at GET/PROPFIND time and re-checked atomically at PUT. Same hash feeds the upload-time check (Phase 14 upload path) so WebDAV and browser upload share one conflict model. Never `W/` (see C2), never mtime.
- **Phase:** conflict.

### C2. Weak ETags and If-Match semantics — `W/"x"` never satisfies If-Match
RFC 9110 §13.1.1: If-Match MUST use **strong comparison**; a weak validator (`W/`-prefixed) never matches, even against itself — a server that stores `W/"abc"` and receives `If-Match: "abc"` must 412. Common mistakes: emitting weak ETags (many static-file layers default to `W/`), stripping `W/` to "make it match" (creates phantom conflicts against conforming clients), or ignoring `If-Match` entirely when the client sends it.
- **Warning signs:** any `W/` in responses on the WebDAV/source surface; code that `.trim_start_matches("W/")` before comparing; a PUT handler that parses `If-Match` and drops it on parse error.
- **Prevention:** emit strong ETags only (quoted sha256 hex); on `If-Match` mismatch → `412 Precondition Failed` with a body naming the current ETag; on `If-None-Match: *` for PUT → 412 if the file exists (create-only semantics, prevents clobbering a file the client thought was new). Evaluate the precondition **before** touching the file, and fail closed on malformed headers (400), never silently write.
- **Phase:** conflict.

### C3. TOCTOU between the precondition check and the write; SQLite vs filesystem non-atomicity
Checking the hash then writing is a race: two concurrent PUTs both pass the check, second clobbers first. The vault_changes table (fed at WebDAV PUT, Phase 13) is SQLite while the file write is the filesystem — a "check DB hash, write file, update DB" sequence has windows where they disagree.
- **Warning signs:** `read → compare → write` with no lock around the critical section; conflict state stored only in SQLite while the file is the truth (or vice versa); concurrent PUTs to the same path producing interleaved/truncated files (no atomic write).
- **Prevention:** serialize per-path (or per-vault) writes with an in-process lock (tokio `Mutex` keyed by vault path — MoriNet is a single binary, an in-process lock is sufficient); write via temp-file + atomic `rename` (same discipline as the deploy `mv` rule); recompute the hash from the bytes just written for the vault_changes record. Decide and document the no-`If-Match` policy: v1.2 should **allow** unconditional PUT (backward compat with existing WebDAV clients — Obsidian plugins, rclone) while honoring `If-Match` when present; make that a config flag if teams want strict mode.
- **Phase:** conflict.

### C4. Upload-time hash check racing the build pipeline and the Refresh path
The upload endpoint (Phase 14) and Refresh (git sync) also rewrite source files. A conflict check that only guards WebDAV PUT leaves Refresh (git pull overwrites local edits silently) and re-upload as clobber paths.
- **Warning signs:** hash check only in `webdav.rs`; Refresh overwriting files whose hash changed since last build without warning; the vault build lock (quota/lock gates) not covering the PUT path, so a build reads a half-written file.
- **Prevention:** one shared "write source file" helper used by WebDAV PUT, upload, and Refresh, each taking the same per-vault lock and each recording pre/post hashes into vault_changes; Refresh detects local modifications (compare working tree vs last-synced hash) and refuses or reports instead of silent overwrite.
- **Phase:** conflict.

### C5. SQLite write contention on group sync (and every new v1.2 write path)
Every SSO login now writes: upsert identity, sync group memberships, maybe create user. Concurrent logins (team start-of-day) on SQLite serialize hard; default busy behavior surfaces as `SQLITE_BUSY` errors at login.
- **Warning signs:** `database is locked` in logs under concurrent login/upload/PUT load; group sync as N separate UPDATE/DELETE/INSERT statements outside one transaction (partial sync if one fails mid-way); long-running read (search, stats) holding a write transaction open.
- **Prevention:** WAL mode + `busy_timeout` in the SQLite options (verify what sqlx pool config already sets); group sync as a **single transaction** with an upsert, and skip the write entirely when the computed membership set equals the stored one (logins are mostly no-ops); keep transactions short — never hold a DB transaction across a filesystem write or `mdbook build` subprocess call; one shared pool, not per-feature pools.
- **Phase:** SSO (group sync) + conflict (vault_changes writes) — verify once at the store layer.

### C6. WebDAV client reality: most clients won't send If-Match
rclone, macOS Finder, Obsidian WebDAV plugins largely ignore ETags. If the feature is designed as "clients must send If-Match," it silently protects nobody; if designed as "always reject without If-Match," it breaks every existing user.
- **Warning signs:** UAT plan that only tests with `curl -H 'If-Match: ...'`; no PROPFIND ETag exposure (clients can't discover the validator); docs promising conflict protection that no shipping client uses.
- **Prevention:** expose ETag on GET **and** PROPFIND (`getetag` property), honor `If-Match`/`If-None-Match` when present, keep unconditional PUT working by default, and position the browser upload check as the guided path. Test with at least one real client (rclone `--vfs-cache-mode writes` or Cadence/obsidian-webdav) not just curl.
- **Phase:** conflict.

---

## Cross-cutting

### X1. Hand-rolling the OIDC protocol instead of using a maintained crate
Every pitfall in section A is already solved (and regression-tested) in `openidconnect`/`oauth2` crates; hand-rolled flows historically ship missing nonce/iss/aud/alg checks. **Phase:** SSO. **Prevention:** crate + thin MoriNet glue; the crate's `ValidationOptions` pin `iss`/`aud`/nonce explicitly.

### X2. Config surface drift between staging and prod
OIDC breaks loudly only in prod: unregistered redirect_uri, wrong `issuer` string (trailing slash!), claim names differing between the team's Keycloak realm and the demo IdP. **Warning signs:** "works locally, 400 at IdP" reports. **Prevention:** a `/admin` (or `/health`-adjacent) OIDC diagnostics view that runs discovery, fetches JWKS, and shows the last token's claims (redacted) so admins self-diagnose; issuer string compared exactly, documented with trailing-slash note. **Phase:** SSO.

### X3. Test matrix gaps
Each feature's auth surface needs the same matrix: existing password users, OIDC users, dual-identity users (same email both ways), admin vs reader, hidden shares, demo-locked accounts. The demo-lock guard (v1.1.0) must apply to REST reads? (decide explicitly — currently it guards load/upload/edit; a read-only API on a demo account is probably fine but must be a decision, not an oversight). **Phase:** all three.

---

## Priority ranking for the milestone
1. **A1 email auto-link takeover** (data-loss-grade, repeatedly CVE'd)
2. **B1 ACL bypass on REST** (same severity class)
3. **C1/C2 ETag layer + strong-validator correctness** (feature is worthless or actively harmful if wrong)
4. **A5 group-claim privilege escalation / staleness**
5. **A2 state/nonce/PKCE validation** (mostly free if using the crate — X1)
6. **C3/C4 write races**, **C5 SQLite contention**, **B2 token leakage**
7. everything else
