#!/bin/sh
set -eu
# Run in the miner image with --entrypoint sh and /workspace mounted read-only.
root=$(mktemp -d)
wrapper_pid=
cleanup() {
    [ -z "$wrapper_pid" ] || kill "$wrapper_pid" 2>/dev/null || true
    rm -rf "$root"
}
trap cleanup EXIT
mkdir "$root/bin"
export GBTC_TEST_ROOT="$root"
export PATH="$root/bin:$PATH"
export GBTC_SOCKS5_HOST=127.0.0.1 GBTC_SOCKS5_PORT=1055 GBTC_VERIFY_MULLVAD=1
export GBTC_EXIT_READY_FILE="$root/ready.json"
cat > "$root/bin/proxychains4" <<'EOF'
#!/bin/sh
case "$*" in
    *curl*) printf '{"mullvad_exit_ip":true}\n' ;;
    *) printf '%s' "$$" > "$GBTC_TEST_ROOT/child.pid"; exec /bin/sleep 600 ;;
esac
EOF
cat > "$root/bin/sleep" <<'EOF'
#!/bin/sh
exec /bin/sleep 0.05
EOF
chmod 755 "$root/bin/"*
for scenario in changed missing stale; do
    now=$(date +%s)
    jq -n --argjson now "$now" '{id:"primary",mullvad_verified_at:$now}' > "$root/ready.json"
    rm -f "$root/child.pid"
    sh /workspace/scripts/container-entrypoint.sh /usr/local/bin/miner &
    wrapper_pid=$!
    tries=0
    while [ ! -s "$root/child.pid" ]; do
        /bin/sleep 0.05
        tries=$((tries + 1)); [ "$tries" -lt 100 ]
    done
    child=$(cat "$root/child.pid")
    case "$scenario" in
        changed) jq -n --argjson now "$now" '{id:"fallback",mullvad_verified_at:$now}' > "$root/ready.json" ;;
        missing) rm "$root/ready.json" ;;
        stale) printf '{"id":"primary","mullvad_verified_at":0}' > "$root/ready.json" ;;
    esac
    set +e
    wait "$wrapper_pid"
    result=$?
    set -e
    wrapper_pid=
    [ "$result" -eq 75 ]
    if kill -0 "$child" 2>/dev/null; then
        printf 'old mining process survived %s\n' "$scenario" >&2; exit 1
    fi
    printf 'route guard: %s stops old work and requests restart\n' "$scenario"
done
