#!/usr/bin/env bash
set -euo pipefail

container_name="${GBTC_CONTAINER_NAME:-gamble-btc}"
mode="stop"

usage() {
    cat <<'EOF'
Usage: scripts/stop-miner.sh [--dry-run]

Stops and removes the Gamble-BTC miner container if it exists.

Options:
  --dry-run  Print the docker command without running it.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            mode="dry-run"
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

if ! command -v docker >/dev/null 2>&1; then
    printf 'missing required command: docker\n' >&2
    exit 1
fi

cmd=(docker rm -f "$container_name")

if [[ "$mode" == "dry-run" ]]; then
    printf '%q ' "${cmd[@]}"
    printf '\n'
    exit 0
fi

if ! docker ps -a --format '{{.Names}}' | grep -Fxq "$container_name"; then
    printf '%s is not running or present.\n' "$container_name"
    exit 0
fi

"${cmd[@]}"
printf 'stopped %s\n' "$container_name"
