#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"

cat >"$fake_bin/docker" <<'FAKE_DOCKER'
#!/usr/bin/env bash
printf 'docker' >>"${FAKE_DOCKER_LOG:?}"
printf ' %q' "$@" >>"$FAKE_DOCKER_LOG"
printf '\n' >>"$FAKE_DOCKER_LOG"

case "${1:-}" in
  build)
    exit 0
    ;;
  ps)
    printf 'gamble-btc\n'
    exit 0
    ;;
  run)
    printf 'fake-container-id\n'
    exit 0
    ;;
  rm)
    exit 0
    ;;
  logs)
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
FAKE_DOCKER

cat >"$fake_bin/curl" <<'FAKE_CURL'
#!/usr/bin/env bash
count_file="${FAKE_CURL_COUNT:?}"
count=0
if [[ -f "$count_file" ]]; then
  count="$(sed -n '1p' "$count_file")"
fi
count=$((count + 1))
printf '%s\n' "$count" >"$count_file"
hashes=$((count * 60000000))
printf '{"mh":60.000,"avg":58.500,"uptime":%d,"accepted":1,"rejected":0,"lifetime_hashes":%d,"pool":"stratum+tcp://test-pool:3333","worker":"test-worker","job":"testjob01","diff":10000.0,"backend":"opencl"}\n' "$count" "$hashes"
FAKE_CURL

chmod +x "$fake_bin/docker" "$fake_bin/curl"

touch "$tmp_dir/card" "$tmp_dir/render"
cat >"$tmp_dir/env" <<'EOF_ENV'
BTC_ADDRESS=bc1qtestaddress
POOL_URL=stratum+tcp://public-pool.io:3333
WORKER_NAME=test-worker
EOF_ENV

run_start_script() {
  run_start_script_direct "$@"
}

run_start_script_direct() {
  env \
    PATH="$fake_bin:$PATH" \
    FAKE_DOCKER_LOG="$tmp_dir/docker.log" \
    FAKE_CURL_COUNT="$tmp_dir/curl-count" \
    GBTC_ALLOW_BATTERY=1 \
    GBTC_DRI_CARD="$tmp_dir/card" \
    GBTC_DRI_RENDER="$tmp_dir/render" \
    GBTC_ENV_FILE="$tmp_dir/env" \
    GBTC_STATUS_INTERVAL=1 \
    "$repo_root/scripts/start-miner.sh" "$@"
}

assert_contains() {
  local text="$1"
  local needle="$2"
  if [[ "$text" != *"$needle"* ]]; then
    printf 'expected output to contain: %s\n' "$needle" >&2
    printf 'actual output:\n%s\n' "$text" >&2
    exit 1
  fi
}

assert_not_contains() {
  local text="$1"
  local needle="$2"
  if [[ "$text" == *"$needle"* ]]; then
    printf 'expected output not to contain: %s\n' "$needle" >&2
    printf 'actual output:\n%s\n' "$text" >&2
    exit 1
  fi
}

dry_run_output="$(run_start_script --dry-run)"
assert_contains "$dry_run_output" "docker run -d --name gamble-btc"
assert_contains "$dry_run_output" "monitor interval: 1s"
assert_contains "$dry_run_output" "container will stop when this script exits"
assert_not_contains "$dry_run_output" "--restart unless-stopped"

detach_output="$(run_start_script --detach --dry-run)"
assert_contains "$detach_output" "--restart unless-stopped"
assert_not_contains "$detach_output" "container will stop when this script exits"

rm -f "$tmp_dir/docker.log" "$tmp_dir/curl-count"
set +e
timeout --preserve-status -s TERM -k 5s 3s env \
  PATH="$fake_bin:$PATH" \
  FAKE_DOCKER_LOG="$tmp_dir/docker.log" \
  FAKE_CURL_COUNT="$tmp_dir/curl-count" \
  GBTC_ALLOW_BATTERY=1 \
  GBTC_DRI_CARD="$tmp_dir/card" \
  GBTC_DRI_RENDER="$tmp_dir/render" \
  GBTC_ENV_FILE="$tmp_dir/env" \
  GBTC_STATUS_INTERVAL=1 \
  "$repo_root/scripts/start-miner.sh" >"$tmp_dir/monitor.out" 2>"$tmp_dir/monitor.err"
monitor_status=$?
set -e
if [[ "$monitor_status" -ne 143 && "$monitor_status" -ne 124 ]]; then
  printf 'expected monitor to exit from timeout signal, got %s\n' "$monitor_status" >&2
  printf 'stdout:\n%s\n' "$(cat "$tmp_dir/monitor.out")" >&2
  printf 'stderr:\n%s\n' "$(cat "$tmp_dir/monitor.err")" >&2
  exit 1
fi

monitor_output="$(cat "$tmp_dir/monitor.out")"
assert_contains "$monitor_output" "1m avg="
assert_contains "$monitor_output" "stopping gamble-btc"
assert_contains "$(cat "$tmp_dir/docker.log")" "docker rm -f gamble-btc"

# Single-key monitor commands run under a pty so stdin is a terminal and the
# key layer engages (proven by the keys hint line). Feeding 'hchc' must yield
# extra on-demand hashrate lines plus connection lines.
if command -v script >/dev/null 2>&1; then
  rm -f "$tmp_dir/curl-count"
  set +e
  # shellcheck disable=SC2094
  (printf 'hchc'; sleep 8) | timeout --preserve-status -s KILL -k 2s 12s script -qec \
    "timeout --preserve-status -s TERM -k 2s 5s env PATH=\"$fake_bin:$PATH\" FAKE_DOCKER_LOG=\"$tmp_dir/docker.log\" FAKE_CURL_COUNT=\"$tmp_dir/curl-count\" GBTC_ALLOW_BATTERY=1 GBTC_DRI_CARD=\"$tmp_dir/card\" GBTC_DRI_RENDER=\"$tmp_dir/render\" GBTC_ENV_FILE=\"$tmp_dir/env\" GBTC_STATUS_INTERVAL=1 \"$repo_root/scripts/start-miner.sh\"" \
    /dev/null >"$tmp_dir/keys.out" 2>"$tmp_dir/keys.err"
  keys_status=$?
  set -e
  if [[ "$keys_status" -ne 143 ]]; then
    printf 'expected keys run to exit 143 from inner timeout, got %s\n' "$keys_status" >&2
    printf 'stdout:\n%s\n' "$(cat "$tmp_dir/keys.out")" >&2
    exit 1
  fi
  keys_output="$(cat "$tmp_dir/keys.out")"
  assert_contains "$keys_output" "keys: h=hashrate now"
  assert_contains "$keys_output" "conn pool=stratum+tcp://test-pool:3333"
  # Each poll (regular or h-triggered) prints exactly one hashrate line
  # ("warming up" or "1m avg="); 5 regular polls + 2 h polls are expected.
  status_lines="$(printf '%s\n' "$keys_output" | grep -c 'MH/s' || true)"
  if [[ "$status_lines" -lt 6 ]]; then
    printf 'expected at least 6 hashrate lines with h keys pressed, got %s\n' "$status_lines" >&2
    printf 'stdout:\n%s\n' "$keys_output" >&2
    exit 1
  fi
else
  printf 'skipping monitor key test: script(1) not available\n' >&2
fi

printf 'start-miner script tests passed\n'
