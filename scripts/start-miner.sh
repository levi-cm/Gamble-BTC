#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

container_name="${GBTC_CONTAINER_NAME:-gamble-btc}"
image_name="${GBTC_IMAGE:-gamble-btc:latest}"
dri_card="${GBTC_DRI_CARD:-/dev/dri/card1}"
dri_render="${GBTC_DRI_RENDER:-/dev/dri/renderD128}"
video_gid="${GBTC_VIDEO_GID:-983}"
render_gid="${GBTC_RENDER_GID:-987}"
mode="start"

usage() {
    cat <<'EOF'
Usage: scripts/start-miner.sh [--dry-run|--probe]

Starts the Gamble-BTC miner with the confirmed Iris Xe GLES settings:
  GBTC_BACKEND=gles
  GBTC_GLES_KERNEL=altbool
  GBTC_GLES_LOCAL_SIZE=16
  GBTC_BATCH_NONCES=16777216
  MESA_NO_ERROR=1

Options:
  --dry-run  Print the docker command without running it.
  --probe    Run GBTC_PROBE_ONLY=1 instead of starting live mining.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            mode="dry-run"
            ;;
        --probe)
            mode="probe"
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

need_cmd docker
require_ac_power

if [[ ! -f .env ]]; then
    printf '.env is missing. Copy .env.example to .env and set a real BTC_ADDRESS first.\n' >&2
    exit 1
fi

if grep -Eq '^BTC_ADDRESS=bc1qexample' .env; then
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

if [[ "$mode" == "probe" ]]; then
    cmd=(
        docker run --rm --network host --env-file .env
        -e GBTC_PROBE_ONLY=1
        -e GBTC_BACKEND=gles
        -e GBTC_DEVICE="$dri_render"
        -e GBTC_GLES_KERNEL=altbool
        -e GBTC_GLES_LOCAL_SIZE=16
        -e GBTC_BATCH_NONCES=16777216
        -e MESA_NO_ERROR=1
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

cmd=(
    docker run -d --name "$container_name" --restart unless-stopped --network host
    --env-file .env
    -e GBTC_PROBE_ONLY=0
    -e GBTC_BENCH_ONLY=0
    -e GBTC_BACKEND=gles
    -e GBTC_DEVICE="$dri_render"
    -e GBTC_GLES_KERNEL=altbool
    -e GBTC_GLES_LOCAL_SIZE=16
    -e GBTC_BATCH_NONCES=16777216
    -e MESA_NO_ERROR=1
    --device "$dri_card:$dri_card"
    --device "$dri_render:$dri_render"
    --group-add "$video_gid"
    --group-add "$render_gid"
    "$image_name"
)

if [[ "$mode" == "dry-run" ]]; then
    print_command docker build --network host -t "$image_name" .
    print_command "${cmd[@]}"
    exit 0
fi

docker build --network host -t "$image_name" .

if docker ps -a --format '{{.Names}}' | grep -Fxq "$container_name"; then
    docker rm -f "$container_name" >/dev/null
fi

print_command "${cmd[@]}"
"${cmd[@]}"
printf 'started %s. Follow logs with: docker logs -f %s\n' "$container_name" "$container_name"
