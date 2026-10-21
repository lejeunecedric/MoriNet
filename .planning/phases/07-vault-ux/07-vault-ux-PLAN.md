# Phase 7 — vault-ux

> Goal: adding a local vault must not require typing a path on the server. An admin clicks **Browse**, walks the directories, picks one.

**User decision (2026-10-21)**: the picker browses **the server's filesystem**, not the admin's machine. A vault is a path on the host running MoriNet; a browser-side folder picker would upload a path string that doesn't exist there.

**REQ**: REQ-UX-001..012

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

| kind      | fields                                                            |
|-----------|-------------------------------------------------------------------|
| `local`   | slug, name, **Browse** button + hidden path                         |
| `local` + import | same, plus an **Import** button (browser-side folder picker) |
| `github`  | slug, name, URL                                                     |

- Clicking **Browse** opens a modal listing `dirs[]`; click a row → descend; click **⬆ up** → parent; a breadcrumb shows the path.
- Clicking **Use this folder** fills the hidden `path` field and closes the modal.
- Vanilla JS inline (no framework — AD-9); the modal is one `<dialog>` element.

### Import (REQ-UX-008..012)

**User decision (2026-10-21): an import CREATES A NEW VAULT. It does not merge into an existing one.**

That changes the model, and simplifies it:

1. The admin clicks **Import** → a browser-side folder picker (`<input type="file" webkitdirectory multiple>`).
2. The browser sends the files, each with its `webkitRelativePath` (`brain/a.md`, `brain/brain/c.md`).
3. The server derives the **slug and the destination folder from the selected folder's top-level name** (`brain` → `<data_dir>/vaults/brain`), creates it, and registers the vault.
4. `book.toml` + `src/SUMMARY.md` are scaffolded when missing, so the vault is immediately a valid mdBook (the vault contract — see HANDOFF).

The browser's path is used only for the **relative layout inside** the vault; the absolute destination is always server-controlled. The browser never dictates where the vault lives on the server.

Consequences:

- **No merge, so no conflict policy in the common case.** The destination is new. A guard still refuses to import into a slug that already exists (`409`) rather than overwriting someone's vault.
- If the destination folder somehow exists with files (re-import of the same name), we refuse with a clear message instead of clobbering.
- Every uploaded name is sanitised (`..`, absolute, NUL refused) and intermediate dirs are created only inside the destination.
- Body capped (default 32 MiB) → `413`. `mdbook build` runs afterwards if `book.toml` is present.

### Validation on create

Selecting a directory with no `book.toml` → a readable message on the vault page (not a raw 500). `LocalSource::sync` already checks `is_dir`; we add the `book.toml` hint.

---

## Bites

1. `GET /admin/browse` — endpoint + JSON + root confinement + tests (curl).
2. Modal markup + CSS in the vault form (browser screenshot).
3. JS: descend / up / breadcrumb / select → fills the field (browser click-through).
4. Hide the path field for GitHub, show for local.
5. Readable error when the folder is not a vault; verify create still works end to end.
6. `POST /admin/import` — multipart upload, slug derived from the folder name, sanitisation, refuse-if-exists, size cap + tests.
7. Import button wiring + `{ imported, skipped }` report shown in the UI (browser pass).

---

## Verification

- Unit + e2e HTTP: admin can browse, cannot escape the root, non-admin is refused.
- **Browser pass**: click Browse → screenshot → descend → up → select → form filled → create vault. Screenshots at each step (AD-12 lesson: styled HTML needs a real render).
- **Import pass**: pick a local folder → a NEW vault appears on the server under that name → files land in it → `{ imported }` reported → importing the same name again is refused → a `..` filename is refused.