#!/usr/bin/env bash
# Optional OKF external-validator step for the local quality gate (plan 20-03).
#
# Contract (20-CONTEXT.md, validator wiring):
#   - Strictly optional: the okflint / okforest binaries are ONLY ever executed
#     if the operator already put them in PATH. This script NEVER auto-installs
#     anything and NEVER touches the network.
#   - Absent from PATH  -> explicit skip note, exit stays green.
#   - Present and failing -> hard gate failure (never swallowed, no `|| true`).
#   - D-04 bar: zero errors AND zero warnings. okforest exits 1 on errors only,
#     so warnings are enforced by parsing its own `--format json` output with
#     python3 stdlib (JSON goes via stdin, never re-evaluated by the shell).
#
# PATH trust boundary: same as the rest of scripts/check.sh (cargo/rustc/mdbook)
# — the gate executes whatever the operator's PATH resolves. A spoofed validator
# could only forge a PASS, never escalate beyond an attacker already on PATH.
set -euo pipefail
cd "$(dirname "$0")/.."

target="docs/morinet-docs"

# --- okflint (Python; mattdav/okflint) --------------------------------------
if command -v okflint >/dev/null 2>&1; then
  echo "==> okflint"
  # Runs unguarded: exit 1 on findings aborts this script via set -e.
  okflint validate --manifest "$target/okf-base.yaml" "$target"
else
  echo "skip: okflint not in PATH — install with pip install okflint to enable the OKF lint gate"
fi

# --- OKForest CLI (npm; okforest) --------------------------------------------
if command -v okforest >/dev/null 2>&1; then
  echo "==> okforest"
  # set -e: a non-zero exit here means okforest saw ERRORS — gate fails.
  json="$(okforest validate "$target" --format json)"
  printf '%s\n' "$json"
  # D-04: warnings fail here even though okforest itself exits 0 on warnings.
  printf '%s\n' "$json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
bad = [f for f in data.get("findings", [])
       if f.get("severity") in ("error", "warning")]
if bad:
    print(f"okforest: {len(bad)} error/warning finding(s) — D-04 bar is 0 errors / 0 warnings")
    for f in bad:
        print("  [%s] %s %s: %s" % (f.get("severity"), f.get("ruleId"), f.get("file", ""), f.get("message", "")))
    sys.exit(1)
'
else
  echo "skip: okforest not in PATH — install with npm install --global okforest to enable the OKForest gate"
fi

echo "okf-check: OK"
