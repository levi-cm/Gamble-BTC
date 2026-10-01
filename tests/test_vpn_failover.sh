#!/bin/sh
set -eu

# Run inside the Tailscale image with scripts mounted at /workspace/scripts.
# No real gateway is disrupted. Mock only the CLI/telemetry boundary while
# executing the production route manager and health check.
test_root=$(mktemp -d)
manager_pid=
cleanup() {
    if [ -n "$manager_pid" ]; then
        kill "$manager_pid" 2>/dev/null || true
        wait "$manager_pid" 2>/dev/null || true
    fi
    rm -rf "$test_root"
}
trap cleanup EXIT
mkdir "$test_root/bin"
export GBTC_TEST_STATE="$test_root/state"
export PATH="$test_root/bin:$PATH"

cat >"$test_root/bin/tailscaled" <<'EOF'
#!/bin/sh
exec /bin/sleep 600
EOF
cat >"$test_root/bin/sleep" <<'EOF'
#!/bin/sh
exec /bin/sleep 0.05
EOF
cat >"$test_root/bin/tailscale" <<'EOF'
#!/bin/sh
set -eu
state=$GBTC_TEST_STATE
selected=$(cat "$state/selected")
case "$1" in
    status)
        jq -n --arg selected "$selected" '{BackendState:"Running",
            ExitNodeStatus:{ID:$selected,Online:true},Peer:{
              p:{ID:"p",DNSName:"primary.test.",Online:true,ExitNodeOption:true,TailscaleIPs:["100.1.1.1"]},
              f:{ID:"f",DNSName:"fallback.test.",Online:true,ExitNodeOption:true,TailscaleIPs:["100.1.1.2"]}}}'
        ;;
    debug) jq -n --arg selected "$selected" '{ExitNodeID:$selected}' ;;
    set)
        case "$2" in
            --exit-node=100.1.1.1) printf p >"$state/selected" ;;
            --exit-node=100.1.1.2) printf f >"$state/selected" ;;
            *) exit 90 ;;
        esac
        ;;
    ping)
        for last do :; done
        case "$last" in
            100.1.1.1)
                [ ! -f "$state/primary-offline" ] || exit 1
                count=$(cat "$state/primary-successes")
                printf '%s' "$((count + 1))" >"$state/primary-successes"
                ;;
            100.1.1.2) [ ! -f "$state/fallback-offline" ] || exit 1 ;;
            *) exit 91 ;;
        esac
        ;;
    *) exit 92 ;;
esac
EOF
cat >"$test_root/bin/curl" <<'EOF'
#!/bin/sh
if [ "$(cat "$GBTC_TEST_STATE/selected")" = p ] &&
   [ -f "$GBTC_TEST_STATE/primary-not-mullvad" ]; then
    printf '{"mullvad_exit_ip":false}'
else
    printf '{"mullvad_exit_ip":true}'
fi
EOF
chmod 755 "$test_root/bin/"*

# Keep production paths untouched and redirect only the per-run readiness file.
sed "s|ready=/run/gbtc-route/ready.json|ready=$test_root/ready.json|" \
    /workspace/scripts/tailscale-entrypoint.sh >"$test_root/manager.sh"
sed "s|ready=/run/gbtc-route/ready.json|ready=$test_root/ready.json|" \
    /workspace/scripts/tailscale-health.sh >"$test_root/health.sh"

reset_case() {
    if [ -n "$manager_pid" ]; then
        kill "$manager_pid" 2>/dev/null || true
        wait "$manager_pid" 2>/dev/null || true
        manager_pid=
    fi
    rm -rf "$GBTC_TEST_STATE"
    rm -f "$test_root/ready.json"
    mkdir "$GBTC_TEST_STATE"
    printf p >"$GBTC_TEST_STATE/selected"
    printf 0 >"$GBTC_TEST_STATE/primary-successes"
}
start_case() {
    GBTC_EXIT_PRIMARY=primary.test GBTC_EXIT_FALLBACK=fallback.test \
        sh "$test_root/manager.sh" >"$test_root/manager.log" 2>&1 &
    manager_pid=$!
}
expect_ready() {
    want=$1
    attempt=0
    while [ "$attempt" -lt 80 ]; do
        if [ -s "$test_root/ready.json" ] &&
           [ "$(jq -r .id "$test_root/ready.json")" = "$want" ]; then
            sh "$test_root/health.sh"
            return
        fi
        kill -0 "$manager_pid" 2>/dev/null || break
        /bin/sleep 0.05
        attempt=$((attempt + 1))
    done
    cat "$test_root/manager.log" >&2
    printf 'expected verified exit %s\n' "$want" >&2
    exit 1
}

reset_case
start_case
expect_ready p
printf 'primary selection and health passed\n'

reset_case
touch "$GBTC_TEST_STATE/primary-offline"
start_case
expect_ready f
printf 'unreachable primary fallback passed\n'
rm "$GBTC_TEST_STATE/primary-offline"
printf 0 >"$GBTC_TEST_STATE/primary-successes"
expect_ready p
[ "$(cat "$GBTC_TEST_STATE/primary-successes")" -ge 3 ]
printf 'automatic failback with hysteresis passed\n'

reset_case
touch "$GBTC_TEST_STATE/primary-not-mullvad"
start_case
expect_ready f
[ "$(cat "$GBTC_TEST_STATE/selected")" = f ]
printf 'unverified primary route fallback passed\n'

reset_case
touch "$GBTC_TEST_STATE/primary-offline" "$GBTC_TEST_STATE/fallback-offline"
start_case
/bin/sleep 0.5
[ ! -e "$test_root/ready.json" ]
[ "$(cat "$GBTC_TEST_STATE/selected")" = p ]
if sh "$test_root/health.sh"; then
    printf 'both-offline route must not become healthy\n' >&2
    exit 1
fi
printf 'both exits offline: readiness denied and exit preference retained\n'
