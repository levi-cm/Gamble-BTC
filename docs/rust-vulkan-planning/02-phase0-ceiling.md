# Part 2 — Baseline First; Throughput Models Are Optional

This phase establishes evidence and fixes benchmark attribution. It does not
require proving a hardware ceiling before the Vulkan implementation can begin.

## T02: audit the harness before trusting numbers

Read `scripts/bench-run.sh`, `scripts/bench-ab.sh`, `scripts/bench-current.sh`,
`scripts/start-miner.sh`, `src/bench.c`, and benchmark code in `src/main.c`.
At review time, `bench-ab.sh` forces `GBTC_BACKEND=opencl` and only forwards a
subset of settings. `bench-run.sh` defaults to GLES. Round-robin order is not
ABBA, and its timeout loop does not prove a hung container was terminated.
Do not use these scripts unchanged to claim Vulkan measurements.

- [ ] Record baseline commit, image ID, backend, kernel variant and exact public
  tuning values. Preserve baseline binary/image before modifying runtime wiring.
- [ ] Add script tests with a mocked docker command covering backend/image/env
  forwarding, failing container exit, timeout cleanup and output validation.
- [ ] Make every result identify the actual backend/image/config. Reject missing,
  duplicate or malformed benchmark summary records and nonzero exit codes.
- [ ] Forward the selected Vulkan knobs once introduced. Retain existing OpenCL
  knobs. Pass an explicit backend/image for every run.
- [ ] Implement ABBA/BAAB ordering or record and analyze a randomized balanced
  order; avoid always warming the same backend first.
- [ ] Match telemetry to the selected DRM device, not hard-coded card1. Read
  device sysfs/proc counters only; missing telemetry is `unknown`, never zero.
- [ ] Use unique files per run, finite timeouts and cleanup of the exact test
  container/process. Never kill by a broad miner process-name pattern.

Check: `bash -n` on modified scripts; mocked script tests; inspect a synthetic
output fixture. Real GPU runs follow the exclusive test-window rule in Part 99.

## Baseline protocol

Under the existing power policy on AC, use the current OpenCL release config and
optionally a GLES historical comparison. Warm up for at least 30 seconds, then
measure at least four balanced paired observations of >=60 seconds per config.
Record clocks, throttling/power state, miner CPU, workload, build activity, exact
image ID and device/driver. No pool credentials are needed in benchmark mode.
Keep quiet-host and compilation-loaded measurements in separate groups.

Throttling is a real property of this laptop, not automatically invalid data.
Label runs `quiet sustained`, `build loaded`, or `controlled full-clock` only when
supported by telemetry. A loaded throttled run cannot prove full-clock kernel
throughput; it can establish real coexistence behavior. Do not discard every
slow run to select favorable samples. Mark unmatched conditions inconclusive.

Create `docs/CEILING.md` with baseline methods/results, variance and explicit
limits. If GPU access is blocked, write `measurement pending`, state why, and
continue host-only work. G1 can record a blocked measurement without passing GPU
qualification gates.

## Optional diagnostic experiment EX-001

After a minimal Vulkan dispatch exists, compare a synthetic unsigned integer
add/xor/rotate/and workload through Vulkan and OpenCL. Prevent dead-code removal
with checked outputs, use several independent chain counts, and report actual
work, device time and wall time separately. Validate timestamp support and wrap
handling if GPU timestamp queries are used.

The historical `80 * 8 * 1.30 GHz / 1400 ops` estimate mixes assumptions about
instruction width, issue rate, SHA operations and dependencies. Do not turn it
into a pass/fail threshold. Compiler ISA instructions, SIMD lanes and abstract
scalar operations have different units. SPIR-V counts are not machine ISA counts.

Report measured workload throughput and assumptions. It is neither proof that
400 MH/s is possible nor proof that 400 MH/s is impossible. A defensible upper
bound would need a separately justified machine-resource model and a valid lower
bound on SHA work. Do not make such a proof a prerequisite for this rewrite.

Done: baseline/harness identity is trustworthy, measurement state is recorded,
and any diagnostic model is clearly separated from measured mining throughput.
