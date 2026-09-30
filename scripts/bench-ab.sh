#!/usr/bin/env bash
# Interleaved A/B benchmark for throttle-prone hardware.
#
# Naive "run each config for 60s" benchmarking is unreliable here: the iGPU
# shares a package power budget with the CPU (throttle_reason_pl1) and drifts
# with load and heat, so later configs get systematically different clocks.
# This harness interleaves the configs round-robin and reports medians, so
# drift hits every config equally.
#
#   scripts/bench-ab.sh ROUNDS SECONDS "label:ENV=V,ENV=V" "label:ENV=V" ...
#
# Example:
#   scripts/bench-ab.sh 4 15 "auto:GBTC_OPENCL_SIMD=auto" "simd32:GBTC_OPENCL_SIMD=32"
set -euo pipefail

cd "$(dirname "$0")/.."

rounds="${1:?usage: bench-ab.sh ROUNDS SECONDS LABEL:ENV=V ...}"
seconds="${2:?usage: bench-ab.sh ROUNDS SECONDS LABEL:ENV=V ...}"
shift 2
if [[ $# -lt 1 ]]; then
  echo "need at least one config" >&2
  exit 2
fi

ts="$(date -u +%Y%m%dT%H%M%SZ)"
out="bench-results/ab-${ts}.tsv"
mkdir -p bench-results

printf 'round\tlabel\tmh_s\tact_freq_mhz\tpl1\n' >"$out"

labels=()
for spec in "$@"; do
  labels+=("${spec%%:*}")
done

for r in $(seq 1 "$rounds"); do
  for spec in "$@"; do
    label="${spec%%:*}"
    envs="${spec#*:}"
    declare -A kv=()
    IFS=',' read -r -a pairs <<<"$envs"
    for p in "${pairs[@]}"; do
      [[ -z "$p" ]] && continue
      kv["${p%%=*}"]="${p#*=}"
    done
    export GBTC_OPENCL_KERNEL="${kv[GBTC_OPENCL_KERNEL]:-unrolled}"
    export GBTC_OPENCL_SIMD="${kv[GBTC_OPENCL_SIMD]:-auto}"
    export GBTC_OPENCL_LOCAL_SIZE="${kv[GBTC_OPENCL_LOCAL_SIZE]:-64}"
    export GBTC_IGC_FORCE_SIMD="${kv[GBTC_IGC_FORCE_SIMD]:-}"

    mh_file="$(mktemp)"
    GBTC_BACKEND=opencl GBTC_BENCH_SECONDS="$seconds" GBTC_BENCH_WARMUP_SECONDS=2 \
      ./scripts/bench-run.sh >"$mh_file" 2>/dev/null &
    bench_pid=$!
    # Sample the GPU state mid-run so throttling is visible, not the post-run
    # idle state.
    sleep "$(( seconds / 2 + 2 ))"
    act="$(cat /sys/class/drm/card1/gt_act_freq_mhz 2>/dev/null || echo -1)"
    pl1="$(cat /sys/class/drm/card1/gt/gt0/throttle_reason_pl1 2>/dev/null || echo -1)"
    wait "$bench_pid" || true
    mh="$(grep -o '"mh_s":[0-9.]*' "$mh_file" | head -n1 | cut -d: -f2)"
    rm -f "$mh_file"
    printf '%s\t%s\t%s\t%s\t%s\n' "$r" "$label" "$mh" "$act" "$pl1" | tee -a "$out"
    unset GBTC_OPENCL_KERNEL GBTC_OPENCL_SIMD GBTC_OPENCL_LOCAL_SIZE GBTC_IGC_FORCE_SIMD
  done
done

printf '\n=== medians (%s rounds, %ss each) ===\n' "$rounds" "$seconds"
for label in "${labels[@]}"; do
  med="$(awk -F'\t' -v l="$label" '$2==l && $3+0>0 {print $3}' "$out" | sort -n | awk '{a[NR]=$1} END{if(NR) print (NR%2)?a[(NR+1)/2]:(a[NR/2]+a[NR/2+1])/2}')"
  printf '%-24s %s MH/s\n' "$label" "${med:-n/a}"
done
printf '\nresults: %s\n' "$out"
