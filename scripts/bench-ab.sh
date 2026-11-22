#!/usr/bin/env bash
# Interleaved A/B benchmark for throttle-prone hardware.
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
order_file="bench-results/ab-${ts}.order"
mkdir -p bench-results

: "${GBTC_IMAGE:=gamble-btc:latest}"
: "${GBTC_BACKEND:=opencl}"
: "${GBTC_DRI_CARD:=/dev/dri/card1}"
: "${GBTC_DRI_RENDER:=/dev/dri/renderD128}"
: "${GBTC_VIDEO_GID:=983}"
: "${GBTC_RENDER_GID:=987}"
: "${GBTC_SHADER_CACHE:=$HOME/.cache/gbtc-mesa-shader-cache}"

: "${GBTC_OPENCL_KERNEL:=unrolled}"
: "${GBTC_OPENCL_SIMD:=auto}"
: "${GBTC_OPENCL_LOCAL_SIZE:=64}"
: "${GBTC_IGC_FORCE_SIMD:=}"
: "${GBTC_OPENCL_POLL_US:=1000}"
: "${GBTC_OPENCL_PLATFORM:=}"
: "${GBTC_BATCH_NONCES:=16777216}"
: "${GBTC_BENCH_TIMEOUT_SECONDS:=}"

card_name="$(basename "$GBTC_DRI_CARD")"

read_telemetry() {
    local path="$1"
    local value=""
    if [[ -r "$path" ]]; then
        value="$(cat "$path" 2>/dev/null || true)"
    fi
    if [[ -z "$value" ]]; then
        printf 'unknown'
    else
        printf '%s' "$value"
    fi
}

configs=("$@")
exec_order=()

printf 'round\tlabel\tmh_s\tact_freq_mhz\tpl1\tfreq_min_mhz\tpl1_ever\tpoll_us\tbatch_nonces\tstatus\n' >"$out"

cleanup() {
    if [[ -n "${bench_pid:-}" ]] && kill -0 "$bench_pid" 2>/dev/null; then
        kill "$bench_pid" 2>/dev/null || true
        wait "$bench_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM HUP

bench_pid=""

for r in $(seq 1 "$rounds"); do
    if (( r % 2 == 0 )); then
        start_idx=$((${#configs[@]} - 1))
        step=-1
        end_idx=-1
    else
        start_idx=0
        step=1
        end_idx=${#configs[@]}
    fi

    i=$start_idx
    while [[ "$i" != "$end_idx" ]]; do
        spec="${configs[$i]}"
        label="${spec%%:*}"
        envs="${spec#*:}"

        declare -A kv=()
        IFS=',' read -r -a pairs <<<"$envs"
        for p in "${pairs[@]}"; do
            [[ -z "$p" ]] && continue
            kv["${p%%=*}"]="${p#*=}"
        done

        opencl_kernel="${kv[GBTC_OPENCL_KERNEL]:-$GBTC_OPENCL_KERNEL}"
        opencl_simd="${kv[GBTC_OPENCL_SIMD]:-$GBTC_OPENCL_SIMD}"
        opencl_local="${kv[GBTC_OPENCL_LOCAL_SIZE]:-$GBTC_OPENCL_LOCAL_SIZE}"
        igc_force="${kv[GBTC_IGC_FORCE_SIMD]:-$GBTC_IGC_FORCE_SIMD}"
        opencl_poll="${kv[GBTC_OPENCL_POLL_US]:-$GBTC_OPENCL_POLL_US}"
        opencl_platform="${kv[GBTC_OPENCL_PLATFORM]:-$GBTC_OPENCL_PLATFORM}"
        batch_nonces="${kv[GBTC_BATCH_NONCES]:-$GBTC_BATCH_NONCES}"
        selected_backend="${kv[GBTC_BACKEND]:-$GBTC_BACKEND}"

        vulkan_local="${kv[GBTC_VULKAN_LOCAL_SIZE]:-${GBTC_VULKAN_LOCAL_SIZE:-}}"
        vulkan_poll="${kv[GBTC_VULKAN_POLL_US]:-${GBTC_VULKAN_POLL_US:-}}"
        vulkan_validation="${kv[GBTC_VULKAN_VALIDATION]:-${GBTC_VULKAN_VALIDATION:-}}"

        gles_kernel="${kv[GBTC_GLES_KERNEL]:-${GBTC_GLES_KERNEL:-altbool}}"
        gles_local="${kv[GBTC_GLES_LOCAL_SIZE]:-${GBTC_GLES_LOCAL_SIZE:-16}}"

        mh_file="$(mktemp)"
        mh_log="$(mktemp)"

        extra_env=()
        if [[ -n "$igc_force" ]]; then
            extra_env+=(-e "IGC_ForceOCLSIMDWidth=${igc_force}")
        fi
        if [[ -n "$opencl_platform" ]]; then
            extra_env+=(-e "GBTC_OPENCL_PLATFORM=${opencl_platform}")
        fi

        (
            GBTC_BACKEND="$selected_backend" \
            GBTC_IMAGE="$GBTC_IMAGE" \
            GBTC_BENCH_SECONDS="$seconds" \
            GBTC_BENCH_WARMUP_SECONDS=2 \
            GBTC_BENCH_TIMEOUT_SECONDS="${GBTC_BENCH_TIMEOUT_SECONDS}" \
            GBTC_OPENCL_KERNEL="$opencl_kernel" \
            GBTC_OPENCL_SIMD="$opencl_simd" \
            GBTC_OPENCL_LOCAL_SIZE="$opencl_local" \
            GBTC_OPENCL_POLL_US="$opencl_poll" \
            GBTC_VULKAN_LOCAL_SIZE="$vulkan_local" \
            GBTC_VULKAN_POLL_US="$vulkan_poll" \
            GBTC_VULKAN_VALIDATION="$vulkan_validation" \
            GBTC_GLES_KERNEL="$gles_kernel" \
            GBTC_GLES_LOCAL_SIZE="$gles_local" \
            GBTC_BATCH_NONCES="$batch_nonces" \
            GBTC_DRI_CARD="$GBTC_DRI_CARD" \
            GBTC_DRI_RENDER="$GBTC_DRI_RENDER" \
            GBTC_VIDEO_GID="$GBTC_VIDEO_GID" \
            GBTC_RENDER_GID="$GBTC_RENDER_GID" \
            GBTC_SHADER_CACHE="$GBTC_SHADER_CACHE" \
            ./scripts/bench-run.sh >"$mh_file" 2>"$mh_log"
        ) &
        bench_pid=$!

        exec_order+=("$label")

        act_file="/sys/class/drm/${card_name}/gt_act_freq_mhz"
        pl1_file="/sys/class/drm/${card_name}/gt/gt0/throttle_reason_pl1"
        freq_min="$(read_telemetry "$act_file")"
        pl1_ever="$(read_telemetry "$pl1_file")"
        act="$freq_min"
        pl1="$pl1_ever"

        elapsed=0
        while kill -0 "$bench_pid" 2>/dev/null; do
            sleep 1
            elapsed=$((elapsed + 1))
            if [[ "$elapsed" -gt $((seconds + 30)) ]]; then
                break
            fi
            act="$(read_telemetry "$act_file")"
            pl1="$(read_telemetry "$pl1_file")"
            if [[ "$act" != "unknown" && "$freq_min" != "unknown" ]]; then
                if [[ "$freq_min" == "unknown" || "$act" -lt "$freq_min" ]]; then
                    freq_min="$act"
                fi
            fi
            if [[ "$pl1" == "0" || "$pl1" == "1" ]]; then
                if [[ "$pl1_ever" == "unknown" ]]; then
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
        bench_pid=""

        mh=""
        if [[ "$bench_status" == "0" ]]; then
            mh="$(grep -o '"mh_s":[0-9.]*' "$mh_file" 2>/dev/null | head -n1 | cut -d: -f2)"
        fi
        if [[ -z "$mh" ]]; then
            if [[ "$bench_status" == "0" ]]; then
                bench_status=1
            fi
        fi
        rm -f "$mh_file" "$mh_log"

        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$r" "$label" "$mh" "$act" "$pl1" "$freq_min" "$pl1_ever" \
            "$opencl_poll" "$batch_nonces" "$bench_status" | tee -a "$out"

        i=$((i + step))
    done
done

printf '%s\n' "${exec_order[*]}" >"$order_file"
printf 'order: %s\n' "${exec_order[*]}" >&2
printf '# order: %s\n' "${exec_order[*]}" >>"$out"

printf '\n=== medians (%s rounds, %ss each) ===\n' "$rounds" "$seconds"
overall_rc=0

# Reconstruct labels in original config order.
labels=()
for spec in "$@"; do
    labels+=("${spec%%:*}")
done

for label in "${labels[@]}"; do
    med="$(awk -F'\t' -v l="$label" '$2==l && $3+0>0 {print $3}' "$out" | sort -n | awk '{a[NR]=$1} END{if(NR) print (NR%2)?a[(NR+1)/2]:(a[NR/2]+a[NR/2+1])/2}')"
    fails="$(awk -F'\t' -v l="$label" '$2==l && ($3=="" || $10!="0") {n++} END{print n+0}' "$out")"
    printf '%-24s %s MH/s (failed runs: %s)\n' "$label" "${med:-n/a}" "$fails"
    if [[ "$fails" != "0" ]]; then
        overall_rc=1
    fi
done

printf '\nresults: %s\n' "$out"
printf 'order file: %s\n' "$order_file"
exit "$overall_rc"