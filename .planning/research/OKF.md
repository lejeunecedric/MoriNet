# Research: OKF — Open Knowledge Format (GitHub issue #11)

**Researched:** 2026-11-06 · **Sources:** Google Cloud blog (v0.1 2026-06-12, v0.2 2026-07), `GoogleCloudPlatform/open-knowledge-format` SPEC.md v0.2 (last commit 2026-08-21), ecosystem (LOKF, awesome-okf, openknowledge.ai linter).

## 1. What OKF is

An open, vendor-neutral Google Cloud spec (Sam McVeety, Amir Hormati) that **formalizes Karpathy's llm-wiki pattern** — exactly the vault format MoriNet already converts. Core: a **bundle** = directory of markdown files, one **concept** per file, YAML frontmatter + free-form body, standard markdown links forming a traversable graph. "If you can `cat` a file, you can read OKF; if you can `git clone` a repo, you can ship it." No schema registry, no central authority, no required tooling.

### The whole format (v0.2, ~1 page of hard rules)
- **Required frontmatter:** `type` only. A concept with just `type` is conformant.
- **Recommended:** `title`, `description`, `resource` (canonical URI of underlying asset), `tags`.
- **Reserved filenames** at any level: `index.md` (directory listing, progressive disclosure, no frontmatter except root `okf_version`), `log.md` (dated changelog, ISO `## YYYY-MM-DD` headings).
- **Links:** `/root-relative` (recommended) or relative; link kind is in prose, not the link.
- **Unknown anything** (types, keys, fields) MUST be tolerated — permissive conformance: never reject a bundle for missing optionals, unknown types, broken links, absent indexes.

### v0.2 = trust signals (the interesting part)
Answers five questions from frontmatter alone:
1. **Provenance** — `sources:` list (`resource` required per entry; optional `id`, `title`, credibility signals `author`/`usage_count`/`last_modified`); per-claim attribution via markdown footnotes keyed to `sources[].id`.
2. **Trust** — `generated: {by, at}` (who wrote it) vs `verified: [{by, at}]` (who confirmed it). **Trust tiers** derived by consumer: no `verified` → unverified; non-human only → machine-confirmed; `human:<id>` → human-reviewed.
3. **Freshness** — `stale_after` absolute instant.
4. **Lifecycle** — `status: draft|stable|deprecated` (absent = stable).
5. **Attestation** — `type: Attested Computation` concept carrying a sanctioned computation + `executor`/`attester` pointers (runtime protocol explicitly deferred to future versions).

**Actor convention:** `<producer>/<version>` for agents, `human:<id>` for people, `process:<id>` for automation.

v0.1→v0.2 breaking: `timestamp` → `generated.at`; body `# Citations` → `sources` frontmatter. Everything else additive.

## 2. Ecosystem signal (as of 2026-11)

- Google ships reference enrichment agent, static visualizer, 3 sample bundles; Knowledge Catalog ingests OKF (internal adoption).
- Community: **LOKF** (ontology profile of OKF via LinkML → JSON-LD/RDF), openknowledge.ai OKF linter plugin (v0.2 rules), awesome-okf list, standards-issue traffic (provenance/summarization-governance proposals). Young but moving; single-vendor origin is the main risk, mitigated by permissive spec + explicit welcome of alternative implementations.

## 3. Fit with MoriNet — we are 80% an OKF server already

**Key structural insight:** OKF formalizes llm-wiki, and MoriNet's vaults ARE llm-wiki/Obsidian directories. Concretely:

| OKF requirement | MoriNet today |
|---|---|
| Bundle = directory of md files | Vault source dir (git/local/WebDAV-editable) ✅ |
| Raw files with frontmatter intact | **WebDAV GET/PUT and `/api/v1` note read serve source markdown verbatim** — frontmatter included ✅ |
| Git repo distribution | GitHub vault sources, git2 sync ✅ |
| `index.md` progressive disclosure | Converter treats it as a normal page; mdBook renders it ✅ (partially) |
| `log.md` changelog | MoriNet has its own `vault_changes` wiki-changes log — richer, but not surfaced as `log.md` ⚠️ |
| Frontmatter consumed for display | **Converter STRIPS frontmatter** (`convert.rs`: "frontmatter must not survive into src") — human view loses all OKF trust signals ❌ |
| llm-wiki `type` field | llm-wiki vaults already carry `type`/`title`/`tags` — near-conformant v0.1 bundles out of the box ✅ |

So the gap is **display + metadata surfacing**, not storage or transport. The read-only REST API (Phase 16) already gives agents a transport for consuming OKF bundles; MCP tools read the converted source.

## 4. Options for issue #11 (v1.3+ candidates)

- **A. OKF detection + conformance report** (S): scan vault frontmatter at build/refresh; flag vault "OKF v0.1/v0.2 bundle" with conformance stats (files with `type`, reserved files, broken links) on the vault page / Statistics. Cheap: `frontmatter::split` already exists.
- **B. Trust-aware rendering** (M): stop stripping OKF families — render `status`/`stale_after`/trust-tier as a chip on the mdBook page header (draft/stale/human-reviewed badges); render `sources` as a "Provenance" footer. Touches `obsidian/convert.rs` + template CSS (Impeccable pass required per pitfall #9b).
- **C. API/MCP metadata** (M): `/api/v1` note list/read gains parsed frontmatter fields (`type`, `tags`, `status`, trust tier); MCP `search`/`read` filters by `type`/`tags`. Makes MoriNet a proper OKF *consumer* for agents, not just a file server.
- **D. Bundle export** (S): "Export as OKF bundle" = zip/git of source dir (already byte-identical to a bundle) + synthesized root `index.md` with `okf_version: "0.2"` if absent. Near-free given source-first architecture.
- **Skip for now:** Attested Computation runtime/executor/attester execution — spec itself defers the wire protocol; MoriNet must not run bundle code (security).

## 5. Recommendation

Sequence **A → D → C → B**: detection + export are near-free wins that make MoriNet an "OKF-native vault host" marketing-wise; C deepens the agent story alongside issue #10 (stats); B is the most visible but needs a design pass and touches the converter's core contract (frontmatter stripping is deliberate today — mdBook chokes on raw frontmatter). Do NOT chase Attested Computation execution. Spec is v0.x single-vendor: keep the OKF layer thin and read-mostly so a v0.3 rename is a 1-file change.

**Suggested phase shape:** one vertical phase "OKF bundle support" (detection + badges + export + API metadata), 3–4 plans, no new dependencies (serde_yaml already in tree via converter).
