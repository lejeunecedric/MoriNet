# Phase 15: etag-conflict - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-30
**Phase:** 15-etag-conflict — Conflict-safe co-editing (CONF-01..05)
**Areas discussed:** Upload update path + conflict UX; Force-overwrite mechanics + audit

---

## Area selection (gray areas presented)

| Option | Description | Selected |
|--------|-------------|----------|
| Upload update path + conflict UX | How re-upload onto an existing vault works; what counts as a conflict | ✓ |
| Force-overwrite mechanics + audit | Conflict page content, force flag transport, vault_changes recording | ✓ |
| Unconditional-write strict-mode flag | Ship config flag now or defer | — (deferred to Claude's discretion) |
| WebDAV response details + PROPFIND cost | 412 body, 200 vs 201, PROPFIND re-hash vs cache | — (deferred to Claude's discretion) |

**Notes:** Context was pre-grounded from `.planning/research/` (PITFALLS.md §C, FEATURES.md Area 3) and code scout: `upload_local_vault` (server.rs:2942) currently refuses existing slugs outright — no update path existed.

## Upload update path + conflict UX

| Option | Description | Selected |
|--------|-------------|----------|
| Same name updates | Same name on home upload form updates the existing uploaded vault; conflict gate replaces "rename the vault" error; no new UI surface | ✓ |
| Explicit Update button | "Update" on vault edit page/card bound to slug; home form stays create-only | |
| You decide | Planner picks | |

| Option | Description | Selected |
|--------|-------------|----------|
| Differing files only | Conflict = present on both sides with different content; local-only added; server-only untouched (merge, no deletes) | ✓ |
| Replace semantics | Server files missing from upload also conflict; force deletes them | |

| Option | Description | Selected |
|--------|-------------|----------|
| JS two-phase | fetch POST; conflict JSON → list + "Upload anyway" re-sends same in-memory FileList with force=1 (no folder re-pick) | ✓ |
| Server HTML page + re-pick | HTML conflict page; user re-picks folder and ticks "overwrite anyway" | |

| Option | Description | Selected |
|--------|-------------|----------|
| Uploaded vaults only | uploads/<slug> vaults, owner-only; git-sourced keep Refresh semantics | ✓ |
| Any personal vault | Re-upload onto git-sourced vaults too (fights Refresh) | |

**User's choice:** all four recommended options (same-name update, merge semantics, JS two-phase, uploaded vaults only).

## Force-overwrite mechanics + audit

| Option | Description | Selected |
|--------|-------------|----------|
| Additive 'forced' column | Migration 0011 adds forced INTEGER DEFAULT 0 (+ actor TEXT); kind vocabulary stays added/modified/removed; force-overwrites = 'modified' rows with forced=1 | ✓ |
| New kind 'overwritten' | Relax CHECK constraint; breaks locked kind vocabulary | |
| No schema change | Plain 'modified' rows under one event_id; indistinguishable from normal edits | |

| Option | Description | Selected |
|--------|-------------|----------|
| Actor column | migration adds actor TEXT (email); populate on force-overwrites, WebDAV PUTs, upload updates | ✓ |
| No actor | Keep vault_changes actor-free this phase | |

| Option | Description | Selected |
|--------|-------------|----------|
| Simple list | Count + paths + "Upload anyway" in existing card styling | |
| Rich page | Also link each conflicting file to its wiki-changes entries (who changed it, when) | ✓ |

**User's choice:** additive forced+actor columns (kind vocabulary preserved), actor populated everywhere, rich conflict page. Note: the rich page was the non-recommended option — user explicitly chose the wiki-changes linkage.

## Claude's Discretion

- Strict-mode config flag for unconditional PUT — deferred (research C6: most clients never send If-Match).
- 412 body wording (name current ETag), 200 vs 201 on successful conditional PUT, 400 body — RFC-correctness required, planner/executor decide.
- PROPFIND getetag hashing cost — hash-on-read acceptable; caching optional.
- Shared write-helper name, lock map type, If-Match parser internals, store method names.

## Deferred Ideas

- Strict-mode config flag (reject unconditional PUT) — future phase if teams ask.
- CONF-F01 locks / CONF-F02 CRDT — already out of v1.2 in REQUIREMENTS.md.
- Diff viewer on conflict page — beyond D-07's wiki-changes linkage.
