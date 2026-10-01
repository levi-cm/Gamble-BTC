#!/usr/bin/env bash
# Bench helper that uses `docker run` directly (works without the compose
# plugin). Mirrors scripts/bench-current.sh semantics.
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p bench-results

ts="$(date -u +%Y%m%dT%H%M%SZ)"
out="bench-results/run-${ts}.jsonl"
log="bench-results/run-${ts}.log"

: "${GBTC_IMAGE:=gamble-btc:latest}"
: "${GBTC_BACKEND:=gles}"
: "${GBTC_GLES_KERNEL:=altbool}"
: "${GBTC_GLES_LOCAL_SIZE:=16}"
: "${GBTC_OPENCL_LOCAL_SIZE:=64}"
: "${GBTC_OPENCL_SIMD:=auto}"
: "${GBTC_OPENCL_KERNEL:=unrolled}"
: "${GBTC_OPENCL_POLL_US:=1000}"
# Optional OpenCL platform substring (e.g. rusticl); empty prefers NEO.
: "${GBTC_OPENCL_PLATFORM:=}"
# IGC knob (diagnostic): force a SIMD width the compiler would not pick itself.
: "${GBTC_IGC_FORCE_SIMD:=}"
: "${GBTC_BENCH_SECONDS:=60}"
: "${GBTC_BENCH_WARMUP_SECONDS:=5}"
: "${GBTC_BATCH_NONCES:=16777216}"
: "${GBTC_DRI_CARD:=/dev/dri/card1}"
: "${GBTC_DRI_RENDER:=/dev/dri/renderD128}"
: "${GBTC_VIDEO_GID:=983}"
: "${GBTC_RENDER_GID:=987}"
: "${GBTC_SHADER_CACHE:=$HOME/.cache/gbtc-mesa-shader-cache}"

mkdir -p "$GBTC_SHADER_CACHE"

extra_env=()
if [[ -n "$GBTC_IGC_FORCE_SIMD" ]]; then
  extra_env+=(-e "IGC_ForceOCLSIMDWidth=${GBTC_IGC_FORCE_SIMD}")
fi
if [[ -n "$GBTC_OPENCL_PLATFORM" ]]; then
  extra_env+=(-e "GBTC_OPENCL_PLATFORM=${GBTC_OPENCL_PLATFORM}")
fi

docker run --rm --network host \
  -e GBTC_BENCH_ONLY=1 \
  -e GBTC_BACKEND="${GBTC_BACKEND}" \
  -e GBTC_DEVICE="${GBTC_DRI_RENDER}" \
  -e GBTC_GLES_KERNEL="${GBTC_GLES_KERNEL}" \
  -e GBTC_GLES_LOCAL_SIZE="${GBTC_GLES_LOCAL_SIZE}" \
  -e GBTC_OPENCL_LOCAL_SIZE="${GBTC_OPENCL_LOCAL_SIZE}" \
  -e GBTC_OPENCL_SIMD="${GBTC_OPENCL_SIMD}" \
  -e GBTC_OPENCL_KERNEL="${GBTC_OPENCL_KERNEL}" \
  -e GBTC_OPENCL_POLL_US="${GBTC_OPENCL_POLL_US}" \
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
  "$GBTC_IMAGE" 2>&1 | tee "${log}" | awk '/^\{/ { print }' | tee "${out}"

test -s "${out}"
printf 'wrote %s\n' "${out}" >&2
printf 'log in %s\n' "${log}" >&2
