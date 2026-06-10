#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p bench-results

kernel="${1:-${GBTC_GLES_KERNEL:-unrolled}}"
local_size="${2:-${GBTC_GLES_LOCAL_SIZE:-64}}"
ts="$(date -u +%Y%m%dT%H%M%SZ)"
out="bench-results/mesa-diag-${kernel}-${local_size}-${ts}.log"

: "${GBTC_BENCH_SECONDS:=15}"
: "${GBTC_BENCH_WARMUP_SECONDS:=3}"
: "${GBTC_BATCH_NONCES:=16777216}"

docker compose run --rm \
  -e GBTC_BENCH_ONLY=1 \
  -e GBTC_BACKEND=gles \
  -e GBTC_GLES_KERNEL="${kernel}" \
  -e GBTC_GLES_LOCAL_SIZE="${local_size}" \
  -e GBTC_BENCH_SECONDS="${GBTC_BENCH_SECONDS}" \
  -e GBTC_BENCH_WARMUP_SECONDS="${GBTC_BENCH_WARMUP_SECONDS}" \
  -e GBTC_BATCH_NONCES="${GBTC_BATCH_NONCES}" \
  -e INTEL_DEBUG="${INTEL_DEBUG:-cs,shader_time}" \
  -e MESA_DEBUG="${MESA_DEBUG:-context}" \
  -e MESA_SHADER_CACHE_DISABLE="${MESA_SHADER_CACHE_DISABLE:-true}" \
  miner > "${out}" 2>&1

cat "${out}"
printf 'wrote %s\n' "${out}" >&2
