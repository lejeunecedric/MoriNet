# OKF v0.3 — What MoriNet Needs to Implement

**Researched:** 2026-11-20 · **Spec:** `open-knowledge-format/open-knowledge-format` SPEC.md v0.3 (published 2026-11-18, one day old) · **Local copy:** `.planning/research/` cache at session time
**Status:** analysis only — no phase opened yet

## 1. TL;DR

v0.3 bills itself as "a precise rewrite of the v0.2 text plus three optional additions" (§13). **Two breaking changes, both consumer-side; zero required producer-side renames.** Most v0.2 bundles stay valid. MoriNet is a producer (export) *and* a consumer (scan, trust tiers, MCP/API metadata, viz graph), so the work is: declare `0.3` on export, adopt the two consumer rules, and sweep the clarifications into `scan_conformance`. Standing decisions hold: OKF surface stays thin, Attested Computation **execution** remains out (security non-starter) — v0.3 only tightens its documentation, nothing forces us to run attesters.

## 2. The v0.2 → v0.3 delta (spec §13)

### Breaking (2)
| # | Change | MoriNet impact |
|---|--------|----------------|
| B1 | Path fields / links without leading `/` resolve **from the containing file's directory** (v0.2 examples wrote bare paths from bundle root). Fallback §12.1: if the bundle declares `okf_version` ≤ 0.2 or none, consumers MAY retry from bundle root. | Graph edges (viz.html data, Phase 20 plan 20-05), link checks, `sources[].resource` resolution. Must resolve **both** forms; use the version declaration to pick the fallback. |
| B2 | Trust tier (§5.3) computed **only from current verification events**: an event is current when `verified[].at >= generated.at` (or `generated` absent). Tier can only go down vs v0.2 reading — safe direction. | **Confirmed gap:** `src/okf.rs::trust_tier` (:94) grants `human-reviewed` to any `human:` event regardless of `generated.at`. Must add the currency filter. Affects API/MCP `note_meta` (:104) and any UI badge. |

### Additive (3)
- `superseded_by` path field on `status: deprecated` concepts (§5.4) — consumers filtering to current knowledge SHOULD follow it. Pass-through in `note_meta` + follow in graph/link consumers.
- Bundle metadata at root `index.md` frontmatter: `okf_version`, `title`, `description`, `license` (§8.1). Export should synthesize `okf_version: "0.3"` (today it writes `"0.2"` — `src/okf.rs:235`) and preserve existing root-index frontmatter on regeneration.
- Extension keys: unknown keys allowed at all levels; future versions reserve `x-` prefix (§4.1). MoriNet's frontmatter handling MUST keep unknown keys (verify `obsidian::frontmatter` round-trip behavior on export — export is byte-identical source files per Phase 18 D-06, so this is already satisfied for pass-through).

### Clarifications to sweep into `scan_conformance` (§13.2 highlights)
- `.md` without non-empty `type` is **not a concept**; validators SHOULD warn (§3.2). Conformance does NOT require frontmatter in every `.md` (§11).
- Timestamps (§5): date-only = start of day UTC (re-accepted); date-time **without offset stays invalid**; accept quoted and YAML-native.
- Unknown `status` value ≠ `stable` — filters exclude the concept (§5.4). Unparseable `stale_after` is never fresh (§5.5).
- Path fields resolving to concepts are graph edges (§6.4); paths case-sensitive; percent-encode reserved chars (§6).
- Actor form generalized `<scheme>:<id>` incl. `team:` (§7) — `team:` in `verified` = machine-confirmed, NOT human-reviewed.
- `log.md` has no frontmatter (§9).
- Version detection (§12): prefer declared `okf_version` in root `index.md`; MoriNet currently **guesses** from key presence (`V02_KEYS`, `src/okf.rs:16` → returns "0.2"/"0.1"). Add: read root `index.md` frontmatter first, map "0.3", keep key-presence guess as fallback for undeclared bundles.

## 3. Validator ecosystem check (feeds Phase 20 D-04 gate)

- Spec is 1 day old; whether **okflint 0.5.1 / okforest 0.14.0** (both installed) already enforce v0.3 rules is **unverified** — several third-party `okf` toolkits exist and version-label themselves differently. **Action before/at Phase 20 execution:** run both against a v0.3 sample (root `index.md` with `okf_version: "0.3"`, `superseded_by`, date-only timestamps) and record which rule set they apply in `20-EVIDENCE.md`.
- Phase 20's docs-vault frontmatter (D-01: `type`/`title`/`description`/`generated`) is **unchanged in v0.3** → Phase 20 plans stay valid as written.

## 4. Recommended sequencing

1. **Execute Phase 20 as planned** (committed, checker-passed, v0.2-based — nothing in v0.3 invalidates it; the honesty rule and frontmatter shape are version-agnostic).
2. **Open Phase 21 "okf-v0.3-support"** with scope: B1 path resolution + version fallback, B2 trust-tier currency rule, version detection via root `index.md`, export declares `0.3` + bundle metadata (`title`/`description`/`license` from vault config), `superseded_by` pass-through, §13.2 conformance sweep, validator v0.3 behavior evidence.
3. **Explicitly out** (re-confirm): Attested Computation execution; JSON-LD bridge; registry submissions (all already deferred).

Suggested command: `/gsd-phase --insert 20 "okf-v0.3-support"` (or as next phase after 20 executes), then `/gsd-spec-phase 21` / `/gsd-discuss-phase 21`.

## 5. Code touchpoints (grounded)

| Anchor | Change |
|--------|--------|
| `src/okf.rs:16` `V02_KEYS` + `scan_conformance` version guess | read declared `okf_version` from root `index.md` first; accept "0.3" |
| `src/okf.rs:94` `trust_tier` | currency filter (`at >= generated.at`), `team:` → machine-confirmed |
| `src/okf.rs:104` `note_meta` | add `superseded_by` (+ maybe `stale_after` freshness flag) |
| `src/okf.rs:235` export root index synthesis | `okf_version: "0.3"`; optional title/description/license; preserve existing frontmatter |
| `src/okf.rs:134-136` caps, `build_bundle_zip` | unchanged (Phase 20 20-05 builds on this) |
| viz.html data injection (Phase 20 plan 20-05) | graph edges must use v0.3 path resolution (B1) — cheap now, plan already owns the edge builder |
| `scripts/check-okf.sh` (Phase 20 plan 20-03) | record validator spec-version behavior in evidence |
