---
status: resolved
phase: 15-etag-conflict-conflict-safe-co-editing-conf-01-05
source: [15-VERIFICATION.md]
started: 2026-10-30T15:05:43Z
updated: 2026-11-20T18:44:36Z
---

## Current Test

[none — all tests resolved]

## Tests

### 1. Real rclone/Obsidian WebDAV client smoke test (CONF-05 / criterion 6)

expected: automated tests prove unconditional PUT keeps 201 and plain uploads unchanged, but no automated test drives a real client — manually run rclone copy + an Obsidian-via-WebDAV edit against a live vault and confirm pre-Phase-15 behavior (no 412s, no client errors).
result: [passed] Real-client smoke test 2026-11-20 (rclone v1.76.0 official static binary, HTTP Basic, against live MoriNet v1.4.0 on 127.0.0.1:9815, local Obsidian-style vault over `/webdav/uatvault`). Scenarios: (1) `rclone copy` 3 .md files IN — all "Copied (new)", exit 0, zero 4xx/5xx, files verified in vault source tree; (2) Obsidian-style edit-and-reupload (`rclone copyto` single changed file) — unconditional PUT (no If-Match/If-Unmodified-Since in any request) answered **HTTP 201 Created**, edit verified server-side; (3) copy OUT — `rclone copyto` single file: PROPFIND 207 + GET 200, bytes identical to source. Guard still works: stale `If-Match: "deadbeef..."` PUT -> 412, matching-etag PUT -> 200, and rclone's unconditional overwrite immediately after -> 201 (no 412). Pre-Phase-15 behavior confirmed: plain uploads unchanged (201), no client errors. Notes: (a) pre-existing Phase-6 routing quirk, unrelated to Phase 15 — PROPFIND on `/webdav/<slug>/` (trailing slash) 404s because axum `{*rest}` does not match an empty remainder, so the rclone remote URL needs a doubled slash (`/webdav/uatvault//`); with it all verbs work, only root `rclone ls` shows no entries (href-vs-root mapping, cosmetic). (b) One transient post-PUT rebuild failure in server log (llm-wiki convert/theme copy) on first write; file was saved, subsequent rebuilds succeeded — documented rebuild behavior, not a WebDAV/client error.

## Summary

total: 1
passed: 1
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
