#!/bin/sh
set -eu

# GPU-only diagnostics do not make network connections. Live mining and any
# diagnostic network command must go through the same strict SOCKS chain.
if [ "$#" -eq 1 ] && [ "$1" = /usr/local/bin/miner ] &&
   { [ "${GBTC_PROBE_ONLY:-0}" = 1 ] || [ "${GBTC_BENCH_ONLY:-0}" = 1 ]; }; then
    exec "$@"
fi

proxy_host=${GBTC_SOCKS5_HOST:-}
proxy_port=${GBTC_SOCKS5_PORT:-1080}

# A numeric IPv4 proxy address avoids resolving the proxy through local DNS.
# Destination names are resolved remotely by proxychains' proxy_dns hook.
if ! printf '%s\n' "$proxy_host" | awk '
    BEGIN { FS = "."; valid = 0 }
    NR == 1 && NF == 4 {
        valid = 1
        for (i = 1; i <= 4; i++) {
            if ($i !~ /^[0-9]+$/ || length($i) > 3 || $i + 0 > 255 ||
                (length($i) > 1 && substr($i, 1, 1) == "0")) valid = 0
        }
    }
    END { exit !(valid && NR == 1) }
'; then
    printf '%s\n' 'Set GBTC_SOCKS5_HOST to the reachable Mullvad SOCKS proxy IPv4 address; direct mining is disabled.' >&2
    exit 64
fi
case "$proxy_port" in
    ''|*[!0-9]*) printf '%s\n' 'GBTC_SOCKS5_PORT must be an integer from 1 to 65535.' >&2; exit 64 ;;
esac
if [ "${#proxy_port}" -gt 5 ] || [ "$proxy_port" -lt 1 ] || [ "$proxy_port" -gt 65535 ]; then
    printf '%s\n' 'GBTC_SOCKS5_PORT must be an integer from 1 to 65535.' >&2
    exit 64
fi

umask 077
proxy_config=$(mktemp /tmp/gbtc-proxychains.XXXXXX)
cat >"$proxy_config" <<EOF
strict_chain
quiet_mode
proxy_dns
remote_dns_subnet 224
tcp_read_time_out 15000
tcp_connect_time_out 8000
[ProxyList]
socks5 $proxy_host $proxy_port
EOF

# Compose readiness belongs to the exact selected and verified exit. Wait with
# no hashing when both routes are unavailable. Direct Docker use can omit it.
# Freshness window for the selected route. A missing/unverifiable file or a
# stale timestamp with the SAME exit id is tolerated briefly: the sidecar
# removes readiness while it re-verifies during transitions, and one slow
# check must not stop mining. Any observed DIFFERENT exit id (fresh or
# stale) is a real route change: stop at once so old work ends. Persistent
# absence (both exits down) still stops mining after a few checks; the
# previously selected (possibly dead) exit stays set, so traffic blackholes
# instead of routing directly.
GBTC_ROUTE_MAX_STALE_CHECKS=3
route_id() {
    [ -s "$GBTC_EXIT_READY_FILE" ] || return 1
    jq -er --argjson now "$(date +%s)" '
        select(.mullvad_verified_at <= $now and
               ($now - .mullvad_verified_at) <= 100) | .id' \
        "$GBTC_EXIT_READY_FILE" 2>/dev/null
}
route_id_any() {
    [ -s "$GBTC_EXIT_READY_FILE" ] || return 1
    jq -er '.id' "$GBTC_EXIT_READY_FILE" 2>/dev/null
}
trap 'rm -f "$proxy_config"; exit 0' INT TERM
if [ -n "${GBTC_EXIT_READY_FILE:-}" ]; then
    until initial_route=$(route_id); do sleep 5; done
fi

# Refuse to start pool traffic until this exact proxy path has a confirmed
# Mullvad exit. Checking through the wrapper also checks proxy-side DNS.
if [ "${GBTC_VERIFY_MULLVAD:-0}" = 1 ]; then
    if ! proxychains4 -q -f "$proxy_config" curl --fail --silent --show-error \
        --connect-timeout 10 --max-time 20 https://am.i.mullvad.net/json |
        grep -Eq '"mullvad_exit_ip"[[:space:]]*:[[:space:]]*true'; then
        printf '%s\n' 'Mullvad exit verification failed; refusing direct or unverified mining.' >&2
        rm -f "$proxy_config"
        exit 69
    fi
fi

# No localnet exemptions or direct fallback. Network diagnostics exec directly.
# Supervise live mining only: drop old Stratum work on route loss/change and let
# Docker's restart policy reconnect through the newly verified route.
if [ "$#" -ne 1 ] || [ "$1" != /usr/local/bin/miner ] ||
   [ -z "${GBTC_EXIT_READY_FILE:-}" ]; then
    exec proxychains4 -q -f "$proxy_config" "$@"
fi
proxychains4 -q -f "$proxy_config" "$@" &
miner_pid=$!
cleanup() {
    kill -TERM "$miner_pid" 2>/dev/null || true
    wait "$miner_pid" 2>/dev/null || true
    rm -f "$proxy_config"
}
trap 'cleanup; exit 0' INT TERM
stale_checks=0
while kill -0 "$miner_pid" 2>/dev/null; do
    sleep 10 &
    wait "$!" || true
    if current_route=$(route_id); then
        stale_checks=0
        if [ "$current_route" != "$initial_route" ]; then
            cleanup
            exit 75
        fi
        continue
    fi
    # No fresh verification: compare against the last recorded id (even if
    # stale) to tell a route change from a verification gap.
    if any_route=$(route_id_any) && [ "$any_route" != "$initial_route" ]; then
        cleanup
        exit 75
    fi
    stale_checks=$((stale_checks + 1))
    if [ "$stale_checks" -ge "${GBTC_ROUTE_MAX_STALE_CHECKS:-3}" ]; then
        cleanup
        exit 75
    fi
done
set +e
wait "$miner_pid"
result=$?
rm -f "$proxy_config"
exit "$result"
