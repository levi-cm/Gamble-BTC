#!/bin/sh
set -eu

primary=${GBTC_EXIT_PRIMARY:-max-vprouter.dinosaur-dojo.ts.net}
fallback=${GBTC_EXIT_FALLBACK:-nico-vpn.dinosaur-dojo.ts.net}
ready=/run/gbtc-route/ready.json
mkdir -p /run/gbtc-route
rm -f "$ready"
tailscaled --tun=userspace-networking --socks5-server=127.0.0.1:1055 \
    --state=/var/lib/tailscale/tailscaled.state &
daemon_pid=$!
# Called by signal/EXIT traps.
# shellcheck disable=SC2317
cleanup() {
    rm -f "$ready"
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
}
trap 'cleanup; exit 0' INT TERM
trap cleanup EXIT

active_node=
verified_at=0
primary_successes=0
while kill -0 "$daemon_pid" 2>/dev/null; do
    status=$(timeout 5 tailscale status --json 2>/dev/null) || status='{}'
    if ! printf '%s' "$status" | jq -e '.BackendState == "Running"' >/dev/null; then
        rm -f "$ready"
        sleep 5
        continue
    fi

    # Require the exact requested exit node, an advertised exit route, and a
    # successful encrypted Tailscale ping. Never clear the exit-node preference.
    chosen=
    for node in "$primary" "$fallback"; do
        peer=$(printf '%s' "$status" | jq -c --arg node "$node" '
            first((.Peer // {})[] |
                select((.DNSName | rtrimstr(".")) == $node) |
                select(.ExitNodeOption == true and .Online == true)) // empty')
        if [ -z "$peer" ]; then
            [ "$node" != "$primary" ] || primary_successes=0
            continue
        fi
        ip=$(printf '%s' "$peer" | jq -r 'first(.TailscaleIPs[] | select(contains(":") | not)) // empty')
        [ -n "$ip" ] || continue
        if ! timeout 5 tailscale ping --tsmp --c=1 --timeout=3s \
            --until-direct=false "$ip" >/dev/null 2>&1; then
            [ "$node" != "$primary" ] || primary_successes=0
            continue
        fi

        # Three healthy primary checks before failback avoid rapid route churn.
        if [ "$node" = "$primary" ] && [ "$active_node" = "$fallback" ]; then
            primary_successes=$((primary_successes + 1))
            [ "$primary_successes" -ge 3 ] || continue
        fi
        id=$(printf '%s' "$peer" | jq -r '.ID')
        prefs=$(timeout 5 tailscale debug prefs 2>/dev/null) || prefs='{}'
        selected_id=$(printf '%s' "$prefs" | jq -r '.ExitNodeID // empty')
        if [ "$selected_id" != "$id" ]; then
            rm -f "$ready"
            if ! timeout 8 tailscale set --exit-node="$ip" \
                --exit-node-allow-lan-access=false >/dev/null 2>&1; then
                continue
            fi
            verified_at=0
        fi

        now=$(date +%s)
        if [ "$active_node" != "$node" ] || [ "$((now - verified_at))" -ge 60 ]; then
            result=$(curl --fail --silent --show-error --connect-timeout 4 \
                --max-time 8 --socks5-hostname 127.0.0.1:1055 \
                https://am.i.mullvad.net/json 2>/dev/null) || result='{}'
            if ! printf '%s' "$result" | jq -e '.mullvad_exit_ip == true' >/dev/null; then
                rm -f "$ready"
                continue
            fi
            verified_at=$now
        fi
        chosen=$node
        active_node=$node
        jq -n --arg node "$node" --arg id "$id" --argjson verified "$verified_at" \
            '{node:$node,id:$id,mullvad_verified_at:$verified}' >"${ready}.tmp"
        mv "${ready}.tmp" "$ready"
        break
    done
    if [ -z "$chosen" ]; then
        rm -f "$ready"
        active_node=
        primary_successes=0
        # A previously chosen but offline exit remains selected, so Tailscale
        # cannot silently revert to ordinary internet routing.
    fi
    sleep 15
done
exit 1
