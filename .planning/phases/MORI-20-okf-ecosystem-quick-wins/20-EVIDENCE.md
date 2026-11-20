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

TODO(20-06)

(to be populated by plan 20-06 — D-02 one-off validation of the demo vault;
no results recorded here until that run exists.)
