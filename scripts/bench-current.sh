#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p bench-results

ts="$(date -u +%Y%m%dT%H%M%SZ)"
out="bench-results/current-${ts}.jsonl"
log="bench-results/current-${ts}.log"

: "${GBTC_BACKEND:=gles}"
: "${GBTC_GLES_KERNEL:=unrolled}"
: "${GBTC_GLES_LOCAL_SIZE:=64}"
: "${GBTC_BENCH_SECONDS:=60}"
: "${GBTC_BENCH_WARMUP_SECONDS:=5}"

docker compose run --rm \
  -e GBTC_BENCH_ONLY=1 \
  -e GBTC_BACKEND="${GBTC_BACKEND}" \
  -e GBTC_GLES_KERNEL="${GBTC_GLES_KERNEL}" \
  -e GBTC_GLES_LOCAL_SIZE="${GBTC_GLES_LOCAL_SIZE}" \
  -e GBTC_BENCH_SECONDS="${GBTC_BENCH_SECONDS}" \
  -e GBTC_BENCH_WARMUP_SECONDS="${GBTC_BENCH_WARMUP_SECONDS}" \
  -e GBTC_BATCH_NONCES="${GBTC_BATCH_NONCES:-16777216}" \
  miner 2>&1 | tee "${log}" | awk '/^\{/ { print }' | tee "${out}"

test -s "${out}"
printf 'wrote %s\n' "${out}" >&2
printf 'log in %s\n' "${log}" >&2
