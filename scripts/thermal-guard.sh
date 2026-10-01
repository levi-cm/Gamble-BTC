#!/usr/bin/env bash
# Independent thermal guard for the GTX 1060 48h campaign.
# Polls nvidia-smi every ~2s, logs CSV, enforces soft/hard trip policy by
# stopping ONLY the workload registered in gpu-workload.lock (owned experiments).
# Never touches unrelated containers. Usage: thermal-guard.sh [state_dir]
set -u
DIR="${1:-bench-results/campaign-20261001}"
LOG="$DIR/thermal-guard.log"
CSV="$DIR/thermal-telemetry.csv"
TRIPS="$DIR/thermal-trips.log"
LOCK="$DIR/gpu-workload.lock"
STOP_FLAG="$DIR/thermal-stop.flag"
COOLDOWN_OK="$DIR/cooldown-ok.flag"
mkdir -p "$DIR"
if [ ! -f "$CSV" ]; then
  echo "ts_utc,temp_c,power_w,power_limit_w,sm_mhz,util_pct,fan_pct,thr_active,sw_therm,slowdown,workload" > "$CSV"
fi
: >> "$LOG"
echo "$(date -u +%FT%TZ) guard start pid=$$" >> "$LOG"
soft_hits=0
cool_ok_since=0
while true; do
  TS="$(date -u +%FT%TZ)"
  Q="$(nvidia-smi --query-gpu=temperature.gpu,power.draw,power.limit,clocks.current.sm,utilization.gpu,fan.speed,clocks_throttle_reasons.active,clocks_throttle_reasons.sw_thermal_slowdown,clocks_throttle_reasons.hw_thermal_slowdown --format=csv,noheader,nounits 2>/dev/null | head -n1)"
  WL="none"
  [ -f "$LOCK" ] && WL="$(head -n1 "$LOCK" 2>/dev/null | cut -c1-80)"
  if [ -z "$Q" ]; then
    echo "$TS MISS empty-nvidia-smi wl=$WL" >> "$LOG"
    echo "$TS,,,,,,,,,$WL" >> "$CSV"
    sleep 2; continue
  fi
  # "72, 105.66, 120.00, 1835, 97, 48, 0x000..., 0x000..., 0x000..."
  TEMP="$(echo "$Q" | cut -d, -f1 | tr -d ' ')"
  THR="$(echo "$Q" | cut -d, -f7 | tr -d ' ')"
  SWTH="$(echo "$Q" | cut -d, -f8 | tr -d ' ')"
  HWSL="$(echo "$Q" | cut -d, -f9 | tr -d ' ')"
  echo "$TS,$Q,$WL" | sed 's/ //g' >> "$CSV"
  ACTIVE_THERM=0
  STRIPPED="$(echo "$SWTH $HWSL" | sed 's/NotActive//g')"
  case "$STRIPPED" in *Active*) ACTIVE_THERM=1;; esac
  # cooldown tracking
  if [ "${TEMP%.*}" -le 70 ] 2>/dev/null; then
    if [ "$cool_ok_since" -eq 0 ]; then cool_ok_since="$(date +%s)"; fi
    if [ $(( $(date +%s) - cool_ok_since )) -ge 60 ]; then : > "$COOLDOWN_OK"; fi
  else
    cool_ok_since=0; rm -f "$COOLDOWN_OK"
  fi
  # trip logic
  TRIP=""
  if [ "$ACTIVE_THERM" -eq 1 ]; then TRIP="HARD thermal-slowdown-active thr=$THR";
  elif [ "${TEMP%.*}" -ge 78 ] 2>/dev/null; then TRIP="HARD temp>=78C";
  elif [ "${TEMP%.*}" -gt 75 ] 2>/dev/null; then
    soft_hits=$((soft_hits+1))
    if [ "$soft_hits" -ge 3 ]; then TRIP="SOFT temp>75Cx3"; fi
  else soft_hits=0; fi
  if [ -n "$TRIP" ]; then
    echo "$TS TRIP $TRIP temp=${TEMP}C wl=$WL" >> "$LOG"
    echo "$TS $TRIP temp=${TEMP}C wl=$WL" >> "$TRIPS"
    : > "$STOP_FLAG"
    # act only on owned workload named in lock
    OWNER="$(head -n1 "$LOCK" 2>/dev/null)"
    if [ -n "$OWNER" ]; then
      case "$OWNER" in
        container:*) C="${OWNER#container:}"; echo "$TS stopping owned container $C" >> "$LOG"; timeout 60 docker stop -t 20 "$C" >> "$LOG" 2>&1 || true;;
        pid:*) P="${OWNER#pid:}"; echo "$TS killing owned pid $P" >> "$LOG"; kill -TERM "$P" 2>/dev/null || true;;
        service:*) S="${OWNER#service:}"; echo "$TS stopping owned compose service $S" >> "$LOG"; timeout 90 docker compose stop -t 20 "$S" >> "$LOG" 2>&1 || true;;
        *) echo "$TS unknown lock owner, no action" >> "$LOG";;
      esac
    else echo "$TS no workload lock, no action (production miner untouched)" >> "$LOG"; fi
    soft_hits=0
    sleep 10; continue
  fi
  sleep 2
done
