# Phase 20: okf-ecosystem-quick-wins - Context

**Gathered:** 2026-11-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Make MoriNet's OKF claims verifiable by third-party tools and make exported bundles open in the ecosystem's standard viewer:
1. **External validation** — the built-in docs vault (and the demo vault, one-off) pass `okflint` and OKForest CLI with zero errors.
2. **Local validation hook** — optional `scripts/check.sh` step runs the external validators when present in PATH (no GitHub CI resurrected).
3. **Conformance badge** — README shows an OKF conformance badge/claim backed by (1).
4. **viz.html in export** — `GET /vault/{slug}/okf-export.zip` output contains a root `viz.html` (official static visualizer) rendering the bundle graph offline; T-18-10 byte-size and entry caps still enforced.

NOT in this phase: registry/gallery submissions (okf.md listing, BundleDex, awesome-okf PRs — no-code quick wins, operator actions), JSON-LD/schema.org bridge, OKF-F01 trust-aware rendering, Attested Computation execution.

</domain>

<decisions>
## Implementation Decisions

### Docs vault conformance (discussed)
- **D-01:** Docs vault pages get OKF frontmatter following the **B3Wiki house pattern** (see `OKF-MAPPING.md` ref below): `type` (descriptive, e.g. `Reference`/`Guide`) + `title` + `description` + `generated: {by, at}`. **Honesty rule:** never emit `verified`, `status`, or `stale_after` — trust fields are claims, no evidence to back them. Applies to the 7 pages in `docs/morinet-docs/src/` (SUMMARY.md is the mdBook index, not a concept).
- **D-02:** Demo vault (server-side content on holldemo, not in repo): **one-off manual validation** — run okflint + OKForest against the demo vault content (on the server or a local copy), fix content at its source, record the clean run as evidence. No permanent automation for the demo vault.
- **D-03:** Bump the docs-seed marker in `src/docs_seed.rs` so every existing install **re-seeds its docs vault once** at next boot — the docs vault is first-party content, replacing it is the point.
- **D-04:** Validation bar: **zero errors AND zero warnings** (B3Wiki standard — the B3Wiki conversion hit 0/0). If a validator rule genuinely cannot apply, document the exception in the evidence note. *(Recommended default adopted — user was away when this was asked; confirm or relax during planning.)*

### Validator wiring (not discussed — planner/researcher discretion, constraints locked)
- Success criterion 2 is the contract: optional `scripts/check.sh` step, runs external validators **only when present in PATH**, no GitHub CI resurrected (standing constraint). Researcher must pin exact tool invocations: okflint is npm-based (`npx okflint`?), OKForest CLI install/run path (28 rules). A B3Wiki Python port of okflint exists at `scripts/okflint.py` (external repo) — usable as a dev-time oracle but the claim must be backed by the official validators.
- When validators are absent from PATH: check.sh prints a skip note and stays green. When present and failing: gate fails (that is the point of the claim).

### viz.html in export (not discussed — planner discretion, constraints locked)
- Vendor the official static `viz.html` as a repo asset (verify its license permits vendoring); embed at the **bundle zip root** inside `build_bundle_zip` — in-memory, offline, no build step, no network.
- Collision policy precedent: Phase 18 synthesizes root `index.md` **only when absent** (D-06 of 18-CONTEXT). Planner decides whether a vault's own `viz.html` wins (consistency with index.md precedent) or the official viewer overwrites — state the choice in the plan.
- T-18-10 caps (`MAX_EXPORT_ENTRIES` 20 000 / `MAX_EXPORT_BYTES` 256 MiB) still enforced; viz.html bytes count toward the budget.

### Conformance badge (not discussed — planner discretion)
- README badge/claim must be backed by criterion 1 evidence (docs vault clean run). Style free (shields.io static badge or plain markdown); claim wording must not overstate — it certifies the docs vault (plus one-off demo vault evidence), not arbitrary user vaults.

### Claude's Discretion
- Exact frontmatter values per docs page, evidence-note location/format, check.sh probe mechanics, viz.html source URL and embedding mechanics.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase research + OKF spec
- `.planning/research/OKF-ECOSYSTEM.md` — this phase's research: validator cluster (§3A), viz.html (§3C), sequencing (§4)
- `.planning/research/OKF.md` — OKF v0.2 spec analysis from Phase 18 (conformance §9/§11, trust layer)
- OKF spec (external): https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md

### House pattern (external repo — user's own conversion precedent)
- `/home/c/Projects/Dev/B3Wiki/OKF-MAPPING.md` — llm-wiki → OKF v0.2 mapping decisions + honesty rule (D-01 source)
- `/home/c/Projects/Dev/B3Wiki/scripts/okflint.py` — Python port of okflint (spec §9 + v0.2 shape), achieved 0 errors / 0 warnings on 161 files
- `/home/c/Projects/Dev/B3Wiki/scripts/to_okf.py` — reference conversion script

### Code anchors
- `src/okf.rs` — `scan_conformance` (:47), export caps `MAX_EXPORT_ENTRIES`/`MAX_EXPORT_BYTES` (:134-136), `check_export_caps` (:140), `build_bundle_zip` (:158) — viz.html injection point
- `src/docs_seed.rs` — docs vault seeding via `include_str!` + marker file `<data_dir>/docs/.morinet-docs-seed` (D-03 bump target)
- `docs/morinet-docs/` — docs vault content (validation target: `book.toml`, `src/SUMMARY.md` + 6 content pages)
- `src/server.rs` — export route :258, `okf_export` handler ~:3156-3210 (grep anchors only, ~10k lines — never whole)
- `scripts/check.sh` — local gate; optional validator step goes here
- `README.md` — badge target

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/okf.rs::build_bundle_zip` — already walks the source dir with in-walk caps, symlink escape rejection, dot-dir exclusion; viz.html injection is one extra in-memory entry.
- Phase 18 index.md synthesis (in-memory, only when absent, never written to disk) — exact precedent for viz.html injection.
- `src/docs_seed.rs` include_str! + marker pattern — content change + marker bump is the whole re-seed mechanism (D-03).

### Established Patterns
- Docs vault is a **native mdBook source** (book.toml + SUMMARY.md), not Obsidian format. mdBook natively consumes YAML frontmatter (title/description/keywords) and strips it from rendered pages — researcher must verify unknown OKF keys (`type`, `generated`) are also stripped and never render as visible text.
- Export stays ACL-gated (404-not-403), byte-identical source files (Phase 18 D-06).
- No GitHub CI — `scripts/check.sh` is the only gate (standing constraint).
- OKF surface stays thin (spec is v0.x single-vendor — locked decision).

### Integration Points
- `scripts/check.sh` — optional validator step (present-in-PATH probe).
- `build_bundle_zip` — viz.html root entry.
- `docs/morinet-docs/src/*.md` — frontmatter addition (content-only change).
- README — badge.

</code_context>

<specifics>
## Specific Ideas

- Evidence note for the clean validator runs (docs vault + one-off demo vault) should live somewhere durable (e.g. alongside the README claim or in `.planning/`) so the badge claim is auditable.
- B3Wiki's OKF-MAPPING frontmatter table is the template for what each docs page gets; `generated.by` for docs pages is a human/process actor the planner picks (e.g. `human:cedric` or `process:morinet-docs`).

</specifics>

<deferred>
## Deferred Ideas

- No-code ecosystem quick wins (research §4): GitHub topics `okf`/`open-knowledge-format`, `[OKF Tool] MoriNet` issue to fabricioctelles/skills, awesome-okf PRs, BundleDex submission — operator actions, not phase work.
- JSON-LD/schema.org bridge (listed ecosystem gap) — future phase.
- OKF-F01 trust-aware rendering — needs Impeccable pass (standing).
- Attested Computation execution — security non-starter (standing).
- Obsidian-consumer recipes / MCP consumer cross-listing (research §3D) — marketing work, future.

</deferred>

---

*Phase: 20-okf-ecosystem-quick-wins*
*Context gathered: 2026-11-18*
