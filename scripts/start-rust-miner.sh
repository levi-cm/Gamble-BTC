#!/usr/bin/env bash
# Explicit Rust candidate launcher; keeps the reference service/image separate.
set -euo pipefail
cd "$(dirname "$0")/.."
image=${GBTC_RUST_IMAGE:-gamble-btc:rust-while-xmrig}
device=${GBTC_DEVICE:-/dev/dri/renderD128}
mode=${1:-mine}
case "$mode" in
  mine|--probe|--bench) ;;
  *) printf 'Usage: %s [--probe|--bench]\n' "$0" >&2; exit 2 ;;
esac
[[ -c "$device" ]] || { printf 'Render device unavailable: %s\n' "$device" >&2; exit 1; }
# Preserve the reference launcher's battery opt-in, without changing power policy.
if [[ "$mode" != --probe && "${GBTC_ALLOW_BATTERY:-0}" != 1 ]]; then
  saw_supply=0
  online=0
  for supply in /sys/class/power_supply/*/online; do
    [[ -r "$supply" ]] || continue
    saw_supply=1
    read -r value < "$supply"
    if [[ "$value" == 1 ]]; then online=1; break; fi
  done
  if [[ "$saw_supply" == 1 && "$online" != 1 ]]; then
    printf 'AC power is not online; set GBTC_ALLOW_BATTERY=1 to opt in.\n' >&2
    exit 1
  fi
fi
args=(docker run --rm --init --name "${GBTC_RUST_CONTAINER:-gbtc-rust-while-xmrig}"
  --network host --device "$device:$device" --group-add "$(stat -c '%g' "$device")"
  -e "GBTC_DEVICE=$device" -e "GBTC_BACKEND=${GBTC_BACKEND:-vulkan}"
  -e GBTC_GLES_KERNEL= -e GBTC_GLES_LOCAL_SIZE= -e GBTC_GLES_AUTOTUNE=
  -e GBTC_HTTP_BIND=127.0.0.1 -e "GBTC_HTTP_PORT=${GBTC_HTTP_PORT:-41176}")
if [[ "$mode" == mine ]]; then
  [[ -f .env ]] || { printf 'Create .env from .env.example and set BTC_ADDRESS.\n' >&2; exit 1; }
  # Environment-file precedes explicit overrides so stale GLES/HTTP values are replaced.
  args=("${args[@]:0:3}" --env-file .env "${args[@]:3}")
elif [[ "$mode" == --probe ]]; then
  args+=(-e GBTC_PROBE_ONLY=1)
else
  args+=(-e GBTC_BENCH_ONLY=1 -e "GBTC_BENCH_SECONDS=${GBTC_BENCH_SECONDS:-60}"
    -e "GBTC_BENCH_WARMUP_SECONDS=${GBTC_BENCH_WARMUP_SECONDS:-30}")
fi
for key in GBTC_VULKAN_KERNEL GBTC_VULKAN_LOCAL_SIZE GBTC_VULKAN_POLL_US GBTC_VULKAN_WAIT_MODE GBTC_VULKAN_WAIT_US GBTC_GPU_QUEUE_MS \
  GBTC_OPENCL_KERNEL GBTC_OPENCL_LOCAL_SIZE GBTC_OPENCL_SIMD GBTC_OPENCL_PLATFORM \
  GBTC_OPENCL_POLL_US GBTC_BATCH_NONCES; do
  if [[ -v "$key" ]]; then args+=(-e "$key=${!key}"); fi
done
exec "${args[@]}" "$image"
