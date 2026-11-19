# Research: OKF Ecosystem — integration & referencing opportunities

**Researched:** 2026-11-18 · **Sources:** okf.md/ecosystem-map (42 tools, scores + timeline through 2026-10-22), okf.md/contribute (submission process), okf.md/tools, bundledex.net, awesome-okf lists (linyiru, McClawdDigital, yzfly), registry.okf-memory.dev. Companion: [OKF.md](OKF.md) (spec research, Phase 18).

**Verified:** MoriNet is NOT currently referenced anywhere in the OKF ecosystem (web search for "MoriNet OKF" returns only unrelated projects).

---

## 1. Where MoriNet would sit on the map

The map has 42 tools in 7 categories (Official, Generator, Validator, Consumer, Publisher, Integration, Standard). **There is no self-hosted open-source OKF server on it.** The nearest neighbours:

| Tool | Category | Position | Gap vs MoriNet |
|---|---|---|---|
| Google Cloud Knowledge Catalog | Official | Mature but heavy (9.5 / ease 1.5) | Cloud infra, enterprise-only |
| Korpus.cloud | Publisher | Early, hosted SaaS | Not self-hosted, OKF docs pending |
| Kiso | Publisher | Publishing engine | Static sites, no auth/API/MCP |
| xMatrix | Publisher | Early | Team workspace, git-based |

MoriNet's unique claim on the map: **the only self-hosted, single-binary, open-source OKF-native server** — with WebDAV write path, Obsidian compatibility, per-user ACL, REST API with `?meta=1` frontmatter metadata, and a native MCP server with `type`/`tags` filters. On the map's axes that is a defensible sweet-spot entry: Released/GA maturity + low setup (one binary, `morinet init`).

Category to request: **Publisher** (host/publish angle) or **Integration**; the contribute page also accepts "Library, etc." — propose "Server/Host" and let the maintainer place it.

## 2. Referencing opportunities (get listed) — prioritized

1. **okf.md Tools page + Ecosystem Map** — the canonical catalog.
   - Process (from okf.md/contribute): open an issue at `github.com/fabricioctelles/skills`, title `[OKF Tool] MoriNet`, body = repo URL, one-line description, category, language/platform (Rust), OKF version (v0.2). Manual review, response in a few days.
   - Requirements MoriNet already meets: open source ✅, OKF v0.2 compatible ✅, working ✅, documented ✅.
   - Precedent: Korpus.cloud was "founder-submitted" — self-submission is normal.
2. **BundleDex (bundledex.net)** — directory of ~1000 OKF bundles, crawled from GitHub (indexes the `okf` topic; JSON API `/api/bundles.json`, MCP at `/api/mcp`, `llms.txt`, HF dataset).
   - Action: add GitHub topics `okf`, `open-knowledge-format` (+ `llm-wiki`, `mdbook`) to the repo and make the repo itself discoverable as an OKF-adjacent tool; submit via their "Submit Your Bundle" flow. McClawdDigital/awesome-okf is fed by BundleDex, so one submission cascades.
3. **awesome-okf lists** — PRs to `linyiru/awesome-okf` (curated, itself an OKF bundle) and `McClawdDigital/awesome-okf` (Tools section). Cheap, high-visibility.
4. **openknowledgeformat.com** — validator/community site (adoption 55); check for a submit/listing path once the okf.md listing lands.
5. **registry.okf-memory.dev** — agent-memory bundles only (10 bundles); low priority, revisit if we ever publish a reference bundle.

## 3. Integration opportunities

### A. Validator interop — credibility, near-free (do first)
The validator cluster is the most active ecosystem segment: **okflint v0.5** (Python — pip/uv install; `okflint validate --manifest okf-base.yaml <base>`, manifest required; corrected 2026-11-19 at Phase 20 plan-time — an earlier draft of this doc mislabeled it npm), **OKForest CLI** (npm-based, `npx okforest validate`, Node 22+, 28 rules; exits 1 on errors only — enforce a zero-warnings bar by parsing `--format json`, not the exit code) + **OKForest Validate** GitHub Action, **OKF-Schema** (JSON Schema), **okf-mcp**, **JidoSeal** (Bronze/Silver/Gold grading).
- Dogfood: run `okflint` and/or OKForest CLI against the built-in **docs vault** and the demo vault; fix any rule gaps our own `scan_conformance` misses.
- Then claim it: "docs vault passes okflint + OKForest" badge in the README.
- Note: MoriNet deliberately has no GitHub CI (only `deploy.yml`) — integrate as an optional step in `scripts/check.sh` or a local dev script, not a resurrected workflow.

### B. Rust alliance — W4G1/okf
`okf` on crates.io (v0.2.6, pure Rust toolkit + TUI) is the only other Rust OKF project. Cross-reference each other (README mention, open an issue upstream); evaluate their validator as a dev-dependency test oracle for `scan_conformance`. "Rust OKF ecosystem" is an untapped visibility niche.

### C. viz.html — official graph visualizer (ease 9.5, adoption 70)
Cheap flagship feature: include the static `viz.html` viewer in the OKF export zip (or link "open as graph" on the vault page). Instantly makes exported bundles explorable with the ecosystem's standard tool. Small spec/effort, high demo value.

### D. MCP/agent consumers — the differentiator to pitch
Consumers (hermes-okf, OKF Agent Memory, OKF-Runtime, KL4A, pi-openwiki, Coding Agent Skills) all read bundles locally; **none has a self-hosted server backend**. MoriNet's MCP + REST with metadata filters is exactly their missing "always-fresh remote bundle" layer.
- Action: write 1–2 short recipes "Consume a MoriNet vault with <tool>" — start with hermes-okf (PyPI, Hermes-adjacent) and OKF Agent Memory. Offer to upstream the recipe to their READMEs (mutual listing).

### E. Obsidian gap — open gap on the map ("Obsidian plugin: validation + templates", Low difficulty)
MoriNet already covers the publishing half (Obsidian vault → WebDAV → live site → OKF export). Short term: document the "Obsidian → WebDAV → MoriNet → OKF" pipeline (marketing gold: *Obsidian-native OKF publishing*). The plugin itself is out of scope but a natural community collaboration.

### F. OKF → schema.org/JSON-LD bridge — open gap (Medium difficulty)
Nobody has built it. MoriNet renders every concept as an HTML page — injecting JSON-LD (`Article`/`TechArticle` + `dateModified`, `author` from `verified`) from frontmatter at serve time would fill the gap and differentiate public vaults (SEO). Candidate Phase 19+.

### G. Watch list (no action)
- **signed-okf** (trust layer, Early) — aligns with our deferred trust-aware rendering; revisit when it stabilizes.
- **AI-XF / W3C DataBook / OriginTrail** — competing/adjacent standards, all Early/Proposal; spec risk unchanged (keep the OKF layer thin, per OKF.md §5).
- **MySQL producer gap** — out of scope (SQLite-only architecture).

## 4. Recommended sequencing

**No-code quick wins (this week):**
1. GitHub repo: add topics `okf`, `open-knowledge-format`, `llm-wiki`; description mentions OKF-native host.
2. Submit `[OKF Tool] MoriNet` issue to `fabricioctelles/skills` (okf.md listing).
3. PRs to both awesome-okf lists; BundleDex submission.

**Code-sized (next phases, priority order):**
1. **Validator interop + conformance badge** (A) — S; dogfood okflint/OKForest on docs vault, optional `check.sh` step.
2. **viz.html in OKF export** (C) — S; export zip ships the official viewer.
3. **Trust-aware rendering** — already deferred from Phase 18 (needs Impeccable pass); pairs with signed-okf watch item.
4. **JSON-LD bridge** (F) — M; fills a listed ecosystem gap, SEO angle for public vaults.

**Risk notes:** spec is v0.x single-vendor (keep OKF surface thin — existing locked decision); the ecosystem is young and listing is manual/community-run — engage as a contributor (recipes, cross-references), not astroturfing.
