# Phase 13 Discussion Log

**Mode:** `--auto` (all gray areas auto-selected, recommended option chosen per question — single pass)
**Date:** 2026-10-29

## Areas discussed & auto-selections

### Change capture location
- Q: Where should the old→new diff be computed? → **Selected:** inside `refresh_my_vault` — read clone HEAD before `src.sync()`, diff trees with git2 after (recommended default).

### Local (non-git) change path
- Q: How to log changes for local/no-upstream vaults? → **Selected:** hash-diff source tree before/after at server-triggered rebuilds (WebDAV PUT); recording the known single-path change is acceptable; best-effort (recommended default).

### Event grouping
- Q: How to group "per refresh event"? → **Selected:** shared opaque `event_id` column on all rows of one refresh (recommended default).

### Retention & display
- Q: Retention window and stats display? → **Selected:** 90-day prune alongside `prune_access_log`; stats section = latest events, newest first, grouped with kind badges (recommended default).

## Claude's discretion
event_id format, store method names, markup, rename detection (`find_similar`).

## Deferred
Line-level diffs; change notifications.
