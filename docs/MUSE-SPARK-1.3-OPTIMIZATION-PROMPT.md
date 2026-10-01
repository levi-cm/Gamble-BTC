# Muse Spark 1.3: hardware-specific Iris Xe miner optimization prompt

Copy everything below the separator into Muse Spark 1.3 with access to this
repository and, when available, the actual laptop. This prompt describes an
optimization task; its targets are objectives, not measured guarantees.

---

You are an expert C, OpenCL, SHA-256, Intel GPU compiler, and Linux performance
engineer. Optimize this existing experimental Bitcoin solo miner specifically
for my Huawei laptop. Work carefully in small, measured steps. Your job is to
produce correct, sustained GPU hashing with the smallest practical CPU cost.
Read the repository before editing; several obvious optimizations already exist.

## 1. My priorities and non-negotiable requirements

I use my CPU heavily for compilation. I am comfortable with the iGPU running
in the background at high utilization. Handcraft the miner for my exact
hardware rather than producing generic GPU advice.

The central requirement is: **use as little CPU as possible, and keep mining
throughput unaffected by compilation running in the background.** Treat this
as an acceptance target and explicitly measure whether it is achieved.

- Aim toward **215 million complete Bitcoin header SHA-256d nonce tests per
  second**, sustained, with correct candidate detection and submission.
- Minimize miner CPU time, driver CPU time, wakeups, and scheduling overhead.
  Prefer a measured reduction from the already low CPU baseline.
- Maintain hashrate while realistic CPU compilation runs concurrently.
- Preserve compilation performance. Do not achieve the target by reducing
  compiler parallelism, lowering compiler priority, capping CPU clocks,
  disabling CPU boost, or reserving CPU cores away from my builds.
- Distinguish miner CPU contention from shared package power, heat, and memory
  contention. The CPU and iGPU are on one package; GPU utilization is not free
  just because the miner's CPU percentage is low.
- If hardware power limits prevent completely unaffected mining under full
  compilation load, show the measurements and explain the remaining limit.
  Never promise complete independence or fabricate a 215 MH/s result.
- Keep all nonce search on the iGPU. Host hashing is permitted only for job
  construction, correctness tests, and checking sparse GPU candidates.
- Preserve Stratum correctness, shutdown behavior, the status UI, and the
  established low-CPU completion path.

Use <=2% of one logical CPU as an initial steady-state CPU reference target,
with <1% as a stretch objective, not a guaranteed threshold. Define the units:
100% in Docker's usual CPU reporting represents one fully busy logical CPU;
2% is 0.02 CPU cores, not 2% of all eight logical CPUs. Record CPU-seconds per
wall-second so different tools cannot obscure the result. Include driver/helper
CPU activity outside the miner process when attributable and measurable.

For compilation coexistence, initially target a hashrate loss within measurement
noise, using 3% as a provisional practical tolerance, and a build-time regression
no larger than 3%. Report exact values even if these objectives fail. Do not
silently redefine success around a less demanding workload.

## 2. Hardware and runtime context

The following was inspected on 2026-10-01; recheck drift-prone details:

- Huawei KLVDZ-WXX9 / M1010 laptop, 16 GiB RAM.
- Intel Core i5-1135G7: Tiger Lake, four physical cores, eight logical CPUs.
- Intel Iris Xe TigerLake-LP GT2 / Xe-LP, PCI ID `8086:9a49`, using `i915`.
- Intel's processor specification lists 80 execution units and a 1.30 GHz
  maximum dynamic graphics frequency. Query the actual device rather than
  assuming every Iris Xe has 96 EUs or newer XeHP instructions.
- CachyOS / Arch, x86_64, kernel `7.2.6-1-cachyos`, Hyprland / Wayland.
- Live DRM devices: `/dev/dri/card1` and `/dev/dri/renderD128`.
- Compose currently defaults to video GID 983 and render GID 987; verify
  actual numeric permissions rather than copying old HD 4600 assumptions.
- Host Mesa: `26.2.3`; Docker: `29.8.1`.
- Running container packages: Intel NEO `26.35.39758.10`, IGC `2.41.5`,
  Mesa `26.1.6-1~bpo13+1`. Host Mesa is not the container's OpenCL compiler.
- Active backend: Intel NEO OpenCL. Mesa Rusticl and GLES are fallback and
  comparison paths; Vulkan is not a working mining backend.
- Default OpenCL configuration: unrolled, local size 64, SIMD auto,
  16,777,216 nonces per batch, completion polling at 1,000 microseconds.
- Existing container resource settings include four CPU equivalents and 4 GiB
  memory. A `cpus: 4.0` limit does not mean four dedicated or occupied cores.
- The live container was using host networking. Compose uses port publishing;
  inspect the actual launch path rather than assuming Compose controls it.
- Existing AC GPU frequency policy requested a 1300 MHz minimum/maximum.
  Do not assume this defeats firmware or package power throttling.

A read-only snapshot showed approximately 1.45% container CPU and 88.83 MiB
memory. A nearby status sample reported 133.617 MH/s instantaneous, 157.813
rolling average, and 196.182 maximum. Actual GPU frequency was 950 MHz with
`throttle_reason_pl1=1`, `throttle_reason_thermal=0`, and AC connected.
The long-term package constraint read 35,000,000 microwatts. These are
snapshots, not synchronized benchmarks and not proof of a particular compiler
workload causing the slowdown. Reproduce and measure the cause.

## 3. Read the current implementation and preserve existing work

Repository:
`/home/opsec1/Documents/levi-private/gamble-btc-docker-laptop/Gamble-BTC-opencode-optimisation`

Read `AGENTS.md`, then these files in focused groups:

1. `src/opencl.c`, `src/opencl.h`, `src/backend.h`, `src/backend.c`.
2. Mining/work construction and benchmark loops in `src/main.c`;
   `src/bench.c`, `src/sha256.c`, and `src/stratum_protocol.c`.
3. `tests/test_opencl_conformance.c` and the relevant deterministic tests.
4. `Dockerfile`, `Makefile`, `compose.yaml`, `.env.example`.
5. `scripts/bench-run.sh`, `scripts/bench-ab.sh`, `scripts/igc-dump.sh`,
   `scripts/start-miner.sh`, and `docs/HARDWARE_MATRIX.md`.

At inspection, HEAD was `a9ac474` and existing uncommitted changes were present
in `src/main.c`, `src/stratum_protocol.c`, `src/stratum_protocol.h`, and
`tests/test_stratum_protocol.c`. Recheck status and inspect the existing diff.
Preserve those changes. Never reset or overwrite unrelated work.

Some documentation still describes OpenCL as a placeholder or lists obsolete
device/group defaults. Treat current source and live probes as authoritative.
Do not spend the task implementing an OpenCL backend that already exists.

Important implementation facts to verify:

- SHA midstate reuse already eliminates the first 64-byte compression of the
  80-byte header from each nonce test. Two compression blocks remain per nonce.
- `emit_unrolled_block` emits all 64 rounds using rotating variable names
  and a 16-word rolling message schedule.
- `emit_nonce_unrolled` runs the header's final block and then the second SHA.
  The final prefilter only compares `BSWAP32(0x5be0cd19 + h)` to target word 0.
- The host performs a full 256-bit target comparison on returned candidates.
- The output stores at most 15 nonces. Returned `hashes_done` is assigned from
  requested work size; that assignment alone cannot prove coverage.
- The preferred OpenCL path writes input and resets the result counter,
  dispatches with event dependencies, flushes, sleep-polls the kernel event,
  and then performs a blocking result read. The blocking fallback uses
  `clFinish`, historically costing approximately one busy CPU core.
- The polled path is synchronous at the backend interface. It is not already
  a two-slot asynchronous pipeline.

## 4. Existing evidence: do not repeat unsuccessful ideas blindly

`docs/HARDWARE_MATRIX.md` records these historical results; reproduce only
when needed and label them as repository evidence until verified:

- Intel NEO improved substantially over Mesa Rusticl/GLES on this laptop.
  A recorded low-CPU run achieved 184.022 MH/s at 1.78% container CPU.
- Later full-clock samples ranged from 186.634 to 193.994 MH/s. They are not
  a repeatable interleaved A/B median and did not simultaneously establish
  the same CPU usage. The gap to 215 is roughly 11–15% from that range.
- Default compiler metadata showed SIMD16, 128 GRFs, seven threads per EU.
  Thread occupancy was already at the recorded hardware maximum. This does
  not establish maximum instruction throughput or eliminate scheduling stalls.
- SIMD32 roughly doubled instruction count and introduced spills without a
  demonstrated gain. Existing looped and dual-nonce variants were slower.
- Larger batches previously failed to improve compute-saturated throughput.
- Basic CH/MAJ rewrites did not create ternary Boolean instructions.
- Two tested IGC scheduling flags produced identical binaries. Do not claim
  gains from flags without checking emitted code and measured performance.
- Several A/B samples were package-power throttled. Never use them as evidence
  for a full-clock compiler improvement.

Intel IGC 2.41.5 documentation explicitly marks `EnableBfn` and `EnableAdd3`
as XeHP+ only. This machine is Xe-LP. Do not describe missing BFN/LOP3/add3
fusion as necessarily a compiler bug, and do not invent unavailable opcodes.
Verify all architecture-specific instructions against primary documentation.

## 5. Measurement and correctness gates before optimization

Use a dedicated candidate image tag. Do not overwrite the shared
`gamble-btc:latest` image or replace the current running container during
exploration. Arrange an isolated GPU benchmark window with the user when
needed: do not silently stop their miner, and do not run two mining processes
on the same GPU and call the result an uncontended baseline.

Establish three distinct workloads:

1. Quiet-host GPU benchmark: estimates the kernel's sustained potential.
2. Mining plus a realistic representative CPU build, retaining normal build
   concurrency: tests the requirement that mining is unaffected by compilation.
3. The same build with mining off in an agreed test window: measures compilation
   slowdown attributable to mining. Match build inputs and cache state.

Use repeated interleaved A/B or ABBA runs, matched initial conditions and
warmup, then a 10–15 minute sustained confirmation for convincing winners.
Sixty-second samples are screening evidence. Sample actual frequency and
throttle flags repeatedly throughout each run; a single midpoint sample in
the existing script is insufficient to certify an entire run as unthrottled.
Report idle and loaded results separately; never discard throttled samples
from the compilation workload because throttling is part of that requirement.

Record wall-clock MH/s from completed work, kernel event duration when
profiling is enabled, CPU time/core equivalents, GPU frequency distribution,
PL1 and thermal flags, package power/temperature where readable, context
switches/wakeups, build wall time, backend, image ID, driver/compiler versions,
kernel configuration, and correctness outcomes. Report median and variability.
Normalize MH/s by actual frequency as a diagnostic, not a substitute for
delivered hashrate. Startup compilation should be recorded separately.

Check the measurement path itself. `scripts/bench-run.sh` currently defaults
to GLES unless overridden. It does not explicitly forward
`GBTC_OPENCL_POLL_US`; adding a host variable without forwarding it into
Docker will not test polling changes. `bench-ab.sh` only handles selected
environment variables and tolerates a failed subprocess with `wait ... || true`.
Ensure candidate image selection, all experiment knobs, and failures are
unambiguous before trusting its output.

Strengthen correctness first where needed. Existing GPU conformance primarily
tests all-match and no-match cases. It verifies returned candidates but does
not prove no matching nonce was missed, and `hashes_done` is self-reported.
For small deterministic ranges, compare the complete expected candidate set
against the reference, using targets chosen to avoid output overflow. Test
known headers, randomized midstates/tails, nonzero nonce bases, endianness,
target equality, nonce-range boundaries, and each new variant. For overflow,
test separately and define detection/recovery rather than silently treating
a truncated list as complete. Never use an optimized GPU path as its own oracle.

## 6. The top ten optimization ideas to evaluate

These are ranked experiments, not ten promises. Prioritize measured benefit
per complexity. Investigate one at a time, preserve the baseline variant, and
explain the relevant code, mechanism, evidence, test, and rejection condition.

### 1. Precompute the nonce-independent SHA prefix and schedule terms

Start in `emit_nonce_unrolled` and work construction. The final header block
has W0–W2 from `tail3`, W3 from the nonce, and fixed padding. Its first three
rounds do not depend on the nonce. Derive a job-level state after those rounds
and start the per-nonce path at the first nonce-dependent round.

Maintain the ORIGINAL midstate for the first compression's feed-forward;
the state after three rounds is not a replacement feed-forward value.
Find additional nonce-independent schedule words/subexpressions by explicit
dependency analysis. Precompute those once per immutable header, not once
per nonce and preferably not redundantly once per batch.

Account for new input loads, uniformity, payload size, and register pressure.
Test precomputation with multiple jobs, extranonce and ntime changes. Do not
JIT-compile a new kernel for every job. This is a plausible arithmetic reduction,
but three removed rounds alone cannot establish the entire 215 MH/s gain.

### 2. Prune the final SHA computation to the target word actually needed

The current GPU filter only consumes the final second-SHA `h` word. Perform a
backward dependency analysis to determine the latest round/state needed to
compute it exactly. In the standard round recurrence, h is a delayed version
of e; final h after 64 rounds relates to e after 61 rounds. Derive the precise
zero-based round indices and rotating variable names before modifying code.

Generate only the operations required by that output. Preserve the initial
second-SHA h feed-forward constant and the byte swap. Check whether IGC
already removes the unused tail operations; only claim novelty if the binary
changes favorably. Combine this with idea 1 only after independent tests.

Preserve the word-0 prefilter as a necessary condition and the host's complete
256-bit comparison. Never submit merely because a partial hash passes.
For rarer candidates, host rehashing is acceptable; do not turn this into CPU
nonce search. Prove equivalence against full SHA-256d over small full ranges.

### 3. Optimize the generated schedule and arithmetic dependency chains

Inspect the generated kernel and ISA rather than making broad source edits.
Explore explicit scalar schedule variables, constant folding for padding,
earlier computation of independent schedule terms, and unsigned 32-bit
reassociation of additions. Compare full unrolling with a few deliberately
chosen partial-unroll arrangements, not the already slower generic looped
variant under another name.

Use live-range and spill evidence to judge whether moving an expression
earlier helps scheduling or merely increases register pressure. Preserve all
modulo-2^32 semantics. Do not reassociate through nonlinear sigma operations.
Reject variants that increase stalls or spills without a repeatable gain.

### 4. Match rotate, Boolean, and constant operations to real Xe-LP ISA

Compare the existing rotate idiom with OpenCL's `rotate` builtin, converting
right rotations to the correct left-rotation count. Examine whether shifts,
rotates, byte swaps, and constants already compile efficiently. The prior
ISA census already included native `rol`, so finding a builtin is not itself
evidence of improvement.

Test small CH/MAJ expression alternatives only when instruction scheduling,
dependency, or register effects provide a fresh hypothesis. Do not expect
BFN/LOP3 or add3 from XeHP+ on this Xe-LP GPU. Record generated binary hashes,
opcode differences, and throughput. If the compiler emits identical code,
stop that experiment rather than crediting benchmark noise to the edit.

### 5. Retune SIMD and local size after real kernel changes

The baseline is already SIMD16 with high recorded occupancy. After arithmetic
or live-range changes, screen SIMD8 and SIMD16 with compatible local sizes
such as 32, 64, and 128. Keep SIMD auto as a control. Revisit SIMD32 only if
the new code materially changes register pressure and avoids the prior spills.

Use explicit required work-group sizes where valid. Query device/kernel limits
and subgroup extensions. Compare instruction issue, occupancy and spills;
maximum threads per EU does not automatically mean maximum hash throughput.
Avoid exhaustive matrices before there is a reason to expect a new optimum.

### 6. Replace frequent completion polling with measured adaptive sleeping

The existing sleep-poll path already solved a major CPU problem. Preserve it.
Measure its event-query rate, wakeups, submission duration, completion latency,
and the result-read cost. At 190 MH/s a 16M batch is about 88 ms; polling every
1 ms creates many checks relative to the number of actual batches.

Experiment with an estimated batch-duration sleep followed by bounded
sleep-polling, using conservative estimates that adapt when compilation
changes GPU clocks. Compare against fixed intervals within the existing
100–50,000 microsecond range. Avoid active spinning and long oversleeps that
leave the GPU idle. Handle interruptions, errors, and shutdown promptly.

The current final blocking read can still entail a separate command wait
after kernel completion. Profile it; if significant, enqueue a nonblocking
read and wait by the same low-CPU mechanism. Do not assume `clWaitForEvents`,
callbacks, or a newer driver necessarily sleep without measuring CPU use.
Keep asynchronous transfer source and destination storage alive until completion.

### 7. Use a shallow two-slot pipeline only if GPU idle gaps justify it

Measure the interval between one kernel ending and the next starting. If
dispatch/readback/poll oversleep creates meaningful gaps, prototype two
independent input/output slots and a queue depth of two to overlap host
handling with the next GPU batch. Do not expect overlapping compute kernels
to multiply the throughput of an already saturated device.

Give every slot its own job ID, extranonce, ntime, target generation, nonce
range, output storage, and event ownership. Never overwrite buffers still in
use. Account for completed work exactly once. On clean jobs and difficulty
changes, stop enqueueing obsolete work and handle pending results according
to Stratum semantics. Bound queue depth to limit stale work and shutdown delay.

The existing `run_batch` API is synchronous; explain the smallest justified
interface change. Reject the pipeline if its gain is negligible relative to
the correctness and lifecycle complexity.

### 8. Amortize immutable work uploads and eliminate redundant host work

Within a nonce sweep, the midstate, tail, and target remain mostly unchanged,
but each batch currently uploads the input and sets the same buffer arguments.
Separate immutable job data from nonce-base changes if profiling shows value.
Compare kernel scalar arguments or a compact nonce-base update against the
current small input buffer. Set invariant arguments once where legal.

Cache job-level work using an explicit identity that includes all relevant
header fields and target generation. Keep result-counter resets ordered.
Inspect Stratum, metrics, HTTP serialization, logging, and launcher wakeups
for measured CPU hotspots. Preserve useful status updates and protocol
responsiveness; do not remove correctness checks to save tiny CPU costs.

The input is only tens of bytes. Do not assume its bandwidth is the bottleneck,
and do not introduce elaborate pinned/SVM buffers without evidence. More batch
size is not automatically better and was previously unhelpful.

### 9. Run a reproducible, architecture-valid compiler experiment

Use `scripts/igc-dump.sh` to capture the baseline and candidate emitted code.
Consider documented relevant compiler settings or a compatible IGC/NEO pair
in a separately tagged image. Verify Tiger Lake support before changing driver
versions; never assume the newest package improves this exact GPU.

Record source, options, compiler versions, compiled binary hash, instruction
count, SIMD width, GRFs, spill/scratch use, and actual A/B results. Reject flags
unsupported by the release build, intended for other architectures, or producing
unchanged code. Keep dependencies reproducibly pinned and verified.

Assess a compiler binary cache only for startup/restart CPU savings; it does
not directly increase sustained hash throughput. Key it by source/options,
device and compiler identity. Handwritten assembly is a final research path:
first establish supported instructions, the kernel ABI, and a workable loading
route. Do not claim that assembly unlocks instructions absent from the GPU.

### 10. Design a measured compilation-coexistence policy for this laptop

The user requires minimal CPU usage and mining unaffected by background builds.
Measure both objectives under realistic full compilation load. Determine
whether throughput drops track GT frequency, PL1, temperature, driver CPU
activity, scheduler delay, or shared-memory contention.

For CPU scheduling interference, consider miner-only lower CPU scheduling
weight or priority, and container CPU controls when profiling justifies them.
Prefer weights over a very tight quota that can starve brief submission bursts.
Do not lower compiler priority or cap CPU frequency. CPU niceness does not
prove GPU scheduling isolation.

Compare the existing maximum-frequency request against less rigid GPU policies
only in explicitly approved host experiments, measuring total package power,
MH/J, sustained hashes and build time. GPU clock floors do not override PL1.
More efficient hashing may reduce power per hash and help coexistence, but
CPU and GPU power independence cannot be created by a container setting.

If full compilation still reduces hashrate, report the actual Pareto tradeoff:
best idle throughput, best concurrent throughput, miner CPU use, and build
regression. Explain what further change would be required. Do not raise package
power limits, disable thermal protections, overclock, undervolt, switch kernel
drivers, or modify TLP/system policy without specific user authorization.

## 7. Execution discipline for a lower-tier model

Work from concrete evidence. Do not jump from reading one function to replacing
the entire backend. Keep a short experiment ledger so you retain decisions.

For each idea:

1. State the exact bottleneck hypothesis and relevant functions.
2. State what already exists and how this experiment differs from prior attempts.
3. Identify assumptions and verify device/API/compiler support.
4. Specify the smallest reversible change and meaningful correctness tests.
5. Build and inspect warnings; verify GPU conformance on the actual candidate.
6. Run matched A/B measurements and a concurrent-compilation check for winners.
7. Mark the result KEEP, REJECT, or INCONCLUSIVE with the supporting numbers.

Start with baseline measurement and correctness gaps, then the strongest kernel
arithmetic ideas and CPU waiting improvements. Do not implement all ten at once.
Do not sum speculative percentage gains: optimizations overlap and power limits
can erase apparent isolated gains. Stop repeating a path when its mechanism
has been disproven unless the kernel/compiler conditions have materially changed.

Use `make test` for deterministic checks, recording a GPU skip as a skip rather
than a pass on actual hardware. Use `docker compose config --quiet` for container
wiring changes, without printing resolved secrets. Build candidate containers
and inspect compiler warnings. Do not assume the current Dockerfile includes
test sources: add an isolated test stage/workflow if needed. Use `git diff --check`.
Sanitizers are appropriate for changes to asynchronous ownership or host memory.

Do not read or publish my `.env` values, wallet address, full status response,
or private runtime logs. Select non-sensitive metrics when collecting evidence.
Keep build outputs and local benchmark traces out of Git. Do not run global
cleanup, delete my caches, commit, push, or deploy without authorization.

Before any expensive/repeated GPU runs or changes to a running service, establish
the agreed test window. Continue independent source analysis while waiting.
If hardware access is unavailable, provide a reviewable patch and exact commands,
and label performance and GPU correctness as unverified.

## 8. Required final output

Produce:

- A concise diagnosis of the actual limiting factors on this laptop.
- A ten-row experiment table with rank, mechanism, relevant code, expected
  direction of GPU/CPU effect, risk, test status, measured result, and decision.
  Expected direction is a hypothesis; avoid invented exact gains.
- Small reviewable changes for supported improvements and relevant tests.
- An exact reproducible candidate launch configuration matching the winning
  image, hardware and verified environment variables.
- A results table comparing baseline and winner under both quiet and compilation
  workloads, plus compilation-only build time. Include sustained MH/s,
  miner CPU core-equivalents, actual clocks, throttling and variability.
- A clear statement whether 215 MH/s was reached, whether CPU use fell, and
  whether mining was unaffected by compilation within the stated tolerance.
- If any objective remains unmet, identify the measured reason and the most
  credible next experiment, without promising an unmeasured result.

Remember: optimize complete, correct nonce tests per wall-clock second and
preserve my compiling performance. A dashboard peak, a requested GPU clock,
an increased nonce counter, or a benchmark while throttling my compiler does
not satisfy this task.

## 9. Primary references to verify technical assumptions

- Intel i5-1135G7 specifications (80 EUs; 1.30 GHz graphics maximum):
  https://www.intel.com/content/www/us/en/products/sku/208922/intel-core-i51135g7-processor-8m-cache-up-to-4-20-ghz-with-ipu/specifications.html
- IGC configuration flags for the inspected compiler version, including the
  XeHP+ qualification on BFN and add3:
  https://github.com/intel/intel-graphics-compiler/blob/v2.41.5/documentation/configuration_flags.md
- OpenCL event profiling and profiling-enabled queue requirements:
  https://registry.khronos.org/OpenCL/specs/unified/refpages/man/html/clGetEventProfilingInfo.html
- OpenCL event-status query behavior:
  https://registry.khronos.org/OpenCL/specs/unified/refpages/man/html/clGetEventInfo.html

Use primary documentation and the actual versioned source when a technical
assumption is uncertain. Do not substitute generic CUDA, AMD, Arc, or newer Xe
advice for this Tiger Lake Xe-LP device.
