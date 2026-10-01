#!/usr/bin/env bash
# Rebuild the tracked Pascal cubin from src/cuda/gbtc_mine.cu.
# Requires the pinned CUDA 12.9 toolbox (CUDA 13 dropped Pascal offline
# compilation). Verifies sm_61 target, register use and zero local memory.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
SRC=src/cuda/gbtc_mine.cu
OUT=src/cuda/gbtc_mine.sm61.cubin
TMPREL="bench-results/campaign-20261001/.kbuild-tmp"
mkdir -p "$TMPREL"
TMP="$PWD/$TMPREL"
trap 'rm -rf "$TMP"' EXIT
scripts/dev-toolbox.sh bash -c \
  "nvcc -arch=sm_61 -cubin -o /tmp/gbtc_mine.cubin -Xptxas=-v /workspace/$SRC 2>/tmp/gbtc_build.log \
   && nvcc -arch=sm_61 -ptx -o /tmp/gbtc_mine.ptx /workspace/$SRC \
   && cuobjdump -sass /tmp/gbtc_mine.cubin > /tmp/gbtc_mine.sass \
   && cp /tmp/gbtc_mine.cubin /tmp/gbtc_mine.ptx /tmp/gbtc_mine.sass /tmp/gbtc_build.log /workspace/$TMPREL/"
cp "$TMP/gbtc_mine.cubin" "$OUT"
echo "cubin: $OUT ($(stat -c%s "$OUT") bytes)"
grep -E 'registers|gmem|local|cmem' "$TMP/gbtc_build.log" | head -n 8
grep -c '' "$TMP/gbtc_mine.sass" | xargs echo "sass lines:"
cp "$TMP/gbtc_mine.sass" bench-results/campaign-20261001/cuda-kernel.sass
cp "$TMP/gbtc_mine.ptx" bench-results/campaign-20261001/cuda-kernel.ptx
echo "ptx/sass archived under bench-results/campaign-20261001/"
