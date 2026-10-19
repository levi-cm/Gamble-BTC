# Rust miner CPU dependency investigation

Date: 2026-10-19. Target: i5-1135G7 / Iris Xe, production Rust candidate
`efd5ae39648051b28b55d8343d5a1c5859943fc52ada08cda4196350ee368817`.

This records the original investigation and implementation recommendation.
The subsequent `while-xmrig` implementation and qualification are documented in
[WHILE-XMRIG.md](WHILE-XMRIG.md).
The practical objective is useful BTC throughput alongside four XMRig RandomX
workers, without reserving a CPU core, limiting XMRig, or changing host policy.
Existing unrelated workloads are left untouched.

The measured primary limit is shared package power, not lack of CPU scheduling
time for the miner. The baseline reached 176.2 MH/s at a mean 1,167 MHz; with
four offline RandomX workers, BTC reached 79.1 MH/s at 495 MHz. The mining
thread's scheduler runqueue delay was only about 0.06% of wall time under that
load. These observations do not rule out small host gaps, but do not support
explaining a roughly 55% throughput loss as CPU starvation.

## What CPU dependence means here

Three mechanisms must be measured separately:

1. CPU time consumed by the BTC process, including driver threads.
2. GPU idle time while a runnable host thread waits to be scheduled or prepares
   another submission. Low process CPU utilization does not exclude this.
3. Shared CPU/GPU package power, cooling and memory-system competition. Reducing
   BTC host CPU time cannot isolate the iGPU from XMRig's power consumption.

The preceding balanced five-minute comparison measured Rust at about 1.43% of
one CPU core, with PL1 limiting and varying clocks. There is little CPU time to
recover in absolute terms. Improving submission continuity may matter more than
removing small Rust allocations. Neither mechanism currently proves how much
BTC hashrate can be retained with XMRig.

## Source findings

### 1. One synchronous GPU slot is the main architectural dependency

`rust/gbtc-core/src/backend.rs`: `ComputeBackend::run_batch` is synchronous.
`rust/gbtc-backend-vulkan/src/lib.rs:46` owns one command buffer, one fence and
one output buffer. `dispatch_and_read` records, submits, waits and reads results
before returning. `Controller::tick` then processes events/candidates/status and
constructs the next work item before another submission can occur.

At 170 MH/s, the default 16,777,216-nonce batch represents approximately 99 ms
of wall processing. A hypothetical additional 5 ms host gap after each 99 ms
dispatch costs about 4.8% of throughput; 20 ms costs about 16.8%. These are
illustrations, not measured gaps. Current code has no GPU timestamp queries to
separate actual execution from these gaps.

**Recommendation:** introduce an explicit submit/retire interface with a bounded
two-slot queue. Each slot owns its command buffer, fence, output buffer and
immutable connection/job/target/extranonce/range snapshot. Put the next valid
range on the GPU queue before verifying the previous range's candidates or
updating the UI. Do not depend on concurrent kernel execution; the objective is
to keep a serial GPU queue fed when the host is briefly unavailable.

Drain pending protocol events before queue refill. Bound queued old-job work by
an explicit time budget. Already-submitted work cannot simply be cancelled when
a clean job arrives; its candidates must still be classified against the saved
generation. Never overwrite an output buffer or descriptor referenced by pending
commands. Preserve noncoherent invalidation, transfer/compute/host barriers,
overflow recovery and teardown guarantees independently for each slot.

### 2. The completion loop wakes approximately every millisecond

`rust/gbtc-backend-vulkan/src/lib.rs:669` repeatedly calls `get_fence_status`
and sleeps for `GBTC_VULKAN_POLL_US`, default 1000 us. The GPU performs all nonce
searching, but the host still wakes repeatedly throughout each dispatch. Sleeps
can resume late under load. A longer fixed interval lowers wakeups but can make
the empty-queue period after completion longer.

**Recommendation:** benchmark bounded `wait_for_fences` calls against polling on
the actual ANV driver. A short finite blocking wait can wake when the fence
signals without an unconditional polling delay, while allowing stop/deadline
checks after each timeout. Start with 5–10 ms timeout slices; preserve the
absolute 30 s dispatch deadline, device-loss handling and cancellation poisoning.
Confirm CPU time and syscall behavior: Vulkan API names alone do not guarantee
that a driver avoids spinning. The existing prior-slot wait near line 462 should
normally find the fence signaled; removing it alone will not solve the main loop.

Treat 5 ms polling as an experiment, not an automatic production default. Without
queueing, it trades lower wakeups for potentially longer GPU idle gaps.

### 3. Rebuilds of unchanged work are avoidable

`rust/gbtc-miner/src/controller.rs:214` clones the selected `MiningJob`; lines
248–262 clone extranonces and the job again. Every batch calls `MiningJob::work`
in `rust/gbtc-core/src/job.rs:98`. That calls `header`, rebuilds the coinbase and
Merkle root, and calculates the header midstate even when only the nonce range
changes. The backend also recalculates the three-round prefix per dispatch.

**Recommendation:** cache prepared header/midstate/tail/prefix by immutable work
identity. Store a shared immutable job snapshot rather than repeatedly copying
coinbase and Merkle vectors. Update only the nonce base/count for the next range.
Invalidate correctly on connection/job/header/extranonce changes; target changes
must remain attached to the correct work generation even when header preparation
can be reused. At this hardware's throughput an extranonce may cover tens of
seconds and hundreds of batches.

This is most relevant to the real mining controller. The synthetic benchmark
already reuses one prepared work item, so it cannot demonstrate this gain.
Keep full candidate rehashing on the CPU: candidates are rare, and that independent
check protects against incorrect GPU results. Do not introduce CPU nonce search.

### 4. Status housekeeping creates avoidable wakeups and lock exposure

`rust/gbtc-http/src/lib.rs:229`: the accept loop wakes every 50 ms even with no
clients; its sampler wakes every 100 ms to perform a once-per-second sample.
Each SSE client also wakes every 100 ms during its one-second interval.
`rust/gbtc-miner/src/lib.rs:24`: the shutdown watchdog wakes every 100 ms before
shutdown is requested. These wakeups are additional to the mining fence loop.

Status snapshots clone JSON while holding the mutex, and the controller takes
the same mutex for hashrate/lifetime updates after batches. Slow network writes
already occur outside the status lock; there is no reason to undo that protection.

**Recommendation:** use interruptible condition-variable/event waits with actual
deadlines, publish a compact once-per-second snapshot and serialize outside the
mining critical path. A listener readiness wait should also have an explicit
shutdown wakeup. Preserve the 16-client bound, I/O deadlines, 300-point history
and joined shutdown. Merely replacing short sleeps with long uninterruptible
sleeps worsens shutdown. This is a smaller optimization than submission design.

### 5. Protocol I/O can delay a share independently of GPU throughput

`rust/gbtc-stratum/src/lib.rs:13` uses a 100 ms blocking socket read timeout.
The connected loop drains incoming data through `read_line` before examining
the outgoing share channel. If a share arrives during an otherwise idle read,
the sender may wait for that read timeout before sending it.

**Recommendation:** wait on socket readiness plus a channel-wakeup/event source,
with absolute handshake/pending-response/shutdown deadlines. Preserve processing
of incoming clean-job events before outgoing shares, bounded frames/channels and
matched response accounting. This primarily improves submission latency and
idle wakeups, not SHA throughput. A full async framework is optional; adopting
one is not itself a performance fix.

### 6. Small hot-path allocations are secondary

`rust/gbtc-backend-vulkan/src/lib.rs:572` allocates an 88-byte push-constant Vec
per dispatch. Empty candidate vectors, sorting and candidate deduplication also
appear on the completion path. The normal prefilter at supported pool difficulty
returns very few candidates, so output copying and independent validation are
unlikely to dominate steady-state CPU use.

Use a fixed array for push constants, cache invariant prefix fields and reuse
bounded storage where this simplifies ownership. Do not remove complete-set
overflow rescans or deduplication to reduce CPU work. Measure before introducing
unsafe pools or complicated allocation strategies.

## Important alternatives ruled out or not established

- ANV on the tested integrated device reports `maxComputeWorkGroupCount.x` as
  2,147,483,647. The default batch is not forced into many tiny dispatches by
  a 65,535-workgroup limit. The software CPU device separately reports 65,535;
  confusing those devices would produce the wrong diagnosis.
- A larger batch amortizes launch work but does not remove the 1 ms wake loop.
  It also increases stale-job reaction delay. Prefer a bounded queue and measured
  dispatch-time target before increasing batch size globally.
- No evidence yet shows CPU hashing of ordinary nonce ranges in production.
  Host hashes perform work preparation and candidate verification.
- RandomX's dataset and shared cache/memory traffic may also interfere. The SHA
  shader does most nonce work in registers and transfers small work/result data;
  a large shared-memory-bandwidth explanation is not established by source alone.
- Priority changes, CPU affinity reservations, fewer XMRig workers and power
  policy changes would change the workload rather than fix the Rust dependency.
  They are outside this investigation.

## Implementation order and acceptance

1. Add low-overhead per-stage counters: work preparation, queue submission,
   completion-wait CPU/wall time, candidate validation and status publication.
   Add GPU timestamp queries where supported, and record scheduler delay,
   wakeups, actual clocks, package power and throttling. Report raw unique hashes
   over total wall time as well as execution-only throughput.
2. Test event-driven fence waiting. Keep it only if wakeups/CPU improve without
   losing loaded wall throughput or shutdown responsiveness.
3. Implement the two-slot submission/retirement interface with a small explicit
   queued-time budget. This is the strongest architectural candidate for making
   GPU feeding less sensitive to host scheduling, but gains remain unmeasured.
4. Cache prepared immutable work and move status work off the submission path.
5. Reduce protocol/UI/watchdog wakeups with explicit wake-on-shutdown semantics.
6. Optimize GPU hashes per joule separately. The live experiment identifies
   shared power as the dominant limit, so this is the priority for recovering
   substantial loaded hashrate. Host-loop changes above primarily reduce CPU
   overhead and improve scheduling resilience; they are not an established
   remedy for the measured clock collapse. Further shader/occupancy/instruction
   experiments must measure hashes per joule and both loaded hashrates.

For each step, test BTC alone, XMRig alone, and both together with four RandomX
workers and unchanged settings. Use at least five measured minutes per trial,
balanced repeated ordering, separate dataset initialization, record huge-page
availability and both hashrates. Add the actual local Stratum/HTTP workload;
synthetic benchmark mode omits those costs. Evaluate BTC retention and XMRig
retention together rather than maximizing a single dashboard number.

Correctness gates include complete candidate sets/overflow, exact final nonce,
clean and nonclean job transitions, reconnect/target changes, queued-generation
retirement, cancellation/device loss, bounded shutdown, validation layers and
independently accepted local-pool shares. Simulated delayed host scheduling must
show bounded queue behavior without duplicate or stale submissions.

### Experiments aimed at the measured power bottleneck

The host is already feeding compute with little CPU time. A useful follow-up
shader investigation should therefore optimize the concurrent workload directly:

- Compare local sizes 32, 64 and 128 using identical XMRig settings. Do not infer
  occupancy from the workgroup size alone; inspect compiled SIMD width, register
  allocation and spill/fill counts on ANV.
- Recheck unrolled/pre3/pruned variants under the loaded condition. Historical
  ISA inspection found no spills in unrolled/pre3 and identical instruction
  statistics for pruned versus unrolled. A name suggesting fewer rounds is not
  evidence of less actual work after compiler optimization.
- Only then try interleaving two independent nonce states per invocation to hide
  arithmetic latency. This can double live registers and reduce occupancy; keep
  it only if measured joint throughput improves without spills/correctness loss.
- Reuse the compiled compute pipeline. Per-dispatch rerecording is host work;
  pipeline/shader compilation already occurs at initialization and is not a
  recurring cost that needs another language rewrite.
- Record BTC MH/s, XMRig H/s and package joules together. Total package energy
  includes both miners, while the readable `uncore` domain is not an exclusive
  BTC-process energy meter. Do not label package watts as GPU watts or optimize
  a combined hashrate number that adds incompatible hash algorithms.

The acceptance criterion should be a Pareto improvement: more BTC throughput
with no material XMRig loss, or less host CPU/wakeup activity with unchanged
throughput. Clock-normalized rates are supporting diagnostics, not replacements
for sustained concurrent wall throughput.

## Live experiment record

The investigation runs the unchanged production image, `pre3`, local size 64,
batch 16,777,216, with 30 s warmup and 300 s measured time. Trials are baseline
1 ms polling, four-thread offline RandomX plus 1 ms polling, and the same RandomX
workload plus 5 ms polling. CPU and scheduler counters and GT/RAPL telemetry
are recorded locally. No MSR/cache-QoS/sysctl changes, CPU caps, core reservations,
public-pool submission or runtime source edits are used. The offline XMRig
configuration disables autosave, MSR access, GPU backends and benchmark upload.

These one-pass experiments identify mechanisms; they are not balanced
performance qualification or a prediction for every real pool/UI or XMRig
configuration.

### Completed results

| Condition | BTC MH/s | BTC CPU, % of one core | Mean GT MHz | Main-thread voluntary context switches/s |
| --- | ---: | ---: | ---: | ---: |
| BTC baseline, 1 ms poll | 176.16 | 1.44 | 1,167 | 906 |
| BTC + four RandomX workers, 1 ms poll | 79.12 | 1.80 | 495 | 927 |
| BTC + same RandomX workers, 5 ms poll | 77.29 | 0.60 | 484 | 196 |

Each measured interval exceeded 300 seconds. Under RandomX, GT PL1 was asserted
in essentially every measured sample. No GT thermal-throttle flag was observed;
this is not a measurement of CPU core temperatures.

The loaded 1 ms trial retained 44.9% of baseline BTC throughput. Average GT
clock retained 42.4%. The mining thread accumulated only 0.624 ms of scheduler
runqueue delay per wall second under that load, versus 0.861 ms in the baseline.
These counters support shared power/clock limits as the primary observed
mechanism, rather than large runnable-but-unscheduled delays. They do not
directly measure GPU idle gaps; GPU timestamps remain necessary for that.

Late-window RAPL readings (last approximately 150 seconds of available samples
per condition) showed package power near 25 W in both baseline and loaded
conditions. CPU-core power rose from approximately 6.8 W to 17.0 W while the
uncore domain fell from approximately 15.7 W to 4.7 W in the 1 ms loaded trial.
These are hardware energy-domain readings, not power attributed exclusively
to either process. The readable configured PL1 constraint was 35 W; observed
package power around 25 W must not be described as proof of a 35 W operating
budget or the precise firmware policy responsible.

The 5 ms experiment reduced BTC process CPU by 66.5% and voluntary context
switches by 78.8%, while reported BTC throughput was 2.3% lower. GT clocks were
also 2.3% lower, so this trial does not isolate a polling-induced throughput
regression or establish 5 ms as a production winner. It does establish that much
of the host overhead can be removed without recovering the large power-related
loss. Prefer testing an event-driven completion wait to deliberately introducing
a longer fixed completion-detection delay.

XMRig 6.26.0 reported approximately 1.2 kH/s throughout the two loaded windows.
The dataset was 2,336 MB with 0% huge pages; four worker scratchpads used 100%
huge pages. No host huge-page provisioning was changed. A configuration with
dataset huge pages may behave differently. There was no XMRig-alone control,
so this experiment cannot quantify how much BTC reduces XMRig throughput.

The tests use synthetic GPU work. They omit real-controller header preparation,
Stratum and HTTP/SSE costs; the static findings for those paths remain proposed
optimizations pending full-runtime profiling. No kernel/driver/API changes or
runtime source edits were made. Only task-owned benchmark/XMRig processes were
stopped; the runner's initial two logging-readiness attempts were discarded
before either loaded measurement. The eventual file logger confirmed readiness
before all loaded trials. All task-owned mining processes exited afterward.

Local evidence:
`bench-results/cpu-investigation-20261019T074159Z/` contains the three raw benchmark
JSON/logs, per-trial `*-scheduler.json`, `power.jsonl`, `analysis.json`,
`xmrig-offline.json` and `xmrig.log`. The source of the temporary trial runner is
saved alongside those files. Scheduler counters describe the main mining thread;
CPU percentages come from the complete miner container cgroup.

### Decision

The code can be made less dependent on frequent CPU wakeups. Start with measured
event-driven fence waits, then cached immutable work and a bounded two-slot
queue with explicit job-generation ownership. Keep candidate validation and
bounded error/shutdown behavior. Do not promise that these changes preserve
176 MH/s while four RandomX workers run: the observed limiting resource is
primarily shared package power. Recovering a large part of that loss requires
better GPU throughput per available watt, and must be evaluated jointly with
XMRig's throughput.
