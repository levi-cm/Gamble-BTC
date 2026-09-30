#!/usr/bin/env bash
# Dump Intel Graphics Compiler output (LLVM IR, vISA asm, GenISA) for the
# OpenCL kernel and copy it back to the host for inspection.
#
#   scripts/igc-dump.sh              # dump with current defaults
#   GBTC_OPENCL_SIMD=16 scripts/igc-dump.sh
#
# Compilation happens at backend init, so a short bench run is enough.
set -euo pipefail

cd "$(dirname "$0")/.."

: "${GBTC_IMAGE:=gamble-btc:latest}"
: "${GBTC_OPENCL_LOCAL_SIZE:=64}"
: "${GBTC_OPENCL_SIMD:=auto}"
: "${GBTC_OPENCL_KERNEL:=unrolled}"
: "${GBTC_BATCH_NONCES:=16777216}"
: "${GBTC_DRI_CARD:=/dev/dri/card1}"
: "${GBTC_DRI_RENDER:=/dev/dri/renderD128}"
: "${GBTC_VIDEO_GID:=983}"
: "${GBTC_RENDER_GID:=987}"
: "${GBTC_SHADER_CACHE:=$HOME/.cache/gbtc-mesa-shader-cache}"

dump_dir="$PWD/bench-results/igc-dump-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$dump_dir" "$GBTC_SHADER_CACHE"

docker run --rm --network host \
  -e GBTC_BENCH_ONLY=1 \
  -e GBTC_BENCH_SECONDS=1 \
  -e GBTC_BENCH_WARMUP_SECONDS=1 \
  -e GBTC_BACKEND=opencl \
  -e GBTC_DEVICE="${GBTC_DRI_RENDER}" \
  -e GBTC_OPENCL_LOCAL_SIZE="${GBTC_OPENCL_LOCAL_SIZE}" \
  -e GBTC_OPENCL_SIMD="${GBTC_OPENCL_SIMD}" \
  -e GBTC_OPENCL_KERNEL="${GBTC_OPENCL_KERNEL}" \
  -e GBTC_BATCH_NONCES="${GBTC_BATCH_NONCES}" \
  -e IGC_ShaderDumpEnable=1 \
  -e IGC_DumpToCustomDir=/igc-dump \
  -e IGC_ShaderDumpPidDisable=1 \
  -e MESA_NO_ERROR=1 \
  -e RUSTICL_ENABLE=iris \
  --volume "$dump_dir:/igc-dump" \
  --memory 4g --cpus 2.0 --ulimit memlock=-1 \
  --device "$GBTC_DRI_CARD:$GBTC_DRI_CARD" \
  --device "$GBTC_DRI_RENDER:$GBTC_DRI_RENDER" \
  --group-add "$GBTC_VIDEO_GID" \
  --group-add "$GBTC_RENDER_GID" \
  "$GBTC_IMAGE" >/dev/null 2>&1 || true

printf 'dumped into %s\n' "$dump_dir" >&2
find "$dump_dir" -type f \( -name '*.visaasm' -o -name '*.asm' -o -name '*.isa' \) -printf '%p\n' 2>/dev/null | head -n 20 >&2
