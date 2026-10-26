# SUMMARY — 06 webdav ✅

PROPFIND (depth 0/1) + GET + PUT admin at `/webdav/<slug>/…` — exposes the vault SOURCE (markdown in, `mdbook build` out), HTTP Basic + per-vault ACL. Pitfalls documented in STATE-LOG (percent-decode before traversal check, `Method::PROPFIND` does not exist, `.route(any(h))`). REQ-WEBDAV-*.

Pre-GSD phase — no separate PLAN; details in ROADMAP Phase 06.
