# OKF Viewer Assets — Provenance & License Record

These files are the official OKF static graph viewer, vendored from the
`open-knowledge-format/open-knowledge-format` repository (the active upstream;
`GoogleCloudPlatform/open-knowledge-format` is deprecated). MoriNet embeds them
into OKF bundle exports (`GET /vault/{slug}/okf-export.zip`) so an extracted
bundle can render its concept graph offline with no network access. MoriNet
does **not** modify these files; the only transformation happens at export
time (plan 20-05), where the documented placeholders in `viz-template.html`
(`__BUNDLE_NAME__`, `__BUNDLE_DATA__`, `/*__VIZ_CSS__*/`, `/*__VIZ_JS__*/`) are
substituted with the bundle's own data and the CDN `<script>` tags are replaced
by the inlined `cytoscape.min.js` / `marked.min.js` bytes.

## License

| Asset | License | Verified from |
|---|---|---|
| `viz-template.html`, `viz.css`, `viz.js` | Apache-2.0, Copyright 2026 Google LLC | License header present at the top of each vendored file (fetched from the pinned commit below); upstream repo root `LICENSE.md` is the Apache-2.0 text: <https://raw.githubusercontent.com/open-knowledge-format/open-knowledge-format/527e81aff55992be21507e97004286789126f88c/LICENSE.md> |
| `cytoscape.min.js` | MIT, Copyright (c) 2016-2023 The Cytoscape Consortium | MIT permission notice at the top of the vendored file; also <https://github.com/cytoscape/cytoscape.js/blob/v3.28.1/LICENSE> |
| `marked.min.js` | MIT, Copyright (c) 2011-2024 Christopher Jeffrey | `marked v12.0.0 ... (MIT Licensed)` banner at the top of the vendored file; also <https://github.com/markedjs/marked/blob/v12.0.0/LICENSE> |

MoriNet itself is MIT OR Apache-2.0 dual-licensed (`Cargo.toml`, `LICENSE`),
which permits distributing these Apache-2.0 and MIT assets alongside it. The
Apache-2.0 attribution obligations are satisfied by retaining the Google LLC
license headers verbatim in `viz-template.html`, `viz.css`, and `viz.js`
(no header was stripped or altered) plus this PROVENANCE.md record.

## Provenance

Upstream viewer sources were fetched at pinned commit
`527e81aff55992be21507e97004286789126f88c` (main, 2026-11-18T17:00:25Z) of
<https://github.com/open-knowledge-format/open-knowledge-format>. The library
pins match the exact versions referenced by the template's CDN tags
(cytoscape 3.28.1, marked 12.0.0).

| File | Source URL | sha256 |
|---|---|---|
| `viz-template.html` | <https://raw.githubusercontent.com/open-knowledge-format/open-knowledge-format/527e81aff55992be21507e97004286789126f88c/src/reference_agent/viewer/templates/viz.html> | `a0aaa6052c64e231533678f06b84b03ff4d371ea2f74030750a17a44574b3e9b` |
| `viz.css` | <https://raw.githubusercontent.com/open-knowledge-format/open-knowledge-format/527e81aff55992be21507e97004286789126f88c/src/reference_agent/viewer/static/viz.css> | `eed1f94f62c1d17d9e6bd73fbc1e004eb68e0d91bc571f855ce48ebdd91c087a` |
| `viz.js` | <https://raw.githubusercontent.com/open-knowledge-format/open-knowledge-format/527e81aff55992be21507e97004286789126f88c/src/reference_agent/viewer/static/viz.js> | `944714077ba8691a420c74e3534e30b76897d49253acb1b1f9c7da1c88ea5e94` |
| `cytoscape.min.js` | <https://cdn.jsdelivr.net/npm/cytoscape@3.28.1/dist/cytoscape.min.js> | `92d752b48ea949720675865197fd2a0001c95bc5888545e990af60321712d4c6` |
| `marked.min.js` | <https://cdn.jsdelivr.net/npm/marked@12.0.0/marked.min.js> | `eb1f6b19880bc80a5fe34c6a61885173b60edda455ba7a33c98714db17d39f99` |

## Update rule

When refreshing these assets: re-fetch from a newly pinned upstream commit (or
exact library version), keep the license headers intact, recompute every sha256
with `sha256sum *` in this directory, update the table above verbatim, and bump
the pinned commit SHA in the Provenance section. The in-repo test
`okf::tests::vendored_okf_viewer_provenance_checksums_match` fails if any file
drifts from its recorded hash.
