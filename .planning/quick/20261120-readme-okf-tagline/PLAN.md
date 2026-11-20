# Quick Task: README tagline — foreground OKF import/read

**Created:** 2026-11-20
**Status:** complete

## Description

Update the README tagline so MoriNet's OKF support reads as import + serve, not export-only.

Exact replacement requested by the operator:

- Old: "Self-hosted mdBook server that exposes llm-wiki / Obsidian vaults as shared wiki sites and portable OKF bundles, with multi-user auth, WebDAV, and MCP."
- New: "Self-hosted mdBook server that exposes llm-wiki / Obsidian vaults and OKF bundles as shared wiki sites, with multi-user auth, WebDAV, and MCP."

## Tasks

1. Replace the tagline at README.md:3 (existing OKF link on "OKF bundles" kept — README link style).
2. Verify no other file carries the old tagline (grep "Self-hosted mdBook server" — README.md:3 was the only occurrence).
