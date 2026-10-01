#!/bin/sh
set -eu
ready=/run/gbtc-route/ready.json
[ -s "$ready" ] || exit 1
id=$(jq -er '.id' "$ready")
verified=$(jq -er '.mullvad_verified_at' "$ready")
now=$(date +%s)
[ "$((now - verified))" -le 100 ] || exit 1
timeout 5 tailscale status --json | jq -e --arg id "$id" '
    .BackendState == "Running" and
    .ExitNodeStatus.ID == $id and .ExitNodeStatus.Online == true' >/dev/null
