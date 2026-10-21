# Phase 7 — vault-ux

> Goal: adding a local vault must not require typing a path on the server. An admin clicks **Browse**, walks the directories, picks one.

**User decision (2026-10-21)**: the picker browses **the server's filesystem**, not the admin's machine. A vault is a path on the host running MoriNet; a browser-side folder picker would upload a path string that doesn't exist there.

**REQ**: REQ-UX-001..007

---

## Design

### Endpoint

`GET /admin/browse?path=<dir>` → JSON, admin-only (behind the existing `require_admin`).

```json
{ "current": "/data/vaults",
  "parent": "/data",
  "dirs": ["demo", "scratch"] }
```

- Only **directories**, never files. Sorted, hidden entries (`.` prefix) skipped.
- `parent` is `null` at the root.
- Traversal above the configured root → `400`. Unauthenticated / non-admin → `401`/`403`.

### Where the root comes from

The browsable root is the configured `storage.data_dir` (e.g. `./data`) — the admin should not be able to browse `/etc` or `/home`. Going **up** stops at that root.

### UI

On `/admin/vaults`, the "Add a vault" form:

| kind      | fields                                    |
|-----------|-------------------------------------------|
| `local`   | slug, name, **Browse** button + hidden path |
| `github`  | slug, name, URL                            |

- Clicking **Browse** opens a modal listing `dirs[]`; click a row → descend; click **⬆ up** → parent; a breadcrumb shows the path.
- Clicking **Use this folder** fills the hidden `path` field and closes the modal.
- Vanilla JS inline (no framework — AD-9); the modal is one `<dialog>` element.

### Validation on create

Selecting a directory with no `book.toml` → a readable message on the vault page (not a raw 500). `LocalSource::sync` already checks `is_dir`; we add the `book.toml` hint.

---

## Bites

1. `GET /admin/browse` — endpoint + JSON + root confinement + tests (curl).
2. Modal markup + CSS in the vault form (browser screenshot).
3. JS: descend / up / breadcrumb / select → fills the field (browser click-through).
4. Hide the path field for GitHub, show for local.
5. Readable error when the folder is not a vault; verify create still works end to end.

---

## Verification

- Unit + e2e HTTP: admin can browse, cannot escape the root, non-admin is refused.
- **Browser pass**: click Browse → screenshot → descend → up → select → form filled → create vault. Screenshots at each step (AD-12 lesson: styled HTML needs a real render).