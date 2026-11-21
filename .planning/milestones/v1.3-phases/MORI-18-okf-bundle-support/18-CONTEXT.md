# Phase 18: okf-bundle-support — Context

**Gathered:** 2026-11-06
**Status:** Ready for planning
**Source:** `.planning/research/OKF.md` (issue #11 research) + orchestrator discretion — no discuss-phase round for this phase.

<domain>
## Phase Boundary

Make MoriNet an OKF-native vault host:
1. **Detection** — scan vault sources for OKF conformance at refresh/register; persist stats; badge on vault UI.
2. **Export** — "Export as OKF bundle" zip download, ACL-gated.
3. **Metadata** — `/api/v1` note objects expose parsed frontmatter metadata; MCP list/search gain `type`/`tags` filters.

NOT in this phase: trust-aware rendering of OKF families in mdBook pages (OKF-F01, needs Impeccable pass), Attested Computation execution (security non-starter), `log.md` synthesis from `vault_changes` (nice-to-have, deferred).
</domain>

<decisions>
## Implementation Decisions

- **D-01** Scope = detection + badge + export + API/MCP metadata (research options A, D, C). Trust-aware rendering (option B) deferred to a later phase — it touches the converter's rendering contract and requires an Impeccable design pass (project rule: no CSS changes without one).
- **D-02** Detection runs at the same hook as the Phase 13 `vault_changes` capture (refresh/register paths), scanning the **source** markdown (the llm-wiki/Obsidian dir), never the generated `.morinet-build/` book. Scan must be best-effort: a failure logs and leaves previous stats untouched — never blocks refresh.
- **D-03** Conformance per OKF SPEC v0.2 §11: every non-reserved `.md` has parseable frontmatter with non-empty `type`; `index.md`/`log.md` reserved (presence noted, structure not validated beyond existence). Version guess: **v0.2** if any concept carries `generated`/`verified`/`sources`/`status`/`stale_after`; else **v0.1** if concepts carry `type`; **"not an OKF bundle"** when < 80% of non-reserved `.md` files have a non-empty `type`. Persist per vault: version guess, concepts count, total count, scanned_at — a single JSON/text column on `vaults` (migration 0013) is acceptable; planner may choose a side table if cleaner.
- **D-04** Frontmatter parsing reuses `src/obsidian/frontmatter.rs` (flat YAML-subset parser, strings-only). OKF nested structures (`sources:` list-of-maps) are treated as opaque presence flags — do NOT add a YAML dependency and do NOT extend the parser beyond what `scalar()`/`list()` already give.
- **D-05** Badge = minimal text pill reusing existing pill/badge CSS classes (vault card + vault page): "OKF v0.2 — 42/45 concepts" / "not an OKF bundle" / no badge for non-scanned. No new CSS, no layout change (stays within existing chrome).
- **D-06** Export route: GET on the vault's action surface (planner picks exact path, e.g. `/vault/<slug>/okf-export.zip`), gated by the same read-ACL as vault browsing (404 when inaccessible, consistent with Phase 16 rule). Streams a zip of the source dir with files **byte-identical**; excludes `.morinet-build/`, `.git/`, and dot-directories; synthesizes in-memory a root `index.md` containing `okf_version: "0.2"` frontmatter **only when the bundle has no root index.md** (never written to disk). Exactly ONE new runtime dependency allowed, archive-only: `zip` with `default-features = false, features = ["deflate"]` (planner may substitute a lighter crate but must justify and keep the count at one).
- **D-07** `/api/v1` metadata exposure must NOT break the v1.2 contract (note list is a bare string array; note read is raw `text/markdown` — verified by pattern map). Additive design: (a) note-list and note-read routes accept `?meta=1` — when present, list returns objects `{"path": …, "frontmatter": {type, tags, status, trust_tier}}` and read returns `{"frontmatter": …, "size": …, "etag": …}` JSON; **default (no param) is byte-identical to v1.2**. (b) The vaults list endpoint (already a JSON object) gains additive `okf` fields (version guess, concepts/total) when stats exist. Trust tier per OKF §5.3: no `verified` key → `unverified`; `verified` only from non-`human:` actors → `machine-confirmed`; any `human:` actor → `human-reviewed`. Files without frontmatter → `"frontmatter": null`.
- **D-08** MCP `list`/`search` tools gain optional `type` and `tags` filter params applied **after** ACL filtering (inaccessible vaults still 404/absent, never 403). Tool schema updates via existing schemars path.
- **D-09** The mdBook rendering pipeline is untouched: the converter keeps stripping frontmatter (pin test "frontmatter must not survive into src" stays green). OKF metadata never reaches rendered book output in this phase.
- **D-10** Baseline to beat: 493 passed / 0 failed / 4 ignored; `scripts/check.sh` green. No new dependencies beyond D-06's single archive crate.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/research/OKF.md` — full OKF v0.2 analysis + MoriNet fit table + option matrix
- `src/obsidian/frontmatter.rs` — flat frontmatter parser to reuse (D-04)
- `src/obsidian/convert.rs` — converter contract: frontmatter stripped for rendering (D-09 pin test ~line 330)
- `src/store.rs` — vault refresh/change-capture hooks (D-02 anchor), migration style, `vaults` table
- `migrations/` — latest is 0012; this phase adds 0013
- `src/api.rs` — `/api/v1` note list/read helpers (D-07), ACL + problem+json conventions
- `src/mcp.rs` — list/search tool wrappers (D-08)
- `src/server.rs` — vault card/page templates + action routes (grep anchors only, ~9500 lines — never whole)
- OKF spec (external): https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md
</canonical_refs>

<specifics>
## Specific Ideas

- Trust-tier derivation is a pure function — unit-testable in isolation (candidate TDD task).
- Conformance scan is a pure function over a directory tree — unit-test with tempdir fixtures (neutral fake vaults: one v0.2-conformant, one v0.1, one mixed/not-OKF).
- Export zip built in-memory or streamed; vaults are small (quota-bounded) — simplicity over streaming cleverness, but do not buffer unbounded: cap follows existing upload quota conventions.
</specifics>

<deferred>
## Deferred Ideas

- OKF-F01 trust-aware rendering (status/stale/trust chips, sources footer) — needs Impeccable pass
- Attested Computation execution — spec defers runtime protocol; running bundle code is a security non-starter
- Synthesize `log.md` in export from `vault_changes` history
- OKF conformance trend in Statistics (pairs with issue #10)
</deferred>

---

*Phase: 18-okf-bundle-support*
*Context gathered: 2026-11-06 from research + discretion*
