# Phase 15: etag-conflict — Conflict-safe co-editing (CONF-01..05) - Context

**Gathered:** 2026-10-30
**Status:** Ready for planning

<domain>
## Phase Boundary

Stop silent last-write-wins clobbering on vault source files:

- Strong content-hash ETags (sha256 of raw **source** markdown bytes) on WebDAV GET + `getetag` in PROPFIND — CONF-01
- `If-Match` enforcement on PUT (412 on mismatch, nothing written / no rebuild / no vault_changes row; malformed precondition → 400) — CONF-02
- `If-None-Match: *` create-only semantics; successful conditional PUT returns the new ETag — CONF-03
- Upload-time conflict detection with per-file conflict list + explicit force-overwrite, recorded in `vault_changes` — CONF-04
- Unconditional writes (no precondition headers, plain uploads) behave exactly as before — rclone/Obsidian regression green — CONF-05
- Race-safe check→write: per-vault in-process write lock + temp-file/atomic-rename via ONE shared "write source file" helper used by both WebDAV PUT and upload (pitfall C3/C5)

**Not in scope:** LOCK/UNLOCK locks, CRDT/real-time merge, conflicted-copy files inside the source tree, strict-mode enforcement for clients that never send If-Match (deferred, see Deferred Ideas).

</domain>

<decisions>
## Implementation Decisions

### Upload update path + conflict UX (CONF-04)
- **D-01** Update entry point: re-uploading with the **same vault name** on the home upload form updates the existing uploaded vault. The conflict gate replaces today's "the slug is already taken — rename the vault" error for this case. No new UI surface (no "Update" button on cards/edit page).
- **D-02** Conflict definition (merge semantics, no deletes): a conflict is a file present on **both** sides (incoming tree vs current `uploads/<slug>` tree) with **different content** (sha256 compare). Local-only files are added; server-only files are left untouched. Force-overwrite never deletes server files missing from the upload.
- **D-03** Force submission is **JS two-phase**: the home page's upload form submits via `fetch`; on a conflict response (JSON) the page renders the conflict list plus an "Upload anyway" button that re-sends the **same in-memory FileList** with a `force=1` field — the user never re-picks the folder. Non-JS/simple fallback may use a plain re-submit with a force checkbox (planner's discretion), but the two-phase fetch flow is the primary UX.
- **D-04** Re-upload scope: **uploaded vaults only** (`data_dir/uploads/<slug>`), **owner-only** (same gate style as Phase 14: locked-account refusal first, then ownership). Git-sourced vaults keep Refresh-from-upstream semantics — no re-upload onto them.

### Force-overwrite mechanics + audit (CONF-04)
- **D-05** Storage: migration `migrations/0011_*.sql` adds to `vault_changes`: `forced INTEGER NOT NULL DEFAULT 0` and `actor TEXT NOT NULL DEFAULT ''`. The kind vocabulary stays **exactly** `added`/`modified`/`removed` (Phase 13 D-08 CHECK constraint untouched). Force-overwrites are recorded as `modified` rows with `forced = 1`, all rows of one force operation sharing one `event_id`.
- **D-06** Actor: populate `actor` (user email) on force-overwrites, WebDAV PUTs, and upload updates. Existing best-effort rule still applies (log-and-swallow errors, `eprintln!("vault-changes: …")`, no tracing dependency — Phase 13 D-04).
- **D-07** Conflict page is the **rich** variant: per-file list where each conflicting file links to its `vault_changes` entries (who changed it, when — now possible via D-06 actor), plus count and the "Upload anyway" action. Render inside the existing `app_page()` shell / card styling — no ad-hoc CSS (Phase 9b rule).

### Claude's Discretion
- Strict-mode config flag (reject unconditional PUT): **deferred** — v1.2 ships unconditional PUT allowed by default (CONF-05 backward compat, pitfall C6). No config flag this phase.
- WebDAV response details: 412 body should name the current ETag (pitfall C2 prevention); status on successful conditional PUT (200 vs today's 201) and 400 body wording per RFC 4918/9110 — planner/executor decide, must stay RFC-correct and backward compatible.
- PROPFIND `getetag` cost: hash-on-read per file is acceptable (markdown is small); caching is an optimization the executor may skip.
- Exact helper name for the shared "write source file" (temp-file + atomic rename + hash), per-vault lock map type (tokio Mutex keyed by vault path), `If-Match` parser details (`*`, quoted tags, comma lists, no `W/` acceptance), route/field names, store method names.

### Folded Todos
None — no pending todos matched this phase.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements & roadmap
- `.planning/ROADMAP.md` §"Phase 15: etag-conflict" — the 7 success criteria + pitfalls-to-avoid list (C1/C2/C3/C6)
- `.planning/REQUIREMENTS.md` — CONF-01..05 (lines ~30-34); CONF-F01/F02 explicitly deferred
- `.planning/research/PITFALLS.md` §C (C1-C3, C5, C6) — ETag layer, strong-validator semantics, TOCTOU/atomic-write prevention
- `.planning/research/FEATURES.md` §"Feature Area 3" — RFC 4918 §9.9 / RFC 9110 §13.2 semantics, authz-before-precondition rule, upload conflict UX conventions
- `.planning/research/SUMMARY.md` — build-order rationale (etag_of() helper feeds Phase 16)

### Existing code to extend
- `src/server.rs` `webdav_entry` (~:796) — GET (~:862), PUT (~:876-927), PROPFIND branch (~:837); `rebuild_book` (~:936) must fire only on successful writes
- `src/webdav.rs` — `prop_entry` (~:136) / `multistatus` (~:181) / `file_propfind_response` (~:255) gain `getetag`; new conditional-header parsing lives here
- `src/server.rs` `upload_local_vault` (~:2942) — Phase 14 pipeline to insert the conflict gate into (after gates, before write/build); slug-collision pre-check (~:3005) becomes the update path per D-01
- `src/store.rs` `log_vault_change` (~:1439), `prune_vault_changes` (~:1486), `recent_vault_changes` — audit writes + conflict-page queries
- `migrations/0010_vault_changes.sql` — current schema + CHECK constraint (untouched); new migration 0011 is additive-only
- `src/server.rs` `record_vault_changes` (~:2690) — refresh-path change recording (actor now available there too)

### Prior phase context
- `.planning/milestones/v1.1.0-phases/MORI-13-wiki-changes-log-v1-1-0/13-CONTEXT.md` — D-03/D-04/D-08 (schema style, best-effort logging, locked kind vocabulary)
- `.planning/milestones/v1.1.0-phases/MORI-14-local-vault-upload-v1-1-0/14-CONTEXT.md` — D-02/D-03/D-05 (multipart gates, path sanitization, quota-before-disk) — the update path must reuse these gates

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `sha2` + `hex` are already dependencies (used for API-token hashing in store.rs) — the `etag_of()` sha256-hex helper needs no new crate.
- `webdav::safe_join` / `source_dir_of` — path safety and source-dir resolution already in place for every verb.
- `log_vault_change` + one-event-per-PUT spawn pattern (server.rs ~:908-922) — extend with actor/forced, keep fire-and-forget.
- `upload_local_vault` gate order (locked → name → quota → exclusive dir create) — the update path mirrors it minus quota/create.
- Phase 14 home-page JS (locked popup script ~:1984, file input handling) — same page gains the two-phase fetch upload JS.

### Established Patterns
- Best-effort audit writes: `tokio::spawn` + `eprintln!` swallow (Phase 13 D-04) — vault_changes must never fail the request.
- `error_page(...)` for browser-facing errors; WebDAV surface answers bare status codes (plus small bodies).
- No tracing crate; no global CSS edits without an Impeccable pass (Phase 9b).
- Tests live inline in `src/server.rs` / `src/store.rs` (`#[tokio::test]` modules, e.g. `upload_*` tests ~:7581+).

### Integration Points
- `webdav_entry` GET/PROPFIND/PUT branches — ETag emission + precondition evaluation (after the existing ACL check: authz-before-precondition).
- `upload_local_vault` — conflict gate between the slug pre-check and the file-write loop; force flag as a multipart text field.
- `vault_stats` "Wiki changes" section (Phase 13) — may surface `forced` badges / actor if trivial (planner discretion).
- New shared write helper consumed by BOTH `webdav_entry` PUT and the upload writer — single atomic-rename + hash implementation (roadmap criterion 7).

</code_context>

<specifics>
## Specific Ideas

- "One hash, three consumers" (research FEATURES.md): the same sha256 powers ETags, vault_changes conflict checks, and (Phase 16) REST read validators — keep ONE `etag_of(bytes)`-style helper.
- Conflict page should turn a dead-end error into an actionable prompt: each conflicting file links to who/when from vault_changes (D-07).

</specifics>

<deferred>
## Deferred Ideas

- **Strict-mode config flag** (reject unconditional PUT when a team wants enforced If-Match) — deferred; revisit if real teams ask. Pitfall C6 warns most shipping clients never send If-Match.
- **CONF-F01 file/folder locks (LOCK/UNLOCK)** — owner-chosen locks, out of v1.2 (REQUIREMENTS.md).
- **CONF-F02 real-time collaborative editing (CRDT)** — out of scope.
- **Rich conflict-page extras** (diff viewer per file) — beyond the linked wiki-changes entries in D-07.

</deferred>

---

*Phase: 15-etag-conflict*
*Context gathered: 2026-10-30*
