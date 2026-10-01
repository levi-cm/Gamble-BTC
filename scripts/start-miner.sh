#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

container_name="${GBTC_CONTAINER_NAME:-gamble-btc}"
image_name="${GBTC_IMAGE:-gamble-btc:latest}"
env_file="${GBTC_ENV_FILE:-.env}"
dri_card="${GBTC_DRI_CARD:-/dev/dri/card1}"
dri_render="${GBTC_DRI_RENDER:-/dev/dri/renderD128}"
video_gid="${GBTC_VIDEO_GID:-983}"
render_gid="${GBTC_RENDER_GID:-987}"
# Tunables (same defaults as compose.yaml); override per run, e.g.
# GBTC_BACKEND=opencl GBTC_OPENCL_LOCAL_SIZE=128 scripts/start-miner.sh
backend="${GBTC_BACKEND:-opencl}"
gles_kernel="${GBTC_GLES_KERNEL:-altbool}"
gles_local="${GBTC_GLES_LOCAL_SIZE:-16}"
opencl_local="${GBTC_OPENCL_LOCAL_SIZE:-64}"
opencl_kernel="${GBTC_OPENCL_KERNEL:-unrolled}"
opencl_simd="${GBTC_OPENCL_SIMD:-auto}"
opencl_poll_us="${GBTC_OPENCL_POLL_US:-1000}"
batch_nonces="${GBTC_BATCH_NONCES:-16777216}"
shader_cache="${GBTC_SHADER_CACHE:-$HOME/.cache/gbtc-mesa-shader-cache}"
status_url="${GBTC_STATUS_URL:-http://127.0.0.1:41174/status.json}"
monitor_interval="${GBTC_STATUS_INTERVAL:-60}"
mode="monitor"
dry_run=0
cleanup_container=0

usage() {
    cat <<'EOF'
Usage: scripts/start-miner.sh [--dry-run|--probe|--detach]

Starts the Gamble-BTC miner with the confirmed Iris Xe OpenCL settings:
  GBTC_BACKEND=opencl (Intel NEO preferred, Rusticl fallback; use gles for
  the GLES path)
  GBTC_OPENCL_LOCAL_SIZE=64
  GBTC_OPENCL_KERNEL=unrolled
  GBTC_OPENCL_SIMD=auto
  GBTC_OPENCL_POLL_US=1000
  GBTC_GLES_KERNEL=altbool
  GBTC_GLES_LOCAL_SIZE=16
  GBTC_BATCH_NONCES=16777216
  MESA_NO_ERROR=1

By default the script stays open, prints a one-minute average hashrate every
GBTC_STATUS_INTERVAL seconds, and stops the container when the script exits.

Options:
  --dry-run  Print the docker command without running it.
  --probe    Run GBTC_PROBE_ONLY=1 instead of starting live mining.
  --detach   Start in the background with restart policy and exit.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            dry_run=1
            ;;
        --probe)
            mode="probe"
            ;;
        --detach)
            mode="detach"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'unknown option: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'missing required command: %s\n' "$1" >&2
        exit 1
    fi
}

require_ac_power() {
    if [[ "${GBTC_ALLOW_BATTERY:-0}" == "1" ]]; then
        return
    fi

    local saw_supply=0
    local online=0
    local path
    for path in /sys/class/power_supply/*/online; do
        [[ -e "$path" ]] || continue
        saw_supply=1
        if [[ "$(sed -n '1p' "$path")" == "1" ]]; then
            online=1
            break
        fi
    done

    if [[ "$saw_supply" == "1" && "$online" != "1" ]]; then
        printf 'AC power is not online; refusing to start. Set GBTC_ALLOW_BATTERY=1 to override.\n' >&2
        exit 1
    fi
}

print_command() {
    printf '%q ' "$@"
    printf '\n'
}

json_number() {
    local json="$1"
    local key="$2"
    printf '%s\n' "$json" | sed -nE "s/.*\"$key\":(-?[0-9]+(\\.[0-9]+)?([eE][+-]?[0-9]+)?).*/\\1/p"
}

json_string() {
    local json="$1"
    local key="$2"
    printf '%s\n' "$json" | sed -nE 's/.*"'"$key"'":"([^"]*)".*/\1/p'
}

mh_from_hash_delta() {
    local hashes="$1"
    local seconds="$2"
    awk -v hashes="$hashes" -v seconds="$seconds" 'BEGIN {
        if (seconds > 0) {
            printf "%.3f", hashes / seconds / 1000000
        } else {
            printf "0.000"
        }
    }'
}

print_hashrate_status() {
    local json="$1"
    local now="$2"
    local prev_hashes="$3"
    local prev_time="$4"
    local timestamp
    local mh
    local rolling_avg
    local accepted
    local rejected
    local uptime
    local lifetime_hashes
    local elapsed
    local hash_delta
    local interval_avg

    timestamp="$(date '+%Y-%m-%d %H:%M:%S')"
    mh="$(json_number "$json" mh)"
    rolling_avg="$(json_number "$json" avg)"
    accepted="$(json_number "$json" accepted)"
    rejected="$(json_number "$json" rejected)"
    uptime="$(json_number "$json" uptime)"
    lifetime_hashes="$(json_number "$json" lifetime_hashes)"

    mh="${mh:-0.000}"
    rolling_avg="${rolling_avg:-0.000}"
    accepted="${accepted:-0}"
    rejected="${rejected:-0}"
    uptime="${uptime:-0}"

    if [[ "$lifetime_hashes" =~ ^[0-9]+$ && "$prev_hashes" =~ ^[0-9]+$ ]]; then
        elapsed=$((now - prev_time))
        if (( elapsed > 0 && lifetime_hashes >= prev_hashes )); then
            hash_delta=$((lifetime_hashes - prev_hashes))
            interval_avg="$(mh_from_hash_delta "$hash_delta" "$elapsed")"
            printf '[%s] 1m avg=%s MH/s live=%s MH/s rolling avg=%s MH/s accepted=%s rejected=%s uptime=%ss\n' \
                "$timestamp" "$interval_avg" "$mh" "$rolling_avg" "$accepted" "$rejected" "$uptime"
            return
        fi
    fi

    printf '[%s] warming up: live=%s MH/s rolling avg=%s MH/s accepted=%s rejected=%s uptime=%ss\n' \
        "$timestamp" "$mh" "$rolling_avg" "$accepted" "$rejected" "$uptime"
}

cleanup() {
    local code=$?
    trap - EXIT INT TERM HUP
    restore_keys
    if [[ "$cleanup_container" == "1" ]]; then
        printf '\nstopping %s...\n' "$container_name"
        docker rm -f "$container_name" >/dev/null 2>&1 || true
    fi
    exit "$code"
}

# Single-key monitor commands (h=hashrate now, c=connection). Active only
# when stdin is a terminal (e.g. the kitty opened from rofi); otherwise the
# monitor keeps its plain sleep and ignores stdin entirely.
keys_enabled=0
saved_stty=""

setup_keys() {
    if [[ -t 0 ]]; then
        saved_stty="$(stty -g 2>/dev/null || true)"
        if [[ -n "$saved_stty" ]]; then
            stty -icanon -echo 2>/dev/null || true
            keys_enabled=1
        fi
    fi
}

restore_keys() {
    if [[ -n "$saved_stty" ]]; then
        stty "$saved_stty" 2>/dev/null || true
        saved_stty=""
    fi
    keys_enabled=0
}

mon_prev_hashes=""
mon_prev_time=""

poll_hashrate_once() {
    local json
    local now
    local lifetime_hashes
    now="$(date +%s)"
    json="$(curl -fsS --max-time 5 "$status_url" 2>/dev/null || true)"
    if [[ -z "$json" ]]; then
        printf '[%s] waiting for status endpoint: %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$status_url"
    else
        print_hashrate_status "$json" "$now" "$mon_prev_hashes" "$mon_prev_time"
        lifetime_hashes="$(json_number "$json" lifetime_hashes)"
        if [[ "$lifetime_hashes" =~ ^[0-9]+$ ]]; then
            mon_prev_hashes="$lifetime_hashes"
            mon_prev_time="$now"
        fi
    fi
}

poll_connection_once() {
    local json
    local timestamp
    local pool
    local worker
    local job
    local diff
    local accepted
    local rejected
    local uptime
    local backend
    json="$(curl -fsS --max-time 5 "$status_url" 2>/dev/null || true)"
    timestamp="$(date '+%Y-%m-%d %H:%M:%S')"
    if [[ -z "$json" ]]; then
        printf '[%s] waiting for status endpoint: %s\n' "$timestamp" "$status_url"
        return
    fi
    pool="$(json_string "$json" pool)"
    worker="$(json_string "$json" worker)"
    job="$(json_string "$json" job)"
    diff="$(json_number "$json" diff)"
    accepted="$(json_number "$json" accepted)"
    rejected="$(json_number "$json" rejected)"
    uptime="$(json_number "$json" uptime)"
    backend="$(json_string "$json" backend)"
    printf '[%s] conn pool=%s worker=%s job=%s diff=%s accepted=%s rejected=%s uptime=%ss backend=%s\n' \
        "$timestamp" "${pool:-?}" "${worker:-?}" "${job:-?}" "${diff:-?}" \
        "${accepted:-0}" "${rejected:-0}" "${uptime:-0}" "${backend:-?}"
}

handle_key() {
    case "$1" in
        h|H) poll_hashrate_once ;;
        c|C) poll_connection_once ;;
    esac
}

wait_interval() {
    local total="$1"
    local i
    local key
    if [[ "$keys_enabled" != "1" ]]; then
        sleep "$total" &
        local sleep_pid=$!
        wait "$sleep_pid"
        return
    fi
    for ((i = 0; i < total; i++)); do
        if read -rsn1 -t1 key; then
            handle_key "$key"
        fi
    done
}

monitor_container() {
    mon_prev_hashes=""
    mon_prev_time=""

    printf 'started %s. Status: %s\n' "$container_name" "$status_url"
    printf 'monitor interval: %ss\n' "$monitor_interval"
    printf 'container will stop when this script exits. Press Ctrl-C to stop now.\n'
    setup_keys
    if [[ "$keys_enabled" == "1" ]]; then
        printf 'keys: h=hashrate now  c=connection\n'
    fi

    while true; do
        if ! docker ps --format '{{.Names}}' | grep -Fxq "$container_name"; then
            printf '%s stopped unexpectedly. Recent logs:\n' "$container_name" >&2
            docker logs --tail 40 "$container_name" >&2 || true
            return 1
        fi

        poll_hashrate_once
        wait_interval "$monitor_interval"
    done
}

need_cmd docker
require_ac_power

if [[ "$monitor_interval" =~ ^[1-9][0-9]*$ ]]; then
    :
else
    printf 'GBTC_STATUS_INTERVAL must be a positive integer, got: %s\n' "$monitor_interval" >&2
    exit 1
fi

if [[ ! -f "$env_file" ]]; then
    printf '%s is missing. Copy .env.example to .env and set a real BTC_ADDRESS first.\n' "$env_file" >&2
    exit 1
fi

if grep -Eq '^BTC_ADDRESS=bc1qexample' "$env_file"; then
    printf '.env still uses the example BTC_ADDRESS; refusing to start live mining.\n' >&2
    exit 1
fi

if [[ ! -e "$dri_card" ]]; then
    printf 'missing DRM card device: %s\n' "$dri_card" >&2
    exit 1
fi
if [[ ! -e "$dri_render" ]]; then
    printf 'missing DRM render device: %s\n' "$dri_render" >&2
    exit 1
fi
mkdir -p "$shader_cache"

if [[ "$mode" == "probe" ]]; then
    mkdir -p "$shader_cache"
    cmd=(
        docker run --rm --network host --env-file "$env_file"
        -e GBTC_PROBE_ONLY=1
        -e GBTC_BACKEND="$backend"
        -e GBTC_DEVICE="$dri_render"
        -e GBTC_GLES_KERNEL="$gles_kernel"
        -e GBTC_GLES_LOCAL_SIZE="$gles_local"
        -e GBTC_OPENCL_LOCAL_SIZE="$opencl_local"
        -e GBTC_OPENCL_KERNEL="$opencl_kernel"
        -e GBTC_OPENCL_SIMD="$opencl_simd"
        -e GBTC_OPENCL_POLL_US="$opencl_poll_us"
        -e GBTC_BATCH_NONCES="$batch_nonces"
        -e MESA_NO_ERROR=1
        -e MESA_LOADER_DRIVER_OVERRIDE=iris
        -e RUSTICL_ENABLE=iris
        -e MESA_SHADER_CACHE_DIR=/home/miner/.cache/mesa_shader_cache
        --volume "$shader_cache:/home/miner/.cache/mesa_shader_cache"
        --device "$dri_card:$dri_card"
        --device "$dri_render:$dri_render"
        --group-add "$video_gid"
        --group-add "$render_gid"
        "$image_name"
    )
    docker build --network host -t "$image_name" .
    print_command "${cmd[@]}"
    exec "${cmd[@]}"
fi

restart_args=()
if [[ "$mode" == "detach" ]]; then
    restart_args=(--restart unless-stopped)
fi

cmd=(
    docker run -d --name "$container_name" "${restart_args[@]}" --network host
    --env-file "$env_file"
    -e GBTC_PROBE_ONLY=0
    -e GBTC_BENCH_ONLY=0
    -e GBTC_BACKEND="$backend"
    -e GBTC_DEVICE="$dri_render"
    -e GBTC_GLES_KERNEL="$gles_kernel"
    -e GBTC_GLES_LOCAL_SIZE="$gles_local"
    -e GBTC_OPENCL_LOCAL_SIZE="$opencl_local"
    -e GBTC_OPENCL_KERNEL="$opencl_kernel"
    -e GBTC_OPENCL_SIMD="$opencl_simd"
    -e GBTC_OPENCL_POLL_US="$opencl_poll_us"
    -e GBTC_BATCH_NONCES="$batch_nonces"
    -e MESA_NO_ERROR=1
    -e MESA_LOADER_DRIVER_OVERRIDE=iris
    -e RUSTICL_ENABLE=iris
    -e MESA_SHADER_CACHE_DIR=/home/miner/.cache/mesa_shader_cache
    --volume "$shader_cache:/home/miner/.cache/mesa_shader_cache"
    --memory 4g --memory-reservation 1g --cpus 4.0 --ulimit memlock=-1
    --device "$dri_card:$dri_card"
    --device "$dri_render:$dri_render"
    --group-add "$video_gid"
    --group-add "$render_gid"
    "$image_name"
)

if [[ "$dry_run" == "1" ]]; then
    print_command docker build --network host -t "$image_name" .
    print_command "${cmd[@]}"
    if [[ "$mode" == "monitor" ]]; then
        printf 'monitor interval: %ss\n' "$monitor_interval"
        printf 'container will stop when this script exits\n'
    fi
    exit 0
fi

if [[ "$mode" == "monitor" ]]; then
    need_cmd curl
fi

docker build --network host -t "$image_name" .

if docker ps -a --format '{{.Names}}' | grep -Fxq "$container_name"; then
    docker rm -f "$container_name" >/dev/null
fi

print_command "${cmd[@]}"
"${cmd[@]}"
if [[ "$mode" == "detach" ]]; then
    printf 'started %s. Follow logs with: docker logs -f %s\n' "$container_name" "$container_name"
    exit 0
fi

cleanup_container=1
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
monitor_container
