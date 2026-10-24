#!/usr/bin/env bash
# Local release — replaces the retired GitHub release.yml / cargo-dist
# (2026-10-25). Builds HERE, publishes a GitHub release whose assets use the
# SAME names the deploy workflow expects (morinet-<target>.tar.xz + .sha256),
# so deploy.yml keeps working untouched (release:published trigger).
#
# Usage: scripts/release.sh v0.1.21
# Env:   GITHUB_TOKEN  (fine-grained, contents:write on this repo)
set -euo pipefail
cd "$(dirname "$0")/.."

TAG="${1:-}"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: $0 vX.Y.Z"; exit 1; }
VERSION="${TAG#v}"
: "${GITHUB_TOKEN:?set GITHUB_TOKEN (fine-grained PAT, contents:write)}"
REPO="$(git remote get-url origin | sed -E 's#(git@github.com:|https://github.com/)##; s#\.git$##')"
TARGET="x86_64-unknown-linux-gnu"

git diff --quiet && git diff --cached --quiet || { echo "working tree must be clean"; exit 1; }

echo "==> quality gate"
scripts/check.sh

echo "==> bump version to $VERSION"
sed -i "s/^version = \"[0-9.]*\"/version = \"$VERSION\"/" Cargo.toml
cargo check -q            # refresh Cargo.lock
git add Cargo.toml Cargo.lock
git commit -m "release $TAG"
git tag "$TAG"
git push origin main --follow-tags

echo "==> package the release binary"
STAGE="$(mktemp -d)"
DIR="morinet-$TARGET"
mkdir -p "$STAGE/$DIR"
cp "target/release/morinet" "$STAGE/$DIR/"
cp LICENSE "$STAGE/$DIR/" 2>/dev/null || true
# --strip-components=1 on the deploy side expects the target-named wrapper dir.
tar cJf "$STAGE/$DIR.tar.xz" -C "$STAGE" "$DIR"
( cd "$STAGE" && sha256sum "$DIR.tar.xz" > "$DIR.tar.xz.sha256" )

echo "==> create the GitHub release (draft → assets → publish)"
API="https://api.github.com/repos/$REPO"
REL_ID="$(curl -fsSL -X POST "$API/releases" \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  -d "{\"tag_name\":\"$TAG\",\"name\":\"MoriNet $TAG\",\"draft\":true,\"target_commitish\":\"main\",\"body\":\"Local build (scripts/release.sh). Binary: $DIR.tar.xz\"}" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"

for ASSET in "$DIR.tar.xz" "$DIR.tar.xz.sha256"; do
  curl -fsSL -X POST "https://uploads.github.com/repos/$REPO/releases/$REL_ID/assets?name=$ASSET" \
    -H "Authorization: Bearer $GITHUB_TOKEN" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "@$STAGE/$ASSET" > /dev/null
  echo "uploaded $ASSET"
done

# Publish only once assets are attached — deploy.yml waits for a release
# WITH assets, publishing last removes the race entirely.
curl -fsSL -X PATCH "$API/releases/$REL_ID" \
  -H "Authorization: Bearer $GITHUB_TOKEN" \
  -d '{"draft":false}' > /dev/null

rm -rf "$STAGE"
echo "released $TAG — deploy.yml (release:published) takes it from here"
