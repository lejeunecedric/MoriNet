# Sharing & roles

Vaults are private by default: only their owner (and admins) can see them.
Sharing is how a vault appears in someone else's vault list.

## My vaults vs Shared with me

The home page has two sections:

- **My vaults** — vaults you own or that were registered by the admin. You
  control them: refresh, rename, delete, share.
- **Shared with me** — vaults other people shared with you by email. You can
  read them (and write, if the owner allowed it), but you cannot change who
  else sees them unless re-sharing is allowed.

## Sharing a vault

Open the **Share** button on a vault you own, enter the person's email, and
pick a role. They see it on their next page load — no invitation click
needed.

## Roles

| Capability | Reader | Writer |
| --- | --- | --- |
| Browse the rendered book | yes | yes |
| Read source files over [WebDAV](webdav.md) | yes | yes |
| Upload / edit files over WebDAV (`PUT`) | no | yes |
| Refresh (rebuild) the vault | no | yes |
| Rename or delete the vault | owner only | owner only |
| Share it further | see *allow re-share* | see *allow re-share* |

## Allow re-share

By default a sharee cannot pass the vault on. The owner can tick
**allow re-share** on a share; that person can then share the vault with
others (as reader or writer). Re-shares are still bounded by the original
role: a reader with re-share enabled cannot grant writer access they don't
have themselves.

## Removing a share

Two different actions, easy to confuse:

- **Remove from my view** (shown to the person the vault was shared with):
  hides the vault from *their* "Shared with me" list. The share itself still
  exists — the owner's share list still shows it, and re-sharing to the same
  email makes it visible again.
- **Revoke** (owner): deletes the share entirely. The other person loses
  access immediately, whether or not they had hidden it.

## Sharing with someone who has no account yet

Sharing to an email that is not registered creates a *pending* share. The
moment someone registers (or is invited) with that exact email address, the
share resolves automatically and the vault appears in their list.

## Loading your own vault

Signed-in users can load a personal vault from the home page, within their
group's quota:

- **From GitHub** — paste the repository URL; it is cloned, converted and
  rebuilt automatically on the background sync, and can be **Refresh**ed on
  demand.
- **From your machine** — the *Local vault* form picks a directory in your
  browser and uploads its markdown files. Uploaded vaults have no upstream:
  Refresh is not available for them — to update one, delete it and upload it
  again (under the same name once deleted).

## Vault display options

The owner can tune how a vault renders from its **Edit** page:

- **Keep page numbers** — off by default, which hides mdBook's own sidebar
  chapter numbers; tick it to show them again.
- **Theme** — pick the mdBook theme (auto, light, navy, rust, coal, ayu).

Both apply at serve time to the vault's pages — no rebuild needed.

## Statistics & wiki changes

Every owner (and admin) sees a **Statistics** page per vault: access hits per
day, top pages, and a **Wiki changes** section listing what each Refresh
actually changed — files added, modified or removed, newest events first.
Change logging is best-effort telemetry: it never affects the Refresh itself,
and entries are kept for 90 days.

## Instance-wide vaults

Some vaults — like this documentation — are marked global by the admin and
granted read access to every group. They appear in "Shared with me" for every
account with no per-user sharing. See the [FAQ](faq.md) for what happens when
a shared vault is deleted.
