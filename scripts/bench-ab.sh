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

printf 'round\tlabel\tmh_s\tact_freq_mhz\tpl1\tfreq_min_mhz\tpl1_ever\tpoll_us\tbatch_nonces\tstatus\n' >"$out"

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
    export GBTC_OPENCL_POLL_US="${kv[GBTC_OPENCL_POLL_US]:-1000}"
    export GBTC_BATCH_NONCES="${kv[GBTC_BATCH_NONCES]:-16777216}"
    export GBTC_OPENCL_PLATFORM="${kv[GBTC_OPENCL_PLATFORM]:-}"

    mh_file="$(mktemp)"
    GBTC_BACKEND=opencl GBTC_BENCH_SECONDS="$seconds" GBTC_BENCH_WARMUP_SECONDS=2 \
      ./scripts/bench-run.sh >"$mh_file" 2>/dev/null &
    bench_pid=$!
    # Sample the GPU state repeatedly through the run (a single midpoint
    # sample cannot certify an entire run). Track the minimum observed
    # frequency and whether PL1 was ever asserted.
    freq_min=-1
    pl1_ever=-1
    act=-1
    pl1=-1
    elapsed=0
    while kill -0 "$bench_pid" 2>/dev/null; do
      sleep 5
      elapsed=$((elapsed + 5))
      # Guard against a hung benchmark: stop sampling after generous slack.
      if [[ "$elapsed" -gt $((seconds + 120)) ]]; then
        break
      fi
      act="$(cat /sys/class/drm/card1/gt_act_freq_mhz 2>/dev/null || echo -1)"
      pl1="$(cat /sys/class/drm/card1/gt/gt0/throttle_reason_pl1 2>/dev/null || echo -1)"
      if [[ "$act" =~ ^[0-9]+$ ]]; then
        if [[ "$freq_min" -lt 0 || "$act" -lt "$freq_min" ]]; then
          freq_min="$act"
        fi
      fi
      if [[ "$pl1" == "0" || "$pl1" == "1" ]]; then
        if [[ "$pl1_ever" == "-1" ]]; then
          pl1_ever="$pl1"
        elif [[ "$pl1" == "1" ]]; then
          pl1_ever=1
        fi
      fi
    done
    if wait "$bench_pid"; then
      bench_status=0
    else
      bench_status=$?
    fi
    mh=""
    if [[ "$bench_status" == "0" ]]; then
      mh="$(grep -o '"mh_s":[0-9.]*' "$mh_file" | head -n1 | cut -d: -f2)"
    fi
    if [[ -z "$mh" ]]; then
      bench_status="${bench_status:-1}"
      if [[ "$bench_status" == "0" ]]; then
        bench_status=1
      fi
    fi
    rm -f "$mh_file"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$r" "$label" "$mh" "$act" "$pl1" "$freq_min" "$pl1_ever" \
      "$GBTC_OPENCL_POLL_US" "$GBTC_BATCH_NONCES" "$bench_status" | tee -a "$out"
    unset GBTC_OPENCL_KERNEL GBTC_OPENCL_SIMD GBTC_OPENCL_LOCAL_SIZE GBTC_IGC_FORCE_SIMD
    unset GBTC_OPENCL_POLL_US GBTC_BATCH_NONCES GBTC_OPENCL_PLATFORM
  done
done

printf '\n=== medians (%s rounds, %ss each) ===\n' "$rounds" "$seconds"
overall_rc=0
for label in "${labels[@]}"; do
  med="$(awk -F'\t' -v l="$label" '$2==l && $3+0>0 {print $3}' "$out" | sort -n | awk '{a[NR]=$1} END{if(NR) print (NR%2)?a[(NR+1)/2]:(a[NR/2]+a[NR/2+1])/2}')"
  fails="$(awk -F'\t' -v l="$label" '$2==l && ($3=="" || $10!="0") {n++} END{print n+0}' "$out")"
  printf '%-24s %s MH/s (failed runs: %s)\n' "$label" "${med:-n/a}" "$fails"
  if [[ "$fails" != "0" ]]; then
    overall_rc=1
  fi
done
printf '\nresults: %s\n' "$out"
exit "$overall_rc"
