#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p bench-results

ts="$(date -u +%Y%m%dT%H%M%SZ)"
out="bench-results/gles-matrix-${ts}.jsonl"
log_dir="bench-results/gles-matrix-${ts}-logs"
mkdir -p "${log_dir}"

: "${GBTC_BENCH_SECONDS:=60}"
: "${GBTC_BENCH_WARMUP_SECONDS:=5}"
: "${GBTC_BATCH_NONCES:=16777216}"

kernels=(unrolled partial looped altbool dualnonce)
local_sizes=(8 16 32 64 128 256)

json_escape() {
  local s=${1//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\r'/}
  printf '"%s"' "$s"
}

for kernel in "${kernels[@]}"; do
  for local_size in "${local_sizes[@]}"; do
    case_log="${log_dir}/${kernel}-${local_size}.log"
    printf 'running kernel=%s local_size=%s\n' "${kernel}" "${local_size}" >&2
    tmp="$(mktemp)"
    set +e
    docker compose run --rm \
      -e GBTC_BENCH_ONLY=1 \
      -e GBTC_BACKEND=gles \
      -e GBTC_GLES_KERNEL="${kernel}" \
      -e GBTC_GLES_LOCAL_SIZE="${local_size}" \
      -e GBTC_BENCH_SECONDS="${GBTC_BENCH_SECONDS}" \
      -e GBTC_BENCH_WARMUP_SECONDS="${GBTC_BENCH_WARMUP_SECONDS}" \
      -e GBTC_BATCH_NONCES="${GBTC_BATCH_NONCES}" \
      miner > "${tmp}" 2>&1
    rc=$?
    set -e
    cp "${tmp}" "${case_log}"
    json_lines="$(awk '/^\{/ { print }' "${tmp}")"
    if [ "${rc}" -eq 0 ] && [ -n "${json_lines}" ]; then
      printf '%s\n' "${json_lines}" | tee -a "${out}"
    else
      printf '{"ok":false,"backend":"gles","kernel":' >> "${out}"
      json_escape "${kernel}" >> "${out}"
      printf ',"local_size":%s,"batch_nonces":%s,"seconds":0,"hashes":0,"mh_s":0,"compile_result":' \
        "${local_size}" "${GBTC_BATCH_NONCES}" >> "${out}"
      json_escape "$(cat "${tmp}")" >> "${out}"
      printf ',"error":"docker compose run failed with exit %s"}\n' "${rc}" >> "${out}"
      tail -n 20 "${case_log}" >&2 || true
    fi
    rm -f "${tmp}"
  done
done

printf 'wrote %s\nlogs in %s\n' "${out}" "${log_dir}" >&2
