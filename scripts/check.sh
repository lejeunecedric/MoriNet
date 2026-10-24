#!/usr/bin/env bash
# Local quality gate — replaces the retired GitHub ci.yml (2026-10-25).
# Everything runs HERE, not on GitHub runners.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> fmt"
cargo fmt --check

echo "==> clippy -D warnings"
cargo clippy --all-targets -- -D warnings

echo "==> tests"
cargo test

echo "==> release build"
cargo build --release

echo "check: OK"
