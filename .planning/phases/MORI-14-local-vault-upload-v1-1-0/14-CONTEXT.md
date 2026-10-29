# Phase 14: local-vault-upload (v1.1.0) - Context

**Gathered:** 2026-10-29
**Status:** Ready for planning

<domain>
## Phase Boundary

A signed-in user loads a vault from their OWN disk (ROADMAP Phase 14,
REQ-LOC-001..002):

1. Home page gains a "Local vault" action: client-side directory picker
   (`<input type="file" webkitdirectory multiple>`) + name field → upload
   the markdown tree.
2. The uploaded tree is stored under the owner's space and goes through the
   normal convert/build/register pipeline (same quota + ACL + locked-account
   gates as the GitHub load).

Out of scope: incremental re-upload / refresh of uploaded vaults (no upstream
— Refresh keeps its existing "no upstream to refresh" answer), zip/tar
archives, admin-side upload (admin local dirs already exist), editing
uploaded files through the UI beyond what WebDAV already provides.
</domain>

<decisions>
## Implementation Decisions

### Transport (REQ-LOC-001)
- **D-01** Multipart upload: `POST /vaults/upload` (new route next to
  `/vaults/create`), axum `Multipart` extractor — add the `multipart` feature
  to the existing axum 0.8 dependency (no new crate). The browser sends one
  part per file; each part's filename carries the client-relative path
  (e.g. `MyWiki/index.md`).
- **D-02** Body limit: axum's default 2 MB body limit is far too small for a
  wiki tree — raise `DefaultBodyLimit` to 256 MB for the upload route only
  (scoped layer, not global). Enforce in code: max 5 MB per file and max
  2000 files; a violation aborts with a clear error page and stores nothing.
- **D-03** Path sanitization (hard security requirement): every client-supplied
  filename is untrusted. Reject (abort whole upload): absolute paths, any `..`
  segment, backslashes, drive-letter prefixes. Strip the top-level picked
  directory name. Write only inside the target dir via joined+canonicalized
  prefix check. Skip junk: `.git/` entries, dotfiles (`.DS_Store` etc.), and
  files without a `.md`/`.markdown` extension are NOT written (only recorded
  as ignored in the response flow). Require at least one markdown file, else
  error "no markdown files found".

### Storage + pipeline (REQ-LOC-002)
- **D-04** Store the tree at `data_dir/uploads/<slug>/` (owner space, sibling
  of `repos/`), then run the SAME shared build dispatch as the GitHub load
  (`LocalSource` in sources.rs converts llm-wiki trees / builds native books,
  AD-14) and register with `create_personal_vault` — identical ACL/owner
  semantics. No `vault_source` URL is set, so Refresh naturally answers
  "this vault has no upstream to refresh" (existing behavior, keep).
- **D-05** Gates identical to `load_personal_vault`: `is_locked` refusal first
  (same "demo account can't add or suppress vaults" message), then quota check
  BEFORE touching disk (no partial writes at limit), same slug derivation
  (lowercase [a-z0-9-]) and UNIQUE-slug error ("rename the vault"). If the
  build fails, remove the `uploads/<slug>` dir (no orphan trees).
- **D-06** Home page UX: a "Local vault" form (name input + directory picker +
  submit) beside the existing GitHub load form; the existing locked-account
  popup script must also intercept this form's submit (same alert string).
  Client-side: show the picked file count before submit (small inline JS).

### Claude's Discretion
- Exact route name/form layout/labels, per-part read strategy (stream to disk
  vs buffer), ignored-file reporting, JS details.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/ROADMAP.md` §"Phase 14: local-vault-upload" — criteria REQ-LOC-001..002
- `.planning/REQUIREMENTS.md` — REQ-LOC-001/002
- `src/server.rs` `load_personal_vault` (~:2732) — the GitHub-load pipeline to
  mirror (locked gate :2737, quota :2755-2776, slug :2778-2786, build+register
  :2788-2814); route table ~:183; home page form ~:577; locked popup JS ~:1988
- `src/sources.rs` `LocalSource` (~:70) + `VaultSource::build` — the shared
  convert/build dispatch (AD-14)
- `src/store.rs` `create_personal_vault` / `seed_vault` (~:150) — registration
- `.planning/phases/MORI-12-demo-lock-vault-display-options-v1-1-0/12-CONTEXT.md`
  — locked-account gate semantics (D-02: load refused server-side + popup)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `load_personal_vault` is the template: locked gate → quota → slug → build →
  `create_personal_vault` → redirect. The upload handler should read like it.
- `LocalSource::build` already converts an on-disk llm-wiki dir into a book —
  the upload just needs to land a clean tree on disk first.
- Locked-popup JS on the home page already intercepts the load form — extend
  the selector, don't duplicate the script.

### Established Patterns
- All pages via `app_page()`; error pages via `error_page(...)`; neutral test
  creds (test-pass-9f3a); axum 0.8 extractors; sqlx bind params only.
- axum default `DefaultBodyLimit` is 2 MB — silent 413 if not raised.

### Integration Points
- New route in `build_router` next to `/vaults/create`.
- Home page index handler (~:516) — new form + JS flag.
- `data_dir/uploads/` — new sibling of `repos/`; delete flow must also clean
  it (check `delete_my_vault`/`admin_delete_vault` repo-dir removal and mirror).

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches.

</specifics>

<deferred>
## Deferred Ideas

- Re-upload to update an existing local vault (needs an "update" verb) —
  future phase.
- Zip upload with size preview — future phase.

</deferred>

---

*Phase: 14-local-vault-upload-v1-1-0*
*Context gathered: 2026-10-29*
