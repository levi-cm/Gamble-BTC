#!/usr/bin/env bash
# Current-config bench helper using `docker run` directly (no compose plugin).
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p bench-results

ts="$(date -u +%Y%m%dT%H%M%SZ)"
out="bench-results/current-${ts}.jsonl"
log="bench-results/current-${ts}.log"

: "${GBTC_IMAGE:=gamble-btc:latest}"
: "${GBTC_BACKEND:=gles}"
: "${GBTC_GLES_KERNEL:=unrolled}"
: "${GBTC_GLES_LOCAL_SIZE:=64}"
: "${GBTC_OPENCL_LOCAL_SIZE:=64}"
: "${GBTC_OPENCL_SIMD:=auto}"
: "${GBTC_OPENCL_KERNEL:=unrolled}"
: "${GBTC_OPENCL_POLL_US:=1000}"
: "${GBTC_OPENCL_PLATFORM:=}"
: "${GBTC_IGC_FORCE_SIMD:=}"
: "${GBTC_BENCH_SECONDS:=60}"
: "${GBTC_BENCH_WARMUP_SECONDS:=5}"
: "${GBTC_BATCH_NONCES:=16777216}"
: "${GBTC_DRI_CARD:=/dev/dri/card1}"
: "${GBTC_DRI_RENDER:=/dev/dri/renderD128}"
: "${GBTC_VIDEO_GID:=983}"
: "${GBTC_RENDER_GID:=987}"
: "${GBTC_SHADER_CACHE:=$HOME/.cache/gbtc-mesa-shader-cache}"
: "${GBTC_VULKAN_LOCAL_SIZE:=}"
: "${GBTC_VULKAN_POLL_US:=}"
: "${GBTC_VULKAN_VALIDATION:=}"

timeout_seconds=""
for candidate in "${GBTC_BENCH_TIMEOUT_SECONDS:-}" "${GBTC_BENCH_TIMEOUT:-}" \
                 "${GBTC_TIMEOUT_SECONDS:-}" "${GBTC_TIMEOUT:-}"; do
    if [[ -n "$candidate" ]]; then
        timeout_seconds="$candidate"
        break
    fi
done
if [[ -z "$timeout_seconds" ]]; then
    timeout_seconds=$((GBTC_BENCH_SECONDS + GBTC_BENCH_WARMUP_SECONDS + 120))
fi
if ! [[ "$timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    printf 'GBTC bench timeout must be a positive integer, got: %s\n' "$timeout_seconds" >&2
    exit 2
fi

container_name="${GBTC_CONTAINER_NAME:-gbtc-current-${ts}}"

mkdir -p "$GBTC_SHADER_CACHE"

cleanup() {
    docker rm -f "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

extra_env=()
if [[ -n "$GBTC_IGC_FORCE_SIMD" ]]; then
    extra_env+=(-e "IGC_ForceOCLSIMDWidth=${GBTC_IGC_FORCE_SIMD}")
fi
if [[ -n "$GBTC_OPENCL_PLATFORM" ]]; then
    extra_env+=(-e "GBTC_OPENCL_PLATFORM=${GBTC_OPENCL_PLATFORM}")
fi

set +e
timeout --signal=TERM --kill-after=5s "${timeout_seconds}s" docker run --rm --name "$container_name" --network host \
    -e GBTC_BENCH_ONLY=1 \
    -e GBTC_BACKEND="${GBTC_BACKEND}" \
    -e GBTC_DEVICE="${GBTC_DRI_RENDER}" \
    -e GBTC_GLES_KERNEL="${GBTC_GLES_KERNEL}" \
    -e GBTC_GLES_LOCAL_SIZE="${GBTC_GLES_LOCAL_SIZE}" \
    -e GBTC_OPENCL_LOCAL_SIZE="${GBTC_OPENCL_LOCAL_SIZE}" \
    -e GBTC_OPENCL_SIMD="${GBTC_OPENCL_SIMD}" \
    -e GBTC_OPENCL_KERNEL="${GBTC_OPENCL_KERNEL}" \
    -e GBTC_OPENCL_POLL_US="${GBTC_OPENCL_POLL_US}" \
    -e GBTC_VULKAN_LOCAL_SIZE="${GBTC_VULKAN_LOCAL_SIZE}" \
    -e GBTC_VULKAN_POLL_US="${GBTC_VULKAN_POLL_US}" \
    -e GBTC_VULKAN_VALIDATION="${GBTC_VULKAN_VALIDATION}" \
    "${extra_env[@]}" \
    -e GBTC_BENCH_SECONDS="${GBTC_BENCH_SECONDS}" \
    -e GBTC_BENCH_WARMUP_SECONDS="${GBTC_BENCH_WARMUP_SECONDS}" \
    -e GBTC_BATCH_NONCES="${GBTC_BATCH_NONCES}" \
    -e MESA_NO_ERROR=1 \
    -e MESA_LOADER_DRIVER_OVERRIDE=iris \
    -e RUSTICL_ENABLE=iris \
    -e MESA_SHADER_CACHE_DIR=/home/miner/.cache/mesa_shader_cache \
    --volume "$GBTC_SHADER_CACHE:/home/miner/.cache/mesa_shader_cache" \
    --memory 4g --cpus 4.0 --ulimit memlock=-1 \
    --device "$GBTC_DRI_CARD:$GBTC_DRI_CARD" \
    --device "$GBTC_DRI_RENDER:$GBTC_DRI_RENDER" \
    --group-add "$GBTC_VIDEO_GID" \
    --group-add "$GBTC_RENDER_GID" \
    "$GBTC_IMAGE" >"$log" 2>&1
rc=$?
set -e

if [[ "$rc" -ne 0 ]]; then
    printf 'bench-current: container exited %d\n' "$rc" >&2
    exit "$rc"
fi

awk '/^[[:space:]]*\{/ {print}' "$log" >"$out"

if [[ ! -s "$out" ]]; then
    printf 'bench-current: missing or empty benchmark output\n' >&2
    exit 1
fi

if ! python3 - "$out" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as fh:
    lines = [line for line in fh if line.strip()]
if not lines:
    raise SystemExit(1)
for line in lines:
    obj = json.loads(line)
    if not isinstance(obj, dict):
        raise SystemExit(1)
    if "ok" not in obj or "backend" not in obj or "mh_s" not in obj:
        raise SystemExit(1)
PY
then
    printf 'bench-current: malformed benchmark output\n' >&2
    exit 1
fi

cat "${out}"
printf 'wrote %s\n' "${out}" >&2
printf 'log in %s\n' "${log}" >&2