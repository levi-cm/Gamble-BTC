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
for scenario in changed missing stale stale-transient missing-transient; do
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
        stale-transient)
            # One stale read with the same exit id must NOT stop mining.
            # Deterministic: slow loop iterations to ~3 s with a sleep stub,
            # so exactly one check observes the stale file (the old code
            # exited on the first stale read; the new code needs 3).
            # Retire the wrapper started by the common setup first.
            kill -TERM "$wrapper_pid"
            set +e
            wait "$wrapper_pid" >/dev/null 2>&1
            set -e
            wrapper_pid=
            printf '#!/bin/sh\nexec /bin/sleep 3\n' > "$root/bin/sleep"
            chmod 755 "$root/bin/sleep"
            rm -f "$root/child.pid"
            now=$(date +%s)
            jq -n --argjson now "$now" '{id:"primary",mullvad_verified_at:$now}' > "$root/ready.json"
            sh /workspace/scripts/container-entrypoint.sh /usr/local/bin/miner &
            wrapper_pid=$!
            tries=0
            while [ ! -s "$root/child.pid" ]; do
                /bin/sleep 0.05
                tries=$((tries + 1)); [ "$tries" -lt 100 ]
            done
            child=$(cat "$root/child.pid")
            printf '{"id":"primary","mullvad_verified_at":0}' > "$root/ready.json"
            # One ~3 s iteration observes the stale file (strike 1 of 3);
            # the second iteration must still be pending here.
            /bin/sleep 5
            if ! kill -0 "$wrapper_pid" 2>/dev/null; then
                printf 'transient staleness stopped mining\n' >&2; exit 1
            fi
            now=$(date +%s)
            jq -n --argjson now "$now" '{id:"primary",mullvad_verified_at:$now}' > "$root/ready.json"
            if ! kill -0 "$child" 2>/dev/null; then
                printf 'child died during transient staleness\n' >&2; exit 1
            fi
            kill -TERM "$wrapper_pid"
            set +e
            wait "$wrapper_pid"
            result=$?
            set -e
            wrapper_pid=
            [ "$result" -eq 0 ]
            if kill -0 "$child" 2>/dev/null; then
                printf 'old mining process survived TERM\n' >&2; exit 1
            fi
            printf 'route guard: stale-transient keeps mining alive\n'
            continue
            ;;
        missing-transient)
            # A briefly removed readiness file (sidecar re-verification
            # gap) must NOT stop mining; persistent absence still stops.
            kill -TERM "$wrapper_pid"
            set +e
            wait "$wrapper_pid" >/dev/null 2>&1
            set -e
            wrapper_pid=
            printf '#!/bin/sh\nexec /bin/sleep 3\n' > "$root/bin/sleep"
            chmod 755 "$root/bin/sleep"
            rm -f "$root/child.pid"
            now=$(date +%s)
            jq -n --argjson now "$now" '{id:"primary",mullvad_verified_at:$now}' > "$root/ready.json"
            sh /workspace/scripts/container-entrypoint.sh /usr/local/bin/miner &
            wrapper_pid=$!
            tries=0
            while [ ! -s "$root/child.pid" ]; do
                /bin/sleep 0.05
                tries=$((tries + 1)); [ "$tries" -lt 100 ]
            done
            child=$(cat "$root/child.pid")
            rm "$root/ready.json"
            # One ~3 s iteration observes the gap (strike 1 of 3).
            /bin/sleep 5
            if ! kill -0 "$wrapper_pid" 2>/dev/null; then
                printf 'transient readiness gap stopped mining\n' >&2; exit 1
            fi
            now=$(date +%s)
            jq -n --argjson now "$now" '{id:"primary",mullvad_verified_at:$now}' > "$root/ready.json"
            if ! kill -0 "$child" 2>/dev/null; then
                printf 'child died during transient gap\n' >&2; exit 1
            fi
            kill -TERM "$wrapper_pid"
            set +e
            wait "$wrapper_pid"
            result=$?
            set -e
            wrapper_pid=
            [ "$result" -eq 0 ]
            if kill -0 "$child" 2>/dev/null; then
                printf 'old mining process survived TERM\n' >&2; exit 1
            fi
            printf 'route guard: missing-transient keeps mining alive\n'
            continue
            ;;
    esac
    set +e
    waited=0
    while kill -0 "$wrapper_pid" 2>/dev/null && [ "$waited" -lt 300 ]; do
        /bin/sleep 0.05
        waited=$((waited + 1))
    done
    if kill -0 "$wrapper_pid" 2>/dev/null; then
        printf 'route guard did not stop on %s within deadline\n' "$scenario" >&2; exit 1
    fi
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
