# Phase 20 Evidence: Official OKF Validator Clean Runs

**What this certifies:** the built-in MoriNet docs vault
(`docs/morinet-docs/` — six OKF-frontmattered concept pages plus the mdBook
index) passes the official external OKF validators with **zero errors and
zero warnings** (D-04 bar).

**Scope limit:** this certifies the built-in docs vault
(`docs/morinet-docs`) and, in the Demo section, a one-off demo-vault
snapshot — **NOT arbitrary user vaults.**

---

## Docs vault run

- **Date (UTC):** 2026-11-20T15:27:10Z
- **Host:** Z2Mini (Linux)
- **Repo state validated:** commit `75a83ae1f8d6b8f49cd7a4f88dece0a8ad99cd62`
  (`git rev-parse HEAD` at run time; content pages unchanged since `598c6b8`)

### Tool versions (as recorded)

```
$ pip show okflint | head -2
Name: okflint
Version: 0.5.1

$ okforest --version
0.14.0
```

(okflint 0.5.1 installed via `uv tool install okflint` — okflint exposes no
`--version` flag; `pip show` is the recorded version source. okforest 0.14.0
installed as an npm global, Node v26.12.1 ≥ 22 requirement.)

### okflint (Python, mattdav/okflint 0.5.1)

Command (exact):

```
okflint validate --manifest docs/morinet-docs/okf-base.yaml docs/morinet-docs
```

Full output:

```
✅ All files are OKF-conformant.
```

Exit code: **0**

Scan-genuineness check (guard against a vacuous pass): with the same manifest,
a scratch copy of the vault with `src/faq.md`'s frontmatter removed was
correctly flagged `❌ [F001] src/faq.md — frontmatter absent or unparsable`
(1 error, exit 1). The clean run therefore reflects a real scan of the six
pages.

### OKForest CLI (npm, okforest 0.14.0)

Command (exact):

```
okforest validate docs/morinet-docs --format json
```

Full output:

```json
{
  "schemaVersion": 1,
  "tool": { "name": "okforest", "version": "0.14.0" },
  "path": "docs/morinet-docs",
  "summary": { "ok": true, "errors": 0, "warnings": 0, "infos": 1, "concepts": 6, "files": 6 },
  "findings": [
    {
      "ruleId": "okf/okf-version",
      "severity": "info",
      "file": "index.md",
      "message": "No root index.md: add one declaring `okf_version: \"0.2\"` and listing the top-level directories.",
      "docs": "https://www.okforest.ai/docs/rules/okf/okf-version/"
    }
  ]
}
```

Exit code: **0**. D-04 warning bar enforced by parsing the JSON
(`summary.errors == 0`, `summary.warnings == 0`), not by the exit code —
`okforest validate` exits 1 on errors only.

### Iterations that fixed findings

- okforest initial run: `error okf/frontmatter src/SUMMARY.md` (type-less
  concept) + `warning okforest/slug src/SUMMARY.md` → excluded `src/SUMMARY.md`
  via `docs/morinet-docs/.okforest/settings.yaml` `ignore:` (mdBook TOC index,
  not an OKF concept — D-01; mirrors the okflint manifest exclusion). No
  content page needed changes.
- okflint: clean on first run with `docs/morinet-docs/okf-base.yaml`
  (Guide/Reference/FAQ taxonomy, no status vocabulary per D-01).

### Config exclusions (not exceptions — both validators judge the same 6 pages)

- `src/SUMMARY.md` — mdBook table-of-contents index, not a concept (D-01).
  Excluded in `okf-base.yaml` (`exclude_patterns`) and
  `.okforest/settings.yaml` (`ignore`).

## Exceptions

| Rule id | Validator | Severity | Why it cannot apply | Decision ref |
|---|---|---|---|---|
| `okf/okf-version` | okforest | info (below the 0/0 bar) | No on-disk root `index.md` by design: OKF is permissive about absent indexes; the export synthesizes `index.md` at zip time, not on disk. | Phase 18 D-06, D-04 |

No `verified` / `status` / `stale_after` fields were introduced to satisfy any
rule — honesty rule (D-01) verified: `grep -rn -E '^(verified|status|stale_after):' docs/morinet-docs/src/` returns nothing.

## Demo vault run

One-off manual validation (D-02 — no automation, no check.sh involvement).

- **Date (UTC):** 2026-11-20T15:52:33Z
- **Host:** Z2Mini (Linux) — same toolchain as the docs-vault run above
- **Source:** the vaults registered in the live public demo instance
  (morinet.treklair.fr). Its vault registry contains exactly two demo vaults,
  both loaded from public GitHub sources; a third on-disk repo directory (`b3`)
  is a leftover of a removed vault and is not served — excluded.
- **Tool versions (re-recorded at run time):** okflint 0.5.1 (`pip show
  okflint`), okforest 0.14.0 (`npx okforest --version`), Node v26.12.1.
- **Method:** read-only copy of each vault's working tree from the server
  (`ssh … sudo tar cz --exclude=.git --exclude=.morinet-build`), extracted to a
  throwaway scratch dir (never committed). Per-vault okflint manifests modeled
  on `docs/morinet-docs/okf-base.yaml` (throwaway, in the scratch copy only).
  D-04 warning bar enforced by parsing `--format json` summary counts, not
  exit codes.

### Result: NOT clean at the D-04 bar — fixes belong at the upstream sources

Neither demo vault reaches 0 errors / 0 warnings. Both sources are operator
content in separate GitHub repos (not this repo), so per the plan the exact
fixes are listed below for the operator; nothing was modified server-side.

#### b3wiki — https://github.com/lejeunecedric/B3Wiki @ `1344f08684658e7cd4e704ae39bd291deec8e5aa`

Working tree clean on the server. 162 files / 153 concepts scanned (genuine
scan — findings span all directories).

okflint (exact command, run from the vault copy root):

```
okflint validate --manifest okf-base.yaml .
```

Full output:

```
⚠️ [L001] AGENTS.md — broken wikilink: [[slug]]
⚠️ [L002] queries/b3-beyond-jimmy-smith.md — broken markdown link: /raw/forums/organissimo-bft66-hammond-b3.md
⚠️ [L002] raw/articles/bandcampdaily-parlor-greens-emeralds.md — broken markdown link: /album-of-the-day
⚠️ [L002] raw/articles/bandcampdaily-parlor-greens-emeralds.md — broken markdown link: /contributors/blake-gillespie
⚠️ [L002] raw/articles/organissimo-bft66-hammond-b3.md — broken markdown link: Base64-Image-Removed
⚠️ [L002] raw/articles/organissimo-bft66-hammond-b3.md — broken markdown link: Base64-Image-Removed

0 error(s), 6 warning(s).
```

Exit code: **0** (0 errors; 6 warnings → fails the D-04 zero-warnings bar).

OKForest CLI (exact command):

```
npx okforest validate . --format json
```

Summary (verbatim):

```json
{ "ok": true, "errors": 0, "warnings": 89, "infos": 230, "concepts": 153, "files": 162 }
```

Exit code: **0** (0 errors; 89 warnings → fails the D-04 bar). Warning
classes (full census):

| Rule | Count | Files |
|---|---|---|
| `okf/description` | 66 | 60 `raw/**` Source pages + 6 others — no `description` frontmatter |
| `okforest/wikilink` | 11 | `log.md` ×5, `README.md` ×2, `AGENTS.md` ×1 (doc placeholder `[[slug]]`), `raw/articles/leslie-overdrive-blog-allmusic.md` ×2, 2 others |
| `okforest/slug` | 6 | `SCHEMA.md`, `README.md`, `OKF-MAPPING.md`, `AGENTS.md`, `content/newsletters/261020-B3Wiki-newsletter.md`, `content/newsletters/261021-B3Wiki-newsletter.md` — uppercase in file names |
| `okf/broken-link` | 6 | `queries/b3-beyond-jimmy-smith.md` → missing `raw/forums/organissimo-bft66-hammond-b3.md`; scraped-article site-relative links in `raw/articles/bandcampdaily-parlor-greens-emeralds.md` (`/album-of-the-day`, `/contributors/blake-gillespie`) and `raw/articles/organissimo-bft66-hammond-b3.md` (`Base64-Image-Removed` ×2) |

Infos (below the bar): `okf/legacy-v01` ×230 — `created`/`updated` fields in
v0.1 shape.

**Fixes needed at the source (B3Wiki repo):**

1. Add `description:` to the 66 flagged pages (mostly `raw/` Source captures).
2. Add the missing `raw/forums/organissimo-bft66-hammond-b3.md` (referenced by
   `queries/b3-beyond-jimmy-smith.md`) or fix that reference.
3. Neutralize scraped site-relative links / `Base64-Image-Removed` image
   placeholders in the two `raw/articles/*` captures (they point at the origin
   site, not the bundle).
4. Convert `[[wikilinks]]` in `log.md`, `README.md`, `AGENTS.md` (and 2 raw
   articles) to markdown links — or accept as documented exception (log.md is
   an OKF reserved changelog; `[[slug]]` in AGENTS.md is documentation prose).
5. Lowercase the 6 non-slug file names (`SCHEMA.md` → `schema.md`, etc.) —
   note this breaks conventional all-caps meta-file names; an okforest
   `ignore` entry is the alternative.

#### progwiki — https://github.com/lejeunecedric/ProgWiki @ `2c2668eec7e5d02670e5e632bb5584cc87142f2d`

Working tree clean on the server. This vault predates the B3Wiki OKF
conversion (llm-wiki shape: wikilinks everywhere, type-less `raw/` captures,
frontmatter-less newsletters) and needs the same conversion treatment.

okflint (exact command, run from the vault copy root):

```
okflint validate --manifest okf-base.yaml .
```

Result: **166 errors, 277 warnings, exit code 1**. Error census:
`F004` ×73 (`sources` not a list of mappings), `F002` ×68 (`type` absent —
all `raw/` pages), `F001` ×6 (no frontmatter: 3 `content/newsletters/*`,
`AGENTS.md`, `SCHEMA.md`, `README.md`), `R002` ×19 (non-ISO `log.md`
headings like `[2026-10-20] create | …`). Top warnings: `L003` ambiguous
wikilinks (×277).

OKForest CLI (exact command):

```
npx okforest validate . --format json
```

Summary (verbatim):

```json
{ "ok": false, "errors": 314, "warnings": 1264, "infos": 1, "concepts": 151, "files": 153 }
```

Exit code: **1**. Errors: `okf/sources` ×221, `okf/type` ×68,
`okf/log-date` ×19, `okf/frontmatter` ×6. Warnings: `okforest/wikilink` ×967,
`okf/description` ×145, `okf/index-entries` ×77, `okf/title` ×68,
`okforest/slug` ×5, `okforest/placeholder` ×2 (`{{ cite book}}`).

**Fixes needed at the source (ProgWiki repo):** the full B3Wiki-style OKF
conversion (`to_okf.py` precedent): add `type: Source` + `resource`/`timestamp`
to 68 `raw/` pages; reshape `sources:` entries into mappings with `resource`;
add frontmatter to the 3 newsletters and `AGENTS.md`/`SCHEMA.md`/`README.md`;
convert `log.md` headings to `## YYYY-MM-DD`; convert wikilinks to markdown
links; add `title`/`description`.

### Status (plan 20-06 checkpoint — RESOLVED 2026-11-20)

The one-off run happened; the D-04 clean bar is **not met** and cannot be met
without upstream content fixes in operator-owned repos (B3Wiki, ProgWiki).
The checkpoint offered: (A) fix at source + re-validate, (B) documented
D-04 exceptions (e.g. scraped `raw/` captures exempt from description/link
rules), or (C) publish the README claim scoped to the docs vault with the
demo validation marked pending.

**Decision (user, 2026-11-20): Option C** — the README badge/claim is
published citing **docs-vault evidence only**; the demo-vault validation is
explicitly marked in-progress/pending-operator in the README. Demo-vault
conformance is **not** claimed. Upstream fixes for B3Wiki/ProgWiki remain
with the operator repos (out of scope for this repo); this section can be
updated with a clean re-run if the sources are ever fixed.

