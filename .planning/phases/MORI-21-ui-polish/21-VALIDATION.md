---
phase: 21
slug: ui-polish
status: verified
nyquist_compliant: true
wave_0_complete: true
created: 2026-11-22
---

# Phase 21 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Reconstructed retroactively after execution (State B) — all commands verified green at HEAD `2ad3117` (624 passed / 0 failed / 4 ignored).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in (`cargo test` — unit + tokio async tests in-crate) |
| **Config file** | none — `Cargo.toml` workspace, no extra test config |
| **Quick run command** | `cargo test --lib <module>::<name> 2>&1 \| tail -5` |
| **Full suite command** | `cargo test 2>&1 \| grep -E "^test result" \| awk '{p+=$4;f+=$6} END {print p,f; exit (f>0)}'` |
| **Estimated runtime** | ~10 s targeted · ~60–90 s full (warm target/) |

---

## Sampling Rate

- **After every task commit:** `cargo test --lib <touched module>::`
- **After every plan wave:** full `cargo test` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` (aggregated in `scripts/check.sh`)
- **Before `/gsd-verify-work`:** full suite must be green
- **Max feedback latency:** ~90 s

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 21-01-01 | 01 | 1 | UI-01 | T-21-01/02/03 | Paths escaped, count + length capped, serde-default | unit | `cargo test --lib okf:: 2>&1 \| tail -3` (`scan_records_non_conformant_paths`, `scan_truncates_overlong_miss_paths`, `clip_miss_path_*`) | ✅ | ✅ green |
| 21-01-02 | 01 | 1 | UI-01 | T-21-01/03 | Badge HTML escapes all vault-controlled strings | unit | `cargo test --lib okf_badge 2>&1 \| tail -3` (8 × `okf_badge_html_*`) | ✅ | ✅ green |
| 21-02-01 | 02 | 1 | UI-02 | T-21-08 | Injected DOM uses hard-coded literals only | unit (embedded bytes) | `cargo test --lib morinet_js_ships_looks_picker 2>&1 \| tail -3` | ✅ | ✅ green |
| 21-02-02 | 02 | 1 | UI-02 | T-21-07 | Handler stops propagation before `mdbook-theme` write | unit (embedded bytes) | `cargo test --lib morinet_js_looks_picker_stops_event_propagation 2>&1 \| tail -3` | ✅ | ✅ green |
| 21-03-01 | 03 | 2 | UI-03 | T-21-13/14 | Theme whitelist reject-not-coerce; field order locked | unit | `cargo test --lib edit_page_ 2>&1 \| tail -3` (+ `edit_submit_rejects_bad_theme`) | ✅ | ✅ green |
| 21-03-02 | 03 | 2 | UI-03 | T-21-16 | Shipped docs prose keeps badge-detail + Looks claims | unit (coverage lock, `2ad3117`) | `cargo test --lib sharing_docs_describe 2>&1 \| tail -3` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

None — existing infrastructure (in-crate `cargo test` suite, 624 tests) covered all phase requirements. The one gap found at audit (docs prose had only a one-shot plan grep, no durable test) was filled inline at `2ad3117`.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Badge click expand/collapse in-flow, card layout intact | UI-01 | Native `<details>` interaction is browser behavior; tests assert markup only | Home → click OKF badge pill → panel expands with version/concepts/list → click again collapses — **RESOLVED 2026-11-22 live UAT (HV-1, see 21-VERIFICATION.md)** |
| Palette pick applies, persists across sessions, wins over serve-time default | UI-02 | localStorage + mdBook `get_theme()` runtime precedence | Book → theme popup → Looks → pick Ayu → restyled on reload, value survives navigation/reload, `aria-current` marks choice — **RESOLVED 2026-11-22 live UAT (HV-2)** |
| Vault Edit rendered row geometry at narrow/wide widths | UI-03 | Flex-wrap placement is pixel-level; source order + `flex-basis:100%` locked by test | `/vault/{slug}/edit` → Theme on own row below checkboxes, Save directly below — **RESOLVED 2026-11-22 live UAT (HV-3, after fix `d564f0c`)** |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify commands, all green
- [x] Sampling continuity: every task has an automated command (no 3-consecutive gap)
- [x] Wave 0: none required (gap filled at `2ad3117`)
- [x] No watch-mode flags
- [x] Feedback latency < 90 s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-11-22 (retroactive audit; full suite 624/0/4 + clippy + fmt green)

---

## Validation Audit 2026-11-22

| Metric | Count |
|--------|-------|
| Gaps found | 1 (UI-03 docs prose — no durable test) |
| Resolved | 1 (`sharing_docs_describe_badge_detail_and_looks_picker`, `2ad3117`) |
| Escalated | 0 (3 browser items already resolved by live UAT) |
