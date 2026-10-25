#!/usr/bin/env bash
# Direct local deploy to the demo server — bypasses GitHub entirely.
# Mirrors deploy.yml's server-side steps: staging + atomic mv (ETXTBSY) +
# systemctl restart + /health.
#
# Usage: scripts/deploy.sh
# Env:   DEPLOY_HOST  e.g. user@morinet.treklair.fr (SSH must work from here)
#        HEALTH_URL   default https://morinet.treklair.fr/health
set -euo pipefail
cd "$(dirname "$0")/.."

: "${DEPLOY_HOST:?set DEPLOY_HOST as user@host}"
HEALTH_URL="${HEALTH_URL:-https://morinet.treklair.fr/health}"

echo "==> release build"
cargo build --release
./target/release/morinet --version

echo "==> scp to staging (never overwrite the running binary: ETXTBSY)"
# /opt/morinet is root-owned; stage in the user's home and sudo-mv across.
ssh "$DEPLOY_HOST" 'mkdir -p ~/morinet-staging'
scp target/release/morinet "$DEPLOY_HOST:~/morinet-staging/morinet"

echo "==> swap + restart + health"
ssh "$DEPLOY_HOST" '
  sudo -n mv -f ~/morinet-staging/morinet /opt/morinet/bin/morinet
  sudo -n chmod +x /opt/morinet/bin/morinet
  /opt/morinet/bin/morinet --version
  sudo -n systemctl restart morinet
'
sleep 2
curl -fsS "$HEALTH_URL" && echo && echo "deploy: OK"
