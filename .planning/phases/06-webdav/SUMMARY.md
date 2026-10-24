# SUMMARY — 06 webdav ✅

PROPFIND (depth 0/1) + GET + PUT admin sur `/webdav/<slug>/…` — expose la SOURCE du vault (markdown in, `mdbook build` out), HTTP Basic + ACL par vault. Pitfalls documentés dans STATE-LOG (percent-decode avant traversal check, `Method::PROPFIND` inexistant, `.route(any(h))`). REQ-WEBDAV-*.

Pre-GSD phase — pas de PLAN séparé ; détail dans ROADMAP Phase 06.
