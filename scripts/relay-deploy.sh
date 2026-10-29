#!/usr/bin/env bash
# Deploy a relay binary to the public relay VM and check it came back with the new caps.
# Usage: scripts/relay-deploy.sh [binary]   (default: ~/.cache/wado/wado-relay.bin, built with
#   cargo zigbuild --release -p wado-relay --target x86_64-unknown-linux-gnu.2.35)
set -euo pipefail
BIN=${1:-$HOME/.cache/wado/wado-relay.bin}
HOST=wado-relay.indiasouthcentral.cloudapp.azure.com
scp -o ConnectTimeout=20 "$BIN" "wado@$HOST:wado-relay.new"
ssh -o ConnectTimeout=20 "wado@$HOST" \
  'sudo install -m755 wado-relay.new /usr/local/bin/wado-relay && sudo systemctl restart wado-relay && sleep 2 && systemctl is-active wado-relay'
echo "health: $(curl -s "https://$HOST/health")"
echo "caps:   $(curl -s "https://$HOST/online/0" | head -c 300)"
