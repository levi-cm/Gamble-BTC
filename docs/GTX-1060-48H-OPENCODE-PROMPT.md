# OpenCode task: 48-hour GTX 1060 Bitcoin miner optimization

Give this entire file to your selected OpenCode model with repository access.
This is an implementation task for that model. The investigation that prepared
this prompt did not start the 48-hour run. It prepared tools and deployed the
required VPN routing; it did not install a new mining backend.

---

## Mission and priorities

Work in `/DATA/test/Gamble-BTC` for an approximately **48-hour optimization
campaign**. Improve this experimental GPU Bitcoin solo miner for the installed
**NVIDIA GeForce GTX 1060 6GB**. The configuration has moved from an Intel
laptop to this NVIDIA machine. Optimize this branch for the new machine.

The preferred optimization direction is a **C-host CUDA/PTX backend specialized
for Pascal `sm_61`**. The user expects OpenCL may leave performance on the table.
Preserve OpenCL as the verified control and rollback, and investigate CUDA early
instead of spending most of the campaign on Intel-derived OpenCL tuning. This
is a direction to test, not evidence that CUDA automatically wins.

Aim as close as physically achievable to **1,000,000,000 correct Bitcoin header
SHA-256d nonce tests per wall-clock second**, while using essentially zero CPU
in steady operation and avoiding GPU thermal throttling. Prefer C for the host
runtime and low-level GPU code. A substantial rewrite or complete rewrite is
allowed if evidence justifies it. Keep a runnable, verified rollback version.

Prioritize in this order:

1. Correct work, candidate detection, target comparison, and share submission.
2. No reboots, thermal protection, and protection of other machine workloads.
3. Very low CPU consumption with responsive networking and shutdown.
4. Highest sustained hashrate within those requirements.

The 1 GH/s target is a goal, not an assumed capability. Do not fake progress by
counting SHA compression rounds as hashes, skipping necessary arithmetic,
counting queued or repeated work as completed, or displaying a short cold peak.
If the final result is below the goal, deliver the best proven implementation
and the measured limit. Reaching 1 GH/s early does not end the campaign: verify
it under sustained heat, reduce CPU cost, and strengthen reliability.

Use <=1% of one logical CPU as the initial operating target, <=0.5% as the
stretch target, and approach zero further when practical. These thresholds
make “basically 0%” measurable; they are not permission to ignore CPU overhead.
100% means one fully busy logical CPU. Also report CPU seconds / wall seconds.
Literal zero CPU is impossible for a networked C process with driver work.
Measure all miner threads together, attributable driver/helper activity where
possible, and monitoring/orchestration overhead separately. Do not obscure
consumption by dividing by the machine's total core count.

## Hard boundaries and authorization

- Read the applicable `AGENTS.md` files first.
- All work stays on **`gtx-1060-6gb`**. Never check out, merge into, commit on,
  or push `main`. Do not create a worktree or a different working branch.
- **No reboot under any circumstances.** Also no GPU reset, driver unload,
  driver replacement, Docker daemon restart, display server restart, or host
  change that requires rebooting. Installed NVIDIA drivers already work.
- No CPU nonce-search backend. CPU hashing is allowed for immutable job
  preparation, sparse candidate validation, and finite correctness tests.
- Do not change the wallet, worker identity, or pool without user instruction.
  Do not print `.env`, wallet values, full status payloads, or unsanitized logs.
- Preserve the required Mullvad SOCKS route for all live mining. The container
  entrypoint uses a strict proxychains chain with remote DNS, configured by
  `GBTC_SOCKS5_HOST` and `GBTC_SOCKS5_PORT`. Do not bypass the entrypoint, remove
  the proxy wrapper during a rewrite, or allow direct fallback on proxy failure.
  Reverify the Mullvad exit after deployment and test failure with an isolated
  unavailable endpoint, without disrupting the shared proxy. GPU-only probe
  and benchmark modes may bypass SOCKS because they do not use the network.
  Compose shares a dedicated Tailscale sidecar's network namespace and uses
  its local SOCKS endpoint at `127.0.0.1:1055`. The required preferred exit is
  `max-vprouter.dinosaur-dojo.ts.net`, with silent automatic fallback to
  `nico-vpn.dinosaur-dojo.ts.net` when the primary is unavailable. Preserve
  automatic failback, proxy-side DNS, Mullvad exit verification and the absence
  of direct routing when both exits fail. Do not change the host's Tailscale
  routes, advertise this miner as a public exit node, or reuse another node's
  identity/state. Include the sidecar/route manager's CPU use in measurements.
- This campaign authorizes scoped miner builds, tests, orderly miner service
  stops/restarts for uncontended experiments, and deployment of verified
  improvements. It does not authorize disturbing other applications.
- Never run two hashing workloads on this GPU simultaneously and label their
  results as uncontended. Serialize the existing miner and benchmark processes.
- The machine runs other Docker services. Do not stop them, prune their images,
  change their CPU quotas, kill their processes, or delete their caches.
- Preserve automatic fan control and firmware safety protections. No
  overclocking, voltage changes, raised power limit, fan policy changes, or
  thermal-limit changes. Improving code is the primary direction. Respond to
  heat by reducing this workload or cooling down, rather than raising limits.
- The status UI may be Tailscale only. No public or unrestricted LAN exposure.
- Use local Git continuously. Push verified major milestones to this project's
  correct GitHub branch as specified below. Never push `main` or force-push.
- Do not change OpenCode model selection or config. Use the user's chosen model.
  Do not spawn more agents unless separately requested.

## Evidence from the preparation investigation

These observations were collected on **2026-10-01**. Refresh them at startup;
they are reference evidence, not a complete performance or correctness audit.

| Item | Observed value |
| --- | --- |
| Repository branch | `gtx-1060-6gb` |
| HEAD | `a12938f230a3254cdae6d0accb829ccedfcf4a5c` |
| Origin | `https://github.com/levi-cm/Gamble-BTC.git` |
| Branch tracking | No upstream shown at inspection |
| GPU | NVIDIA GeForce GTX 1060 6GB, 6144 MiB |
| NVIDIA driver | `580.95.05` |
| Running miner | Docker container `gamble-btc`, service `miner` |
| Running image ID | `sha256:9ff600559a5142be04815fa1ad3dfb2ab266ea63b2eff36c1bea5b314f5d7167` |
| Backend/platform | OpenCL / NVIDIA |
| Kernel/local size | `unrolled` / `64` |
| SIMD setting | `auto` |
| Nonces per batch | `16777216` |
| Event poll interval | `1000` microseconds |
| Passive observation | `608.5906 MH/s` from completed-counter delta over `60.0968 s` |
| Miner process CPU over same interval | `1.06495%` of one logical CPU |
| GPU samples, every 15 seconds | 97% utilization, 72 C, 1835 MHz SM clock |
| Sampled GPU draw | Approximately 104.8–106.0 W |
| Power limit | 120 W, unchanged from default |
| Driver temperature values | Target 83 C; slowdown 99 C; shutdown 102 C |
| Thermal clock reasons at separate query | SW/HW thermal slowdown not active |
| CPU/toolkit tooling | Host PATH originally lacked CUDA; use the prepared developer container below |

The ~608.6 MH/s observation uses the backend's reported completed-work counter,
not independent GPU coverage proof. No shares were accepted or rejected in that
minute. A short zero-share window does not demonstrate a broken miner or a
correct submission path. The observations came from real pool mining, without
starting another GPU workload. Temperature/throttle observations do not certify
48-hour stability. The gap to 1 GH/s is approximately **64.3%**.

NVIDIA GPU utilization describes time with kernels executing; 97% does not
mean 97% instruction efficiency, occupancy, or theoretical SHA performance.
Investigate integer instructions, dependencies, register pressure, and spills.
Do not infer a hardware ceiling from utilization or floating-point TFLOPS.

Existing uncommitted edits were in `.env.example`, `AGENTS.md`, `Dockerfile`,
`README.md`, `compose.yaml`, and `src/main.c`. The preparation files are additional
work. Inspect the current diff, identify ownership, and preserve all preexisting
changes. Do not use `git reset --hard`, blanket stashes, or `git clean -fdx`.

## Installed tools and measured hypothesis screening

Native host C development tools are also installed: GCC/build-essential,
Clang/LLVM 19, CMake/Ninja/ccache, pkg-config, C EGL/GLES/cJSON and OpenCL
headers, gdb/Valgrind, Python development/venv support, ShellCheck/cppcheck,
pidstat/sysstat, strace and jq. A direct `make -j2` completed without warnings.
Host drivers and services were not replaced or restarted.

Use the already built developer image through `scripts/dev-toolbox.sh`; avoid
installing a host CUDA toolkit or changing the driver. The image is
`gamble-btc-toolbox:cuda12.9`, built from a pinned CUDA 12.9.1 Ubuntu 24.04
base. `scripts/dev-toolbox.sh scripts/check-dev-tools.sh` passed on the actual
GTX 1060. It mounts this checkout, uses your UID/GID and NVIDIA passthrough.
Build outputs under the mounted checkout belong to this project.

Available: GCC 13.3, Clang/LLVM, make/pkg-config, CUDA nvcc 12.9.86, ptxas,
cuobjdump/nvdisasm, nvprof 12.9, compute-sanitizer, cuda-gdb, OpenCL headers and
loader/clinfo, C EGL/GLES/cJSON headers, Python 3, jq, pidstat/sysstat, strace,
gdb/Valgrind, ShellCheck/cppcheck, Vulkan/SPIR-V inspection tools, and host
Rust/Cargo 1.75 with formatter/clippy. `tmux`, Docker and Git are on the host.
OpenCode 1.18.34 is installed at `/home/nico/.local/bin/opencode`; the preparation
has not selected a model, changed its configuration, authenticated a provider
or launched an agent. Preserve the user's existing model preference and access.

Use `nvprof` for Pascal profiling; bundled Nsight Compute does **not** support
Pascal. Nsight Systems is not installed and is not required for these two paths.
A real nvprof kernel/memcpy trace and compute-sanitizer memcheck completed on
the GTX 1060 (zero reported memory errors for the small CUDA screening run).
Hardware counter access may still be restricted by host driver policy; check once and record
limitations, rather than repeatedly trying unsupported tooling or changing
shared driver settings. Low-CPU host timing, ptxas resources and SASS inspection
are already available without hardware counters.

A private C/CUDA screening prototype reused the current generated OpenCL
algorithm and identical synthetic header, 64-thread blocks and 2^24 nonce
batches. Three CUDA variants passed **18 independent Python hashlib comparisons**
covering sparse complete candidate sets, exact prefilter equality, zero/nonzero
bases and a range ending at UINT32_MAX. These are screening tests, not complete
Bitcoin protocol or 48-hour acceptance evidence.

| Short synthetic screen | MH/s (30 s timed, 5 s warmup) |
| --- | --- |
| OpenCL unrolled, first / final control | 616.29 / 609.74 |
| Plain C-style CUDA translation | 621.96 |
| CUDA explicit funnel shifts and CH/MAJ lop3, first / repeat | 624.75 / 625.55 |
| CUDA explicit last-three-round pruning | 622.51 |
| Existing OpenCL pre3 | 609.82 |

The CUDA screening host used blocking context synchronization and measured
0.61–0.65% of one CPU core during its timed loop; this excludes live Stratum,
UI and sidecar overhead. All three CUDA kernels used 40 registers/thread with
zero reported spills/local storage. All had 2610 static SASS instruction lines;
the plain translation already emitted funnel shifts and lop3. Explicit tail
pruning did not reduce this count, consistent with existing dead-code removal.
There is no evidence of a large gain simply from changing API or host language.

The entire ordered screen lasted about four minutes, with no competing miner.
Temperature was sampled every two seconds, peaked at 75 C, and no sampled
SW/HW thermal-slowdown flag activated. These checks do not establish multi-hour
stability. Fixed-order runs, changing heat/boost and different host harnesses
mean the small CUDA advantage remains provisional. Repeat fair ABBA comparisons
with sustained clocks, CPU and correctness before committing a backend rewrite.
The goal still requires a substantial arithmetic/scheduling improvement.

Prototype source, binaries and raw records currently live under ignored
`bench-results/hypothesis-20261001/`. Read the sanitized preparation report in
`docs/GTX-1060-PREPARATION.md`. If promoting a prototype into maintained code,
move source to a tracked directory, add correctness coverage, and rebuild the
actual miner/UI; the screening cubins are not deployed mining backends.

## Read this code before proposing a rewrite

Read in this order, using focused file sections:

1. `src/opencl.c`, `src/opencl.h`, `src/backend.h`, `src/backend.c`.
2. `src/main.c`: `build_header_midstate`, `compute_merkle_root`, Stratum
   functions, benchmark loop, mining loop, metrics and HTTP/SSE threads.
3. `src/bench.c`, `src/sha256.c`, `src/stratum_protocol.c` and their headers.
4. `tests/test_opencl_conformance.c`, protocol, hash, benchmark, HTTP and UI tests.
5. `Makefile`, `Dockerfile`, `compose.yaml`, `.env.example`, launch/bench scripts.

Current important facts:

- This is one C binary. The UI is embedded HTML/JavaScript in `src/main.c`;
  there is no separate npm frontend that needs to be invented.
- OpenCL is implemented in `src/opencl.c` and loaded through
  `dlopen("libOpenCL.so.1")`. Vulkan remains a placeholder. GLES exists but is
  not the target path. Compose already provides `gpus: all`.
- Midstate reuse already removes the first 64-byte header compression per
  nonce. Two compression blocks remain in the standard per-nonce path.
- Available OpenCL variants are `unrolled`, `looped`, `dual`, and `pre3`.
  `pre3` already precomputes the first three nonce-independent rounds on the
  host, preserving the original midstate for feed-forward. Evaluate it;
  do not spend hours implementing an already present optimization.
- The generated second SHA currently emits all 64 rounds even though only its
  final `h` word is consumed by the GPU prefilter. Check actual compiler output
  before assuming those last operations survive dead-code elimination.
- The GPU filter checks `BSWAP32(0x5be0cd19 + h) <= target[0]`. The host rehashes
  returned candidates and performs the complete 256-bit comparison.
- Output stores at most 15 nonces. `run_batch` clamps the count, losing the
  distinction between exactly 15 and overflow. Existing tests acknowledge
  possible truncation; they do not implement recovery from lost candidates.
- `hashes_done` is assigned from requested nonce count after successful batch
  completion. Preserve exact coverage and counting when changing dispatch.
- The preferred path chains nonblocking input/reset writes and the kernel,
  flushes, sleep-polls the kernel event, then does a blocking output read.
  It is synchronous at the `run_batch` boundary. There is no existing two-slot
  asynchronous pipeline. The final read does not become guaranteed free just
  because kernel execution has finished; measure transfer/wait cost.
- Its fallback uses blocking writes and `clFinish`. Do not reintroduce busy
  host waits in pursuit of GPU speed. Measure waits on this NVIDIA driver.
- `GBTC_OPENCL_SIMD` emits the Intel-specific
  `intel_reqd_sub_group_size` attribute. It is not a valid NVIDIA tuning knob
  merely because parsing accepts it. Keep `auto` on NVIDIA unless supported
  behavior is established. Check the real extension/property definitions if
  changing the old queue-throttling hints; comments are not API evidence.
- The mining thread pumps Stratum between synchronous GPU batches. Longer
  batches/queued work can increase stale-job and shutdown delay. Candidates
  are currently handled before the subsequent Stratum pump. Test clean-job,
  difficulty-change and disconnect behavior when changing concurrency.
- GPU conformance already includes sparse complete-set checks, a nonzero base,
  end-of-range checks, equality and overflow cases. Extend these meaningfully;
  do not claim all current coverage is just all-match/no-match testing.
- `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/HARDWARE_MATRIX.md`, the Muse
  prompt, and several scripts retain Intel laptop assumptions. Historical Iris
  Xe results are not GTX 1060 baselines or proof of NVIDIA optimization limits.

In particular, `scripts/bench-run.sh` still defaults to GLES, passes Intel DRM
devices/groups, sets Intel/Mesa variables, and lacks NVIDIA `--gpus all`.
`scripts/bench-ab.sh` reads Intel `gt_act_freq_mhz`/PL1 sysfs paths. Do not use
these unchanged and present their output as NVIDIA measurements. Adapt a
minimal reliable harness first. `scripts/igc-dump.sh` is Intel-specific.

## Make a 48-hour run durable and bounded

Start the campaign clock when actual campaign execution begins, not when this
file was created. Persist `started_at_utc` and a deadline 48 hours later. Context
refreshes and agent restarts must not reset the deadline or previous decisions.
Use monotonic durations inside runs and UTC timestamps for cross-run records.

Do not rely on one chat turn remaining alive for 48 hours. Arrange persistent
execution in the available OpenCode environment, with resumable bounded work
units, an external thermal watchdog, and an independent deadline guard. Use a
documented supervised session or runner that continues through terminal loss.
If the environment cannot sustain/restart the agent, say so explicitly and
provide the exact resume mechanism. Never say “48-hour loop running” solely
because you wrote a shell script, started a benchmark, or intend to continue.

Keep concise human-readable status in `docs/GTX-1060-OPTIMIZATION-STATUS.md`
and sanitized milestone evidence in `docs/GTX-1060-OPTIMIZATION-RESULTS.md`.
Keep volatile state and raw measurements under ignored `bench-results/`.
Checkpoint before and after every experiment, at least every 30 minutes, and
before a context refresh. Include:

- Start/deadline, current phase, current experiment ID and next exact action.
- Best known correct build, baseline build and rollback build identities.
- Source Git commit plus dirty patch fingerprint, image ID and configuration.
- Latest throughput/CPU/temperature results and unresolved correctness issues.
- Rejected hypotheses and why; do not repeat them after context loss.
- Active owned process/container IDs, watchdog state, last telemetry time.
- Git milestones pushed, pending pushes, disk cleanup and preserved artifacts.

Acquire one campaign lock and one GPU workload lock. A file lock does not stop
the preexisting miner: explicitly inspect it and manage its service. Prefer
owned process groups and explicitly named experiment containers. Never use
`pkill miner`, broad container name matching, or cleanup traps that kill others.
Remove stale lock metadata only after proving the old owner has exited.

At the deadline, stop scheduling new experiments and finalize. Reserve the last
four hours for verification/reporting. Give every command a meaningful timeout
and bound termination grace. The supervisor must stop expired experiment
workers even if the model hangs. Preserve the verified service under its thermal
guard; stop an unsafe/unverified workload. Distinguish a benchmark worker from
the intended final running miner. Do not exceed the deadline by silently
starting another long soak.

### Suggested allocation, adjustable by evidence

| Elapsed campaign time | Work |
| --- | --- |
| 0–4 h | Inventory, locks, rollback capture, thermal guard, NVIDIA harness, baseline and correctness gates |
| 4–16 h | C-host CUDA/PTX prototype, Pascal ISA/resource analysis and direct comparison to OpenCL |
| 16–28 h | SHA arithmetic/code-generation and launch tuning on the best NVIDIA path; bounded OpenCL controls |
| 28–36 h | Combine independently proven winners, reduce CPU waits, verify job lifecycle and overflow |
| 36–44 h | Multi-hour continuous best-candidate mining with thermal, CPU and protocol observation |
| 44–48 h | Final regressions, live UI/build verification, milestone push, cleanup and results |

These are work budgets, not mandatory sleeps. Continue useful investigation,
implementation or sustained validation for the full campaign. Stop early only
for user cancellation, a real safety problem, or a blocker that prevents useful
progress. If ideas stop producing gains, use remaining time for stability and
reliability rather than inventing changes or rerunning identical benchmarks.

## Thermal guard: build this before repeated GPU tests

Use an independent, low-overhead monitor, preferably persistent NVML polling,
with approximately two-second samples. A single persistent monitor is preferable
to repeatedly launching heavy commands from every worker. Record its own CPU
cost. Capability-check fields; report unavailable data instead of filling in
zero. Monitor GPU temperature, power, fan speed where available, SM clock,
utilization, SW/HW thermal reasons and power-related clock reasons separately.
Record counter deltas where supported; historic counters are not this run's
throttling time. Occasional NVIDIA diagnostic queries can supplement NVML.

Initial conservative operating policy:

- Aim for a stable operating temperature **at or below 75 C**.
- Treat >75 C over three successive samples as a soft trip: end the active
  experiment/mining workload cleanly and enter cooldown. Also react earlier
  to a rising temperature trend that is about to cross the hard limit.
- Treat **78 C**, or any SW/HW thermal slowdown indication, as an immediate
  stop condition for the owned hashing workload. Lower this hard limit if
  fresh device limits require a larger safety margin; never raise it simply
  to improve a benchmark.
- Do not use the observed 99 C slowdown or 102 C shutdown temperatures as
  acceptable running targets. They are emergency thresholds, not objectives.
- Cool down until <=70 C continuously for 60 seconds before resuming, with
  the automatic fan policy intact. A cooldown taking over 15 minutes or three
  trips in one hour disables further high-load trials pending diagnosis.
- More than three missed two-second telemetry samples, loss of the GPU, an
  Xid/error relevant to this GPU, or an unresponsive workload means stop and
  diagnose. Do not continue blind and do not use a reboot/reset as recovery.

The guard must act independently of the agent's reasoning loop. Check stop
behavior with simulated telemetry transitions first. A bounded GPU kernel must
finish promptly enough for the temperature policy; do not create an unbounded
persistent kernel. `docker pause` freezes host tasks and does not guarantee that
already queued GPU commands stop: use orderly process/container termination
with bounded grace, or cooperative cancellation proven on this device.

Repeated trips mean the configuration is not sustainable. Reduce this miner's
load or choose a more efficient variant; do not benchmark it cold and report
that as the final sustainable result. Duty-cycle pauses, cooldowns, reconnects
and thermal interventions belong in the delivered wall-clock hashrate. Report
both active-kernel rate and overall rate if they differ. Thermal throttle flags
must remain inactive throughout the final confirmation; distinguish natural
boost variation and power capping from thermal throttling. No assertion of
“zero thermal throttling” if telemetry coverage is missing.

## Experiment protocol: one causal change at a time

For each experiment, write a short record before touching code:

1. Hypothesis: what limits the current code and what evidence points to it?
2. Existing mechanism: which functions already do part of this?
3. Smallest change: exact code/configuration and expected GPU/CPU direction.
4. Correctness gate and thermal/termination risk.
5. A/B measurement and rejection condition.

Then implement, build, run correctness gates, and compare against the current
best verified baseline. Each record ends KEEP, REJECT or INCONCLUSIVE with raw
measurement references. A compile success is not a performance result. Repeat
an inconclusive experiment only when a new measurement or hypothesis makes it
useful. Rejected changes should not accumulate in the live implementation.

Use warm, matched-temperature ABBA or randomized paired comparisons. Start
with 30–60 second screening runs after warmup; promising changes need several
2–5 minute paired runs, then 30–60 minute confirmation before becoming major
milestones. Final winners need the multi-hour run. Record medians, variability,
clock/temperature distributions and sample coverage. Restarting a GPU workload
can change boost and compiler startup costs; keep conditions comparable.

Retain a throughput winner only when its gain exceeds observed noise without
breaking CPU/thermal/correctness requirements. A repeatable ~1% gain can matter;
do not mandate arbitrary big gains, and do not count a noisy peak as a win.
Keep multiple Pareto choices when one lowers CPU/heat and another raises rate.

Measure completed nonce tests / monotonic wall duration, GPU event duration and
inter-kernel gaps separately. Enable profiling only in a measured diagnostic
mode if it changes behavior. Count nonces once, after successful completion;
maintain unique work identities and distinguish stale/repeated work. Benchmark
targets should make output atomics sparse, as in real mining. Easy-target
correctness tests must not contaminate throughput measurements.

Record backend, compiler/build options, kernel hash, local size, batch size,
wait strategy, driver/toolkit version, Git/image identity, correctness outcome,
CPU seconds, context switches where readable, GPU utilization, SM clocks,
power, temperatures, throttling and warmup. CPU startup/build costs should be
reported separately from sustained operation. Account for normal host services;
do not manufacture a quiet host by stopping unrelated workloads.

## Exactly two optimization paths

The user narrowed this task to **at most two ideas**. Do not reopen a broad
backend or language survey during the 48-hour campaign. Safety, correctness,
measurement, CPU reduction and thermal protection are requirements of both
paths, rather than additional competing ideas.

1. **C host + CUDA/PTX specialized for Pascal sm_61.** Test native rotate and
   `lop3` Boolean operations, register allocation, SHA specialization and
   genuinely blocking waits. This is the primary path for controlled NVIDIA
   instruction selection and a minimal low-level runtime.
2. **NVIDIA-specific OpenCL tuning in the existing C miner.** Keep this as the
   measured control, rollback and fallback winner. Test the existing `pre3`
   kernel, equivalent SHA arithmetic improvements, register/live-range choices,
   dispatch sizing and low-wakeup completion. Intel SIMD/IGC tuning is excluded.

Allocate most implementation effort to the first path after correctness gates.
Use short matched OpenCL controls, and let sustained measurements determine
which is deployed. If CUDA offers no reproducible gain, finish by improving
and validating the OpenCL path. Do not spend the campaign adding two complete
rewrites for their own sake.

### Investigated alternatives: closed for this campaign

- **Rust instead of C:** host language is not the device SHA instruction stream.
  A host-only rewrite cannot by itself eliminate the GPU arithmetic bottleneck.
  Rust can be fast, but there is no repository-specific evidence that rewriting
  the host improves hashing here. Current upstream Rust's NVPTX target requires
  Volta `sm_70` or newer starting with Rust 1.97; the GTX 1060 is `sm_61`.
  Pinning older nightly GPU toolchains adds work without a demonstrated gain.
  Keep C. Installed host Rust tools are available for inspection, not a third path.
- **Vulkan:** compute shaders are feasible on Linux, but the repository backend
  is a placeholder. Another SPIR-V/driver compiler pipeline has no measured
  advantage here and adds pipeline, synchronization and runtime implementation.
  It is excluded under the two-path limit, not declared universally slower.
- **DirectX:** Direct3D 12's native programming stack is Windows-oriented. This
  Linux deployment has no native D3D12 miner backend. An OS change or additional
  translation stack is outside this campaign and has no demonstrated SHA gain.
- **GLES and high-level GPU frameworks:** do not spend time porting or tuning
  them for this NVIDIA branch.

### Required measurement and correctness gates

Adapt the harness for `--gpus all`, the NVIDIA platform and selected candidate
image. Synthetic benchmarking must use `GBTC_BENCH_ONLY=1` and avoid wallet/pool
configuration; `--network none` is appropriate. Set environment variables
explicitly in the container instead of assuming host exports are forwarded.
For example, the probe should be passed with:

```sh
docker compose run --rm --no-deps -e GBTC_PROBE_ONLY=1 miner
```

Do not run GPU probes/conformance concurrently with throughput trials. Query
actual device/kernel limits and NVIDIA identity. Establish the default
unrolled baseline and screen `pre3`, `looped`, and `dual` on this GPU; historical
Intel rankings do not predict NVIDIA rankings.

Verify complete small-range prefilter candidate sets against an independent
SHA-256d oracle. Existing reference helpers share code with job preparation, so
also include authoritative known 80-byte header/hash vectors and an independent
reference where practical. Check byte order from full header to nonce submission,
full target equality, nonzero bases, nonce wrap policy, job/extranonce/ntime
changes, and every kept kernel variant. Do not make a GPU skip count as a pass.

Fix result overflow semantics before relying on larger outputs, broader targets
or pipelines. Preserve raw count/overflow information. Recover by safe GPU
rescanning of subranges or by an exact GPU target filter/output strategy; do not
silently discard valid shares or search huge ranges on the CPU. Avoid double
counting logical work during overflow recovery. Test with a local mock Stratum
server, including easy targets, to prove actual submit/response behavior without
requiring a lucky public-pool share in every test.

### Path 1: C host with CUDA/PTX for sm_61

A CUDA/PTX prototype is the user's preferred early investigation. Start after
the baseline and safety/correctness gates, while retaining the working OpenCL
path. Establish whether it improves instruction selection, register allocation,
wait behavior or measured throughput. Preserve a side-by-side comparison until
the new path wins. A C host using NVIDIA's CUDA Driver API and a compiled cubin
or PTX keeps the main program in C. Small C-style CUDA kernels are acceptable
GPU toolchain inputs; avoid introducing a large C++ framework or high-level
language runtime into the miner.

Query this device's compute capability; the GTX 1060 target is Pascal `sm_61`.
Use a pinned CUDA **12.x** toolchain with support for that target, preferably
in a build container. CUDA **13.0 removed offline compilation support for
Pascal**, so do not install “latest CUDA” and repeatedly try unsupported builds.
`nvidia-smi` displaying “CUDA Version 13.0” describes driver capability and is
not proof of an installed compiler or `sm_61` toolkit support. Do not replace
the working host driver to acquire compiler tools.

Prototype the same correct algorithm first, compile explicitly for the actual
target, inspect ptxas resource reports/SASS where supported, and compare
native rotations/ternary logic and instruction scheduling. CUDA context
`CU_CTX_SCHED_BLOCKING_SYNC` and blocking-sync event choices are candidates
for low-CPU waits; measure their actual behavior. Do not substitute spinning
stream/event queries. Keep the host runtime dependency minimal and document
container passthrough and runtime dependencies.

No rewrite is a win until correctness, sustained hashrate, CPU and thermal
behavior pass the same gates as OpenCL. If it loses, preserve the experiment
record and run the better backend. Handwritten assembly is a bounded last
experiment after ABI/instruction support and a loading route are established.

### Path 2: tuned OpenCL in C

Apply the shared kernel, dispatch and CPU work below to the existing OpenCL
backend. Retain only variants that pass correctness and measured acceptance.
Do not add another backend or change host languages.

### Shared kernel work within the two paths

Evaluate the existing `pre3` variant first. Derive extra nonce-independent
message-schedule terms for the padded header tail. Preserve unsigned modulo
2^32 arithmetic and the original compression feed-forward state. Cache
immutable job data with an identity covering all header and target changes.
Do not compile a new program for every incoming job without evidence that
startup overhead and stale time are acceptable.

Investigate pruning the second SHA's last operations. In the standard recurrence,
final `h` after 64 rounds equals `e` after 61 rounds. Derive indices and rotating
variable mappings explicitly, retain `0x5be0cd19` feed-forward and byte swap,
and compute everything required for that exact prefilter. Full host validation
must remain. Check whether the compiler already eliminates the unused tail;
three saved rounds alone cannot explain the ~64% needed for 1 GH/s.

Inspect native instruction selection for rotates, Boolean CH/MAJ/sigma XORs,
integer sums and byte swap. On NVIDIA, compare native rotate/funnel shift and
ternary logic mapping. PTX `lop3.b32` is available from `sm_50`; do not confuse
this with Intel's unavailable Xe-LP operations or newer PTX qualifiers. Verify
CH/MAJ truth tables/operand order and emitted SASS before crediting a rewrite.
Do not assume OpenCL accepts inline PTX. Compare source idioms first; use a
CUDA/PTX prototype if needed to control those instructions reliably.

Inspect registers per thread, spills/local memory, dependency latency and
instruction count using tools that actually support Pascal. Do not assume the
latest Nsight release profiles this old GPU. If binary/ISA inspection is not
available for OpenCL, record that limitation rather than inventing register
counts. Explore scalar rolling schedules, deliberate partial unrolling, live
range reductions and constant folding. Hoisting everything can create spills.
Do not globally force register caps without measuring the spill tradeoff.

### Shared dispatch work within the two paths

Screen local sizes 64, 128, 256 and, when legal/useful, 32 or 512. NVIDIA warps
are not Intel SIMD tuning modes. Device/kernel work-group limits are mandatory.
Compare one nonce/thread with a bounded nonce loop per thread only if it
amortizes setup without damaging register use or job latency. The existing
`dual` variant emits two sequential paths; do not assume this is optimal
instruction interleaving or twice the performance.

Screen batch sizes around 2^22, 2^24 and 2^26 when measurements justify it.
At the observed baseline, 2^24 nonces take roughly 27.6 ms; at 1 GH/s they
would take about 16.8 ms. Prefer completion/job response within about 100 ms;
test actual bounds and shutdown responsiveness. Huge batches cannot cure a
compute bottleneck and may increase stale work. Check range arithmetic and
rounding so changing dispatch never skips or repeats nonces unintentionally.

### Shared CPU and job-lifecycle requirements

Profile host event polling, final readback and inter-kernel gaps. Compare the
current 1 ms sleep-poll with adaptive sleeping based on recent batch duration,
bounded coarser polls, or an event callback that signals a condition variable.
Handle callback lifetime, lost wakeups, cancellation and driver error states.
Do not assume blocking driver APIs sleep without measuring them. Investigate
nonblocking readback followed by a low-CPU completion wait if useful.

If gaps justify it, add a shallow two-slot pipeline with separate inputs,
outputs, host transfer storage and events. Own each slot's job ID, target
generation, extranonce, ntime and nonce range. Do not reuse or free memory
before transfer completion, including error paths. Consume each result once.
Bound queued obsolete work and thermal-stop latency. Queue overlap does not
multiply throughput of an already compute-saturated GPU.

Keep networking, metrics and UI event driven. Condition-variable waits, blocking
socket readiness and low-frequency metrics are preferable to busy loops.
Moving Stratum handling to a lightweight thread can reduce clean-job latency
if queueing grows; introduce explicit immutable job ownership and locking.
Test reconnect/backoff rather than continuously hashing disconnected old work.
The Compose route guard now stops old work on route changes/loss and restarts
the process; ordinary pool disconnects still need proper C lifecycle handling.
Do not remove sparse candidate revalidation to save negligible CPU time.

## Local Git and GitHub milestones

At startup capture branch, HEAD, dirty-file list, sanitized original diffs and
running build identities. Preserve a baseline image/binary plus original patch
in the ignored campaign directory. Do not commit wallet/config/log artifacts.
If preexisting edits must be included in a coherent checkpoint, first identify
them as supplied NVIDIA migration work and keep that scope explicit; never
silently absorb another active writer's changes.

Make focused local commits for buildable, meaningful implementation checkpoints.
Stage explicit files or hunks; never indiscriminately use `git add .` on this
dirty checkout. Check the staged diff before committing. For rejected trials,
revert your own committed experiment or restore only your owned changes; never
reset the whole branch or erase the original migration work.

Push major **verified** milestones: a functioning NVIDIA harness/thermal guard,
a proven kernel improvement, a winning new backend, and final validated work
are examples. Do not push every noisy experiment or broken trial. Put sanitized
evidence and reproducible configuration in the milestone commit. User approval
for these branch pushes is already given; do not stop to ask again routinely.

Before each push:

1. Confirm the working branch is exactly `gtx-1060-6gb`.
2. Verify origin still points to `https://github.com/levi-cm/Gamble-BTC.git`
   (an equivalent SSH URL for the same repository is acceptable).
3. Fetch only the relevant branch metadata and inspect divergence. Do not
   merge `main`. Coordinate same-branch remote work before any integration.
4. Confirm the milestone's correctness/build/live checks and scan staged
   content for secrets and generated artifacts.
5. Push with an explicit refspec, for example:

```sh
git push -u origin HEAD:refs/heads/gtx-1060-6gb
```

Never use an unqualified `git push`, `--all`, `--mirror`, or force-push. Verify
the remote branch resolves to the expected milestone commit. If auth/network
or remote divergence prevents a safe push, keep the local commit, record the
exact blocker and continue independent optimization. Do not overwrite remote
work, edit credentials, publish elsewhere, or falsely report a successful push.

## Continuously remove obsolete build artifacts

Cleanup is part of the loop, not an end-of-task suggestion. Check owned artifact
storage after each experiment and at least every 30 minutes. Delete old build
files once a verified newer equivalent replaces them and no active process,
test, comparison or rollback needs them. Small obsolete binaries also qualify;
do not retain them merely because each is below an arbitrary size threshold.

Maintain an explicit retention set:

- Original runnable baseline and its required metadata/patch.
- Current best verified build.
- Previous verified winner for immediate rollback.
- Current active experiment and active A/B controls.
- Small source/ISA fingerprints and measurements needed to explain decisions.

After rejection/promotion, remove superseded object files, disposable test
binaries, old kernel/cubin dumps, temporary build trees and duplicate candidate
images that are outside the retention set. Check inactivity and replacement
identities; mtime or a directory name alone is not proof. Keep compact experiment
records after removing their reproducible bulky binaries.

For repository files, resolve exact paths inside this repository, verify they
are generated and ignored/untracked, preview the paths and reclaimed bytes in
the log, then remove only those specific owned paths. Preserve tracked sources,
manifests, lockfiles, dependencies, active build trees and unknown directories.
Avoid recursive wildcard cleanup, broad `make clean` while another build runs,
and `git clean -fdx`. Make future experiment directories clearly owned.

For Docker, record candidate tags/image IDs as you create them. Inspect active
container references and retained rollback tags before deleting exact obsolete
owned image IDs/tags. Do not use `docker system prune`, broad builder/image/volume
prunes, global cache deletion or cleanup of other repositories. Shared Docker
layers/cache do not become yours just because this build used them. Bound
candidate retention and use an owned builder if isolated build-cache cleanup
is needed. Record free disk/space reclaimed. Stop generating new bulky builds
if safe retained artifacts cannot fit; do not solve that by deleting rollback.

## Build, deploy and verify every implementation

After **every implementation**, including backend-only changes, rebuild the Web
UI from the latest code and update the running miner to that build. Here the UI
is part of the C binary, so rebuilding the image and restarting the scoped miner
updates both backend and UI. Also rebuild/redeploy any newly separated affected
components. No stale running binary/UI when claiming a completed change.

Use unique candidate image identities and retain rollback images. During
experiments the rebuilt candidate may run as the sole scoped test miner;
rejected candidates must be replaced by the retained verified winner. Keep
production wallet/config values private. Do not overwrite the only known-good
image or continue serving an old UI after replacing its backend.

Use `make test` for deterministic helpers, `docker compose config --quiet` for
wiring, image builds with warnings inspected, and actual GPU conformance in a
test-capable container with NVIDIA passthrough. The Dockerfile now has a
`tests` stage, and the installed toolbox can build/run tests from the checkout.
The current full `make test` fails in the legacy Intel launcher's PTY key test
(expected 143, got 137); the eight C executables, including real NVIDIA
conformance, passed separately. Record this existing failure and fix/adapt the
launcher only as needed for the retained campaign. Never call the full suite
green or hide a skipped GPU test. New route checks can be run with:

```sh
docker run --rm --network none --entrypoint sh \
  -v "$PWD:/workspace:ro" gamble-btc:latest /workspace/tests/test_route_guard.sh
docker run --rm --network none --entrypoint sh \
  -v "$PWD/scripts:/workspace/scripts:ro" -v "$PWD/tests:/workspace/tests:ro" \
  gamble-btc-tailscale:latest /workspace/tests/test_vpn_failover.sh
```

Use sanitizers for changed host ownership/concurrency, relevant protocol/UI
tests, and `git diff --check`. Run meaningful tests for every retained change.

Verify the live HTML, `/status.json`, and SSE from the actual rebuilt candidate.
Use a real browser to confirm status updates, disconnect/reconnect behavior
and that the displayed build/kernel/backend are current. A separate heavy web
framework is unnecessary. Maintain compatibility or document intentional status
contract changes. Show honest average/sustained rates and thermal-pause status.

Current host publication is loopback only: `http://localhost:41174/`.
At preparation time the machine's Tailscale IPv4 was `100.124.44.53`, so the
corresponding address would be `http://100.124.44.53:41174/`, but **that address
was not exposed by the inspected loopback-only port mapping**.

The sidecar also has its own authenticated tailnet identity. At preparation
time it was `gamble-btc-gtx1060.dinosaur-dojo.ts.net`, IPv4 `100.104.57.98`.
Its UI was fetched successfully from a separate peer in that tailnet. Resolve
that address with `docker compose exec tailscale tailscale ip -4`; its userspace
networking forwards incoming Tailscale TCP to the shared local HTTP server.
This access already works without publishing an unrestricted host interface.

Resolve host `tailscale ip -4` again when configuring/reporting. Bind the Docker host
port explicitly to that IP for Tailscale access; optionally also keep loopback.
The process may listen on `0.0.0.0` inside the isolated container for forwarding;
that is different from publishing the host port on every interface. Verify the
host bindings and actual access. Do not use host `0.0.0.0` publishing as a
shortcut. If Tailscale is absent/offline, report that and retain safe local access.
Whenever reporting a development URL, give the local and current Tailscale
forms with the same protocol/port; label unavailable endpoints accurately. In
strict Tailscale-only mode, say that the localhost endpoint is not listening.

## Finish with evidence, not promises

At 48 hours deliver and checkpoint:

- Best correct implementation running with the latest embedded UI, or the
  safe retained baseline if no candidate passed. Preserve the thermal guard
  while leaving mining running; unsafe workloads remain stopped.
- Local commits and verified GitHub pushes on `gtx-1060-6gb` only.
- A concise source-backed explanation of the remaining bottleneck and whether
  a rewrite helped.
- A baseline/final table: sustained MH/s and GH/s, fraction of 1 GH/s achieved,
  CPU seconds/wall seconds and one-core %, temperature distribution/max,
  clocks/power, thermal trip/throttle counts, telemetry coverage, test outcomes
  and multi-hour duration. Include all pauses in delivered throughput.
- An experiment ledger with KEEP/REJECT/INCONCLUSIVE, causal hypotheses and
  actual results. List changes that remain unverified.
- Actual image/Git identities, safe reproducible configuration, current status
  URLs, rollback instructions and whether accepted-share evidence exists.
- Cleaned paths/image IDs and reclaimed space, plus the retained baseline,
  winner and rollback inventory.
- Exact campaign elapsed time, interruptions, remaining blockers, and any
  pending remote push. Do not claim an unattended 48-hour run if it was not one.

Do not declare 1 GH/s achieved from an instantaneous UI maximum. Do not declare
basically zero CPU from a rounded monitor display. Do not declare no throttling
from one `nvidia-smi` snapshot. Deliver the best result that survives sustained,
correct, low-CPU, thermally controlled operation on this GTX 1060.

## Primary references for uncertain technical decisions

Use these as starting points; verify version/device support before applying
features. Links document mechanisms, not promised performance gains.

- [NVIDIA Pascal tuning guide](https://docs.nvidia.com/cuda/archive/12.9.1/pascal-tuning-guide/index.html)
- [PTX 8.8 instruction reference, including lop3 and funnel shifts](https://docs.nvidia.com/cuda/archive/12.9.1/parallel-thread-execution/index.html)
- [CUDA 13.0 release notes: removed Pascal offline compilation](https://docs.nvidia.com/cuda/archive/13.0.0/cuda-toolkit-release-notes/index.html#deprecated-architectures)
- [CUDA 12.9 Driver API context scheduling](https://docs.nvidia.com/cuda/archive/12.9.1/cuda-driver-api/group__CUDA__CTX.html)
- [NVIDIA SMI: temperature and clock event reasons](https://docs.nvidia.com/deploy/nvidia-smi/index.html)
- [OpenCL event profiling requirements](https://registry.khronos.org/OpenCL/specs/unified/refpages/man/html/clGetEventProfilingInfo.html)
- [OpenCL API specification: events, callbacks and transfer ownership](https://registry.khronos.org/OpenCL/specs/unified/html/OpenCL_API.html)

- [Rust NVPTX platform support and minimum GPU generation](https://doc.rust-lang.org/rustc/platform-support/nvptx64-nvidia-cuda.html)
- [Vulkan compute shader specification](https://docs.vulkan.org/spec/latest/chapters/shaders.html)
- [Direct3D 12 programming guide](https://learn.microsoft.com/en-us/windows/win32/direct3d12/directx-12-programming-guide)
- [CUDA 12.9 profiler support table](https://docs.nvidia.com/cuda/archive/12.9.1/profiler-users-guide/index.html)
