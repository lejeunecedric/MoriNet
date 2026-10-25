# WebDAV

Every vault you can access is also exposed as a WebDAV collection, so you can
browse and edit notes from your file explorer, `cadaver`, `rclone`, or any
WebDAV-capable client — no browser required.

## URL

```text
https://your-morinet.example.com/webdav/<slug>/
```

`<slug>` is the vault's slug, the same one you see in the vault's web URL
(`/vault/<slug>/`). The path addresses the vault's **source** files — the
markdown that gets compiled — not the generated HTML.

## Authentication

WebDAV uses HTTP Basic auth with the same email and password you use to log in
on the website. No separate token. Your access rights per vault apply
identically: a vault you cannot open on the web is invisible over WebDAV too.

## Supported operations

| Method | Supported | Notes |
| --- | --- | --- |
| `PROPFIND` | yes | Directory listings (depth 0 and 1). |
| `GET` | yes | Download any source file. |
| `PUT` | yes | Create or overwrite a file. Requires **writer** access. |
| other verbs | no | `MKCOL`, `DELETE`, `MOVE`, `COPY` are not supported in v1. |

## Reader vs writer

- **Reader**: can list and download (`PROPFIND`, `GET`).
- **Writer**: additionally can upload new or updated files (`PUT`).

Roles are set by the vault's owner when sharing — see
[Sharing & roles](sharing-and-roles.md).

## Edits go live after a rebuild

A `PUT` writes the source file and then rebuilds the book, so the served HTML
reflects your new note right away. If the rebuild fails (for example the
server cannot find `mdbook` — see the [FAQ](faq.md)), the file is saved but
the rendered pages stay on the last successful build until the next Refresh.

## Example

Mount it on Linux with `davfs2`:

```bash
sudo mkdir /mnt/handbook
sudo mount -t davfs https://your-morinet.example.com/webdav/handbook/ /mnt/handbook
```

Or from the command line with `curl`:

```bash
curl -u you@example.com -X PUT \
  --data-binary @new-note.md \
  https://your-morinet.example.com/webdav/handbook/src/new-note.md
```

Remember that a new file only appears in the book's table of contents if the
source's `SUMMARY.md` lists it (mdBook rule for native books).
