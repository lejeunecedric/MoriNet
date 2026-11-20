---
phase: 20
slug: okf-ecosystem-quick-wins
status: verified
threats_open: 0
asvs_level: 1
created: 2026-11-20
---

# Phase 20 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Register authored at plan time (all six `20-0*-PLAN.md` carried `<threat_model>` blocks).
> Verified 2026-11-20 by security auditor against HEAD `9690be4` — i.e. **after** the
> code-review fix commits (`0489653` sanitize_viz_body, `12257fb` re-seed tree-state,
> `9d06da4` TOML escaping, `aec8291` summary counters, `407a047` suffix strip),
> not against plan-time behavior.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Vault content → exported bundle | Hostile vault markdown/frontmatter enters synthesized `viz.html` opened by a victim | Untrusted markdown, titles, tags |
| Operator PATH → local gate | `scripts/check-okf.sh` executes operator-installed validator binaries | Gate pass/fail signal |
| Upstream repos/CDN → repo assets | Vendored viewer (template, viz.css/js, cytoscape, marked) embedded into every export | Third-party code shipped to end users |
| Docs vault seed | First-party embedded content force-re-seeded at versioned marker | Operator-owned docs vault on disk |
| Evidence/README committed | Validator transcripts and badge published publicly | Potential credentials/hostnames leakage |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-20-01 | Tampering | Forced docs re-seed (`src/docs_seed.rs`) | mitigate | Force path reachable only via `seed_docs_vault` → `seed_tree` (tree-state decision, `12257fb`); sole external caller `main.rs:187` on first-party `data_dir/docs`. Tests `missing_marker_with_existing_tree_forces_reseed`, `seed_docs_vault_grants_all_groups` pass | closed |
| T-20-02 | Elevation of Privilege | mdBook preprocessor command in vault `book.toml` | accept | Pre-existing mdBook property for ANY registered vault (`sources.rs` runs `mdbook build`); docs vault write stays role-gated (`can_write`, `server.rs:1018-1021`), docs grant is read-only (`store.rs:1407-1415`); seeded command is absolute `current_exe`, TOML-escaped (`docs_seed.rs:74-86`, `9d06da4`) | closed |
| T-20-03 | Tampering | `mdbook-okf-preprocessor` subcommand | mitigate | Mutates in-flight stdin JSON only — zero fs/network/process calls (grep-verified); malformed stdin fails build loudly (`invalid_json_fails_loudly` passes) | closed |
| T-20-04 | Supply Chain | Third-party validators (okflint PyPI, okforest npm) | mitigate | Official registries only, versions recorded verbatim in `20-EVIDENCE.md` (okflint 0.5.1, okforest 0.14.0); zero curl usage | closed |
| T-20-05 | Information Disclosure | Committed `20-EVIDENCE.md` / README | mitigate | Grep for credentials/private IP ranges (100.76.x, 10.x, 192.168.x) clean | closed |
| T-20-06 | Spoofing | PATH-resolved validator binaries (`scripts/check-okf.sh`) | mitigate | `command -v` probe only; never auto-installs, never fetches network; spoofed validator can only forge a PASS | closed |
| T-20-07 | Tampering | Gate weakening (`scripts/check.sh` wiring) | mitigate | `check.sh:17` under `set -euo pipefail`, no `\|\| true`; skip paths print explicit notes; `summary.errors/warnings == 0` asserted, fail-closed on unknown schema (`aec8291`) | closed |
| T-20-08 | Tampering | JSON parsing of validator output | mitigate | python3 stdlib `json.load` on stdin, single-quoted code, no eval, no shell interpolation of untrusted strings | closed |
| T-20-09 | Supply Chain | Vendored viewer assets `assets/okf-viz/` | mitigate | Pinned upstream commit `527e81af…` + pinned CDN versions; lock test `vendored_okf_viewer_provenance_checksums_match` recomputes sha256 vs `PROVENANCE.md` (passes) | closed |
| T-20-10 | Legal / License | Apache-2.0 attribution for vendored viewer | mitigate | Google LLC Apache-2.0 headers retained in all three viewer files; MIT for cytoscape/marked; license table in `PROVENANCE.md` | closed |
| T-20-11 | Tampering | Vendored JS executes in end-user browser | transfer | Mainstream pinned libs (cytoscape 3.28.1, marked 12.0.0) from official distributions; `sha256sum` of all 5 assets matches `PROVENANCE.md` exactly | closed |
| T-20-12 | Denial of Service | Bundle zip resource exhaustion | mitigate | `check_export_caps(entries+1, bytes+len)` BEFORE `start_file` (`okf.rs:505`, `:563`); caps 20 000 / 256 MiB; tests `viz_bytes_count_toward_caps`, `export_caps_guard_direct` pass | closed |
| T-20-13 | Tampering (XSS) | Stored XSS via hostile vault content into synthesized `viz.html` | mitigate | TWO layers at HEAD: `sanitize_viz_body` (`okf.rs:300`, sole body assignment site `:535` — tag-starts escaped, `javascript:/data:/vbscript:/file:` → `blocked:`) + JSON unicode-escape; labels/descriptions rendered via `textContent`. Regression test `viz_sanitizes_markdown_bodies` passes. Closes review finding 20-R1 (HIGH) | closed |
| T-20-14 | Information Disclosure | Data exfiltration via `viz.html` | mitigate | Fully offline: libs inlined; grep `fetch(/XMLHttpRequest/WebSocket/sendBeacon` = 0 across vendored assets and synthesis code | closed |
| T-20-15 | Integrity (Overclaim) | README conformance badge wording | mitigate | Claim scoped: docs vault only, demo validation in progress, "not arbitrary user vaults" (README:55); user-approved checkpoint 2026-11-20 | closed |
| T-20-16 | Information Disclosure | Demo-vault evidence section | mitigate | Cites only public GitHub repos at pinned SHAs + public docs URL; no credentials/private hostnames | closed |
| T-20-17 | Tampering | Demo vault copy handling | mitigate | Throwaway copy under scratch dir only; `git ls-files` shows no demo-vault content committed | closed |

*Status: open · closed*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-20-01 | T-20-02 | mdBook `[preprocessor.*] command` executes arbitrary commands by design for ANY vault whose book.toml an identity can write. Pre-existing platform property (all vaults, since v0.1.x), not introduced by Phase 20; docs vault write stays role-gated and its seeded command is first-party absolute `current_exe`. Documented in 20-01 threat model per D-05. | Phase 20 threat model (planner, D-05) | 2026-11-20 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-20 | 17 | 17 | 0 | gsd-security-auditor (verify-only, HEAD `9690be4`) |

Notes: plan-time claim for T-20-13 (unicode-escape alone) was disproven by code review
(`20-REVIEW.md` 20-R1 HIGH) and remediated at `0489653` before this audit; the auditor
verified the two-layer mitigation at HEAD, not the plan text.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-20 (auditor verdict SECURED, 17/17 closed)
