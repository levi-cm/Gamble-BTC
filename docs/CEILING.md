# Baseline and Ceiling

## Baseline identity

- Repository: `Gamble-BTC-opencode-optimisation`
- Branch: `igpu-vulkan`
- Commit: `a12938f230a3254cdae6d0accb829ccedfcf4a5c`
- Baseline image identity: `gamble-btc:latest` / `gamble-btc:ocl-cand-pre3` =
  `24805ee25283` (1.32GB) (from `docs/RUST-COMPATIBILITY.md`).
- Hardware: Intel Iris Xe Graphics (TGL GT2), vendorID 0x8086, deviceID 0x9a49,
  Mesa 26.2.4-arch3.1 (from `docs/RUST-COMPATIBILITY.md`).
- Backend/kernel/tuning: OpenCL path is the current fast path; GLES remains for
  historical comparison. Exact tuning values are carried by environment variables
  and Compose settings recorded in `docs/RUST-COMPATIBILITY.md` sections 6-7.
- Power policy: AC power required unless `GBTC_ALLOW_BATTERY=1`.

## Protocol

- Warm up for at least 30 seconds.
- Measure at least four balanced paired observations of at least 60 seconds per
  config.
- Use interleaved/balanced ordering (ABBA/BAAB or equivalent), not always warming
  the same backend first.
- Record clocks, throttling/power state, miner CPU, workload, build activity,
  exact image ID, and device/driver.
- Keep quiet-host and compilation-loaded measurements in separate groups.
- Missing telemetry is recorded as `unknown`, never zero.
- No pool credentials are needed in benchmark mode.

## Measurement state (2026-10-18)

The user explicitly deferred hashrate work while another agent uses the iGPU
for the more important jcalc project. That workload was left running. Five
shader screens on immutable candidate image `9b74a12b3628` are shared-load
exploration only, not quiet measurements: looped 43.16, unrolled 111.58,
pre3 114.35, pruned 122.45 and boolean 115.00 MH/s. These were 30 second measured
windows after 30 second warmup. They do not establish a speed ranking, a C
comparison, a sustained winner or 350 MH/s. The image predates final integration
fixes; current image identity and correctness evidence are in
[QUALIFICATION.md](QUALIFICATION.md).

Native host ANV output established that unrolling removes register spills;
manual tail pruning matches unrolled instruction statistics. Remaining local
size/subgroup experiments, balanced paired C/Rust comparisons, long confirmation
and real compilation coexistence trials are deferred. `scripts/bench-rust.py`
provides immutable image identity, wall hashing, cgroup CPU and clock/PL1/AC
telemetry for that later work. Missing telemetry is null.

## Limits

- Historical throughput estimates are diagnostic context only, not pass/fail
  thresholds.
- Throttling is a real property of this laptop; loaded throttled runs describe
  coexistence, not full-clock kernel throughput.
- Unmatched conditions are inconclusive.