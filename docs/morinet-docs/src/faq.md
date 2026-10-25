# FAQ

## Why do I have to press Refresh after changing a theme or an asset?

Because the pages you read are pre-compiled HTML. MoriNet builds each vault
with `mdbook build`, and themes, CSS and JS assets are baked in **at build
time**. Editing a theme file or a custom stylesheet changes the source, not
the served copy — the next Refresh (or the next GitHub sync) rebuilds the book
and your change appears. The same applies to notes edited through
[WebDAV](webdav.md): a `PUT` triggers a rebuild automatically, but if the
build fails the old pages stay up until the next successful Refresh.

## The server says `mdbook not found in PATH`

Every vault is compiled by calling the external `mdbook` binary. It must be
installed **on the server machine** and reachable from the server process's
`PATH` — installing it on your laptop does not help. With a systemd service,
make sure the binary's directory (often `~/.cargo/bin`) is in the service's
`PATH`. After installing mdbook, press Refresh again.

## How many personal vaults can I create?

Personal vaults are limited per group. The built-in `free` group allows **2**
personal vaults per account; the `admin` group is unlimited. An admin can move
accounts between groups. Deleting a personal vault frees its slot
immediately.

## What happens when the owner deletes a vault that was shared with me?

The vault disappears for everyone: it is removed from your "Shared with me"
list, its WebDAV and [MCP](mcp.md) access stop working, and all shares of it
are deleted with it. There is no separate "orphaned share" state — if you need
the content to survive, ask the owner to export or transfer it before
deleting.

## I edited a file over WebDAV but the website still shows the old text.

Check three things: the file must live in the vault's source tree (usually
under `src/`); for native mdBook sources a *new* file must be listed in
`src/SUMMARY.md` to appear in the table of contents; and the rebuild after
upload may have failed — press Refresh and look for an error, or see the
mdbook answer above.

## Where does MoriNet keep its data?

Everything — the SQLite database, cloned GitHub repos, personal vault
uploads, and the built-in documentation vault — lives under `[storage]
data_dir` (see the [Config reference](config-reference.md)). Backing up that
one directory backs up the whole instance.

## Is this documentation editable?

Yes. The pages of this vault are ordinary markdown files under
`<data_dir>/docs/morinet-docs/src/`. They are written once, on the very first
boot of your instance, and never overwritten afterwards — your edits survive
restarts. Edit them, then press Refresh to republish.
