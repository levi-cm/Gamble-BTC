# Part 5 — Bounded Performance Campaign

Run after correct Vulkan, Rust OpenCL and runtime integration. A baseline can be
collected earlier; the optimization campaign cannot substitute for correctness.
The orchestrator owns measurements; all GPU tests and benchmarks run one at a time.

## Measurement contract

- Compare immutable candidate and baseline image IDs, explicit backends, identical
  batch/input settings, clocks/power policy and workload. Keep baseline usable.
- Warm up >=30 s, use >=4 balanced pairs with >=60 s timed intervals, then confirm
  a winner for 10–15 minutes. Separate compiler cold start from warmed mining.
- Record successful UNIQUE nonce count / monotonic elapsed seconds / 1e6. Count
  each SHA-256d nonce test once, not each compression. Include runtime wait and
  readback costs in wall throughput. Report kernel-only timing separately.
- Record median, range and paired deltas. Use median absolute deviation (MAD) to
  estimate noise. A speed win requires a paired median gain greater than both
  3% and twice baseline relative MAD, with the same direction in at least 3/4
  pairs and sustained confirmation. Otherwise mark it inconclusive or reject.
- Record actual GT clocks, throttling, AC state and workload throughout. Keep
  loaded sustained results even when power-limited; do not label them full-clock.
- CPU core-equivalents = delta process user+system CPU seconds / wall seconds,
  summed over all miner threads and child processes. For a cgroup use its CPU
  usage delta. 0.02 cores = 2% of ONE logical CPU, not 2% of all eight CPUs.
  Record the CPU source and sampling window. Target <=0.02 warmed quiet cores;
  record idle, startup, probe, client count and loaded measurements separately.
- Report wakeups if readable, RSS, batch latency, stale work and valid submission
  counters. Mark unavailable counters unknown. No sysfs writes for benchmarking.

## Mandatory compilation coexistence matrix

Choose a real representative local build the user normally runs. Record project
commit, exact command, compiler, parallelism, cache state and elapsed time. Use a
task-owned build directory; do not clean or modify another active worktree.
Keep the SAME build command and parallelism in every comparison.

| Run | Purpose |
| --- | --- |
| Build alone | Compilation elapsed baseline |
| Mining alone | Quiet sustained hashrate and CPU |
| C OpenCL baseline + build | Current coexistence baseline |
| Rust Vulkan + build | New Vulkan coexistence |
| Rust OpenCL + build | Runtime rewrite cost without changing GPU API |

Use >=3 matched repetitions of build-loaded runs, with balanced order/cache
conditions. Report build slowdown = (combined/build-alone - 1)*100 and mining
loss = (1-loaded/quiet)*100. Do not lower build priority, cap cores/clocks or
parallelism, reserve cores, or change host policy to claim success. Worker process
niceness is not authorization to change the compilation workload.

For G5, use a 5% non-regression margin against the matching C OpenCL baseline for
sustained throughput, loaded mining and build duration; if baseline variability
exceeds that margin, extend measurement or mark inconclusive. A real trade-off
is documented and leaves automatic production cutover pending. Never claim zero
interference merely because CPU usage is small.

## Ranked experiments (one mechanism per candidate)

| ID | Mechanism and recon | Implement only if | Reject/inconclusive condition |
| --- | --- | --- | --- |
| EX-010 | Existing `pre3` prefix in `src/opencl.c`; preserve original midstate feed-forward | Rust/Vulkan port lacks equivalent savings | Already implemented, no emitted reduction, or no repeatable win |
| EX-011 | Second-SHA word prefilter pruning | ANV retains demonstrably unused arithmetic | Compiler already removes it or full-set conformance fails |
| EX-012 | Scalar schedule/reassociation/constant padding | Machine-code or profiling evidence identifies a cost | Increased spills, correctness loss, or no gain |
| EX-013 | Local size and optional subgroup size | Supported sizes/features and correct baseline exist | Unsupported capability or register/throughput regression |
| EX-014 | Dual nonce per invocation | New shared-prefix conditions differ from rejected dual path | Spills or latency erase gain |
| EX-015 | Two-slot pipeline | Measured host/device idle gaps justify complexity | No repeatable benefit or job/ownership races |
| EX-016 | Isolated Mesa or NEO/IGC comparison | Current compiler limitation has evidence | Wrong driver mapping, no code change, regression |
| EX-017 | Poll interval/batch latency balance | CPU or stale-work/dispatch overhead is measured | CPU >target or stale work/loaded behavior worsens |

First port a correct unrolled baseline. Do not implement every kernel variant
before a working runtime. Optional experiments may be explicitly rejected without
code when the mechanism is already disproven; record why and move on.

Pruning notes: `h_64 = e_61` concerns the evolving state before feed-forward.
The final digest word includes the initial SHA state addition and byte-order
conversion. The full host target check still needs a complete digest. Never
remove a needed comparison or infer validity from one word.

For pipeline experiments, every slot owns immutable job/target generations,
input/output buffers, reset commands and completion objects. Bound depth to two,
serialize queue ownership, and test job invalidation while both slots are pending.

## Binary evidence

Hash shader source, compiler version/options and emitted SPIR-V. Obtain ANV native
ISA only through applicable Mesa tools; IGC OpenCL ISA is not Vulkan ISA.
SPIR-V changes do not prove native code changes. Instruction count is one signal,
not throughput. Identical GPU code rejects a claimed shader-codegen improvement,
but host scheduling/wait changes can improve wall performance with the same code.

Respect historical negative results in `docs/HARDWARE_MATRIX.md`: SIMD32,
looped/dual, larger batches, IGC-pruned tail, scheduling knobs. Revisit only with
a stated changed condition. Never add speculative gains together.

Done: every attempted experiment has correctness, measurements, decision and
reason in the ledger. Report achieved throughput and gap to 400 without claiming
an absolute upper bound or a guaranteed future improvement.
