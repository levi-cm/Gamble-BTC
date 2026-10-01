#!/usr/bin/env bash
# NVIDIA bench harness for the GTX 1060 campaign.
# Runs ONE serialized synthetic benchmark trial (GBTC_BENCH_ONLY=1, no wallet,
# --network none) in a disposable GPU container and records JSON + telemetry.
#
# The caller must stop the production miner service first for uncontended
# results; this script refuses to run while gpu-workload.lock names another
# owner, while the thermal stop flag is set, or while the GPU is too hot.
#
# Env knobs (all explicit, no host forwarding):
#   IMAGE (default gamble-btc:latest), TAG (required),
#   GBTC_OPENCL_KERNEL, GBTC_OPENCL_LOCAL_SIZE, GBTC_OPENCL_SIMD,
#   GBTC_OPENCL_POLL_US, GBTC_CUDA_BLOCK, GBTC_BATCH_NONCES, GBTC_BENCH_SECONDS,
#   GBTC_BENCH_WARMUP_SECONDS, GBTC_BACKEND, OUTDIR (default bench-results/campaign-20261001)
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
OUTDIR="${OUTDIR:-bench-results/campaign-20261001}"
LOCK="$OUTDIR/gpu-workload.lock"
STOPFLAG="$OUTDIR/thermal-stop.flag"
IMAGE="${IMAGE:-gamble-btc:latest}"
TAG="${TAG:?TAG is required}"
BACKEND="${GBTC_BACKEND:-opencl}"
CUBLOCK="${GBTC_CUDA_BLOCK:-64}"
KERNEL="${GBTC_OPENCL_KERNEL:-unrolled}"
LOCAL="${GBTC_OPENCL_LOCAL_SIZE:-64}"
SIMD="${GBTC_OPENCL_SIMD:-auto}"
POLL="${GBTC_OPENCL_POLL_US:-1000}"
BATCH="${GBTC_BATCH_NONCES:-16777216}"
SECS="${GBTC_BENCH_SECONDS:-60}"
WARMUP="${GBTC_BENCH_WARMUP_SECONDS:-5}"
mkdir -p "$OUTDIR"
CSV="$OUTDIR/bench-trials.csv"
[ -f "$CSV" ] || echo "ts_utc,tag,image,backend,kernel_ocl,local_ocl,simd,poll_us,cublock,batch,bench_s,warmup_s,mh_s,hashes,seconds,ok,cpu_steady_pct,cpu_full_pct,temp_before,temp_after,sm_mhz,power_w,note" > "$CSV"

snap() { nvidia-smi --query-gpu=temperature.gpu,clocks.current.sm,power.draw --format=csv,noheader,nounits 2>/dev/null | head -n1 | tr -d ' '; }

if [ -f "$STOPFLAG" ]; then echo "REFUSE: thermal stop flag set" >&2; exit 3; fi
if [ -f "$LOCK" ]; then echo "REFUSE: gpu-workload.lock owned by $(cat "$LOCK")" >&2; exit 3; fi
TEMP0="$(snap | cut -d, -f1)"
if [ "${TEMP0%.*}" -gt 75 ] 2>/dev/null; then echo "REFUSE: GPU at ${TEMP0}C" >&2; exit 3; fi
if docker ps --format '{{.Names}}' | grep -q '^gbtc-bench-'; then echo "REFUSE: another bench container running" >&2; exit 3; fi

TS="$(date -u +%Y%m%dT%H%M%SZ)"
NAME="gbtc-bench-${TAG}-${TS}"
echo "container:$NAME" > "$LOCK"
echo "[harness] start $NAME image=$IMAGE backend=$BACKEND kernel=$KERNEL local=$LOCAL cublock=$CUBLOCK batch=$BATCH secs=$SECS temp=$TEMP0"

cleanup() { rm -f "$LOCK"; }
trap cleanup EXIT

proc_cpu_ticks() {
  # Total user+sys ticks of a host PID (all threads share these counters).
  awk '{print $14+$15}' "/proc/$1/stat" 2>/dev/null || echo ""
}
CID="$(docker run -d --gpus all --network none --name "$NAME" \
  -e "GBTC_BACKEND=$BACKEND" -e GBTC_BENCH_ONLY=1 -e GBTC_PROBE_ONLY=0 \
  -e "GBTC_OPENCL_KERNEL=$KERNEL" -e "GBTC_OPENCL_LOCAL_SIZE=$LOCAL" \
  -e "GBTC_OPENCL_SIMD=$SIMD" -e "GBTC_OPENCL_POLL_US=$POLL" \
  -e "GBTC_CUDA_BLOCK=$CUBLOCK" \
  -e "GBTC_BATCH_NONCES=$BATCH" -e "GBTC_BENCH_SECONDS=$SECS" \
  -e "GBTC_BENCH_WARMUP_SECONDS=$WARMUP" \
  --entrypoint /usr/local/bin/miner "$IMAGE" 2>&1 | tail -n1)"
if [ -z "$CID" ]; then echo "docker run failed" >&2; exit 1; fi
HPID="$(docker inspect --format '{{.State.Pid}}' "$CID" 2>/dev/null || echo 0)"
CPU_SAMPLES="$OUTDIR/.cpu-${TAG}-${TS}.tmp"
: > "$CPU_SAMPLES"
( while kill -0 "$HPID" 2>/dev/null; do
    T="$(date +%s)"; V="$(proc_cpu_ticks "$HPID")"
    [ -n "$V" ] && echo "$T $V" >> "$CPU_SAMPLES"
    sleep 2
  done ) &
SAMPLER=$!
T0="$(date +%s)"
timeout "$(( SECS + WARMUP + 120 ))" docker wait "$CID" >/dev/null 2>&1
docker logs "$CID" > "$OUTDIR/trial-${TAG}-${TS}.log" 2>&1 || true
T1="$(date +%s)"
kill "$SAMPLER" 2>/dev/null || true
wait "$SAMPLER" 2>/dev/null || true
docker rm -f "$CID" >/dev/null 2>&1 || true
WALL="$(( T1 - T0 ))"; [ "$WALL" -le 0 ] && WALL=1
CPU_PCT="$(awk -v hz="$(getconf CLK_TCK)" -v w="$WALL" -v skip="$(( T0 + WARMUP + 2 ))" 'NR==1{f=$1;v0=$2} {l=$1;v1=$2; if($1>=skip && sf==""){sf=$1;sv0=$2}} END{if(NR>=2 && l>f) printf "%.3f",(v1-v0)/hz/(l-f)*100; else print "?" }' "$CPU_SAMPLES")"
CPU_STEADY="$(awk -v hz="$(getconf CLK_TCK)" -v skip="$(( T0 + WARMUP + 2 ))" '$1>=skip{c++; if(c==1){f=$1;v0=$2} l=$1;v1=$2} END{if(c>=2 && l>f) printf "%.3f",(v1-v0)/hz/(l-f)*100; else print "?" }' "$CPU_SAMPLES")"
rm -f "$CPU_SAMPLES"
AFTER="$(snap)"
TEMP1="$(echo "$AFTER" | cut -d, -f1)"
JSON="$(grep -o '{.*}' "$OUTDIR/trial-${TAG}-${TS}.log" | tail -n1)"
MH="$(echo "$JSON" | grep -o '"mh_s":[0-9.]*' | cut -d: -f2)"
HASHES="$(echo "$JSON" | grep -o '"hashes":[0-9]*' | cut -d: -f2)"
SECSS="$(echo "$JSON" | grep -o '"seconds":[0-9.]*' | cut -d: -f2)"
OK="$(echo "$JSON" | grep -o '"ok":[a-z]*' | cut -d: -f2)"
SMPOW="$(echo "$AFTER" | cut -d, -f2,3)"
echo "$TS,$TAG,$IMAGE,$BACKEND,$KERNEL,$LOCAL,$SIMD,$POLL,$CUBLOCK,$BATCH,$SECS,$WARMUP,${MH:-?},${HASHES:-?},${SECSS:-?},${OK:-?},${CPU_STEADY:-?},$CPU_PCT,${TEMP0:-?},${TEMP1:-?},${SMPOW:-?}," >> "$CSV"
echo "[harness] done mh_s=${MH:-?} ok=${OK:-?} cpu_steady=${CPU_STEADY:-?}% cpu_full=${CPU_PCT}% t=${TEMP0:-?}->${TEMP1:-?}C"
