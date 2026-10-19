# Rust miner alongside XMRig

Branch: `while-xmrig`. Target: i5-1135G7 / Tiger Lake Iris Xe.

This implements the host-dependency recommendations in
[RUST-CPU-DEPENDENCY.md](RUST-CPU-DEPENDENCY.md). The existing C and Rust reference
images remain available. The branch builds `gamble-btc:rust-while-xmrig`, with a
separate `gamble-btc:rust-while-xmrig-debug` validation image. The launcher defaults
to container `gbtc-rust-while-xmrig` and localhost port 41176.

## Runtime changes

Vulkan uses two independently owned command buffers, fences, result buffers,
descriptor sets and timestamp query pools. `submit_batch` returns a ticket;
`retire_batch` consumes it exactly once. The synchronous API rejects a full queue
instead of overwriting an outstanding result. Overflow rescans reuse only the
retiring slot and preserve the other slot's results. Cancellation during a wait
poisons the context; teardown retains its bounded device-idle watchdog.

The production controller calibrates with one small dispatch, then sizes up to
two batches using observed GPU execution time and an estimated queued-time
budget. This is an estimate, not a hard GPU preemption deadline: a sudden clock
drop can make already-submitted work run longer. Each ticket retains immutable
job, connection, target, extranonce and nonce-range metadata. Protocol events
are drained after completion and before refill; candidate rehashing and status
updates follow refill. Failed submissions restore the reserved nonce state.

Immutable jobs use `Arc`; a bounded prepared-work cache stores header midstate
and tail words per job identity, connection and extranonces. Backend push
constants use a fixed 88-byte array and cached three-round SHA state. Host SHA
remains an independent correctness check; no CPU BTC search backend was added.

Production completion waits use finite Vulkan fence waits, default 10 ms slices,
with a 30 s absolute dispatch deadline. Polling remains available for comparison.
HTTP listener readiness and Stratum socket/share readiness use an owned eventfd
for immediate wakeup. Shared descriptor ownership prevents callbacks or senders
from writing through a closed/reused descriptor. The sampler and SSE clients
wait on one-second deadlines; the watchdog blocks until shutdown. Legacy callers
that supply only an atomic stop flag retain bounded polling compatibility.

The status API publishes backend submission/completion/wait counts, cumulative
GPU execution nanoseconds when supported, and controller preparation,
validation and status wall nanoseconds. These wall times are not CPU times.
The dashboard's queued-path rate uses completed hashes over a wall-time window,
not only time spent inside the retirement call. Benchmark telemetry excludes
warmup and includes all measured queued-tail completions.

## Controls

| Variable | Default | Meaning |
| --- | --- | --- |
| `GBTC_VULKAN_WAIT_MODE` | `fence` | `fence` or legacy `poll` |
| `GBTC_VULKAN_WAIT_US` | `10000` | Fence timeout slice, 1000–100000 microseconds |
| `GBTC_VULKAN_POLL_US` | `1000` | Sleep interval in poll mode |
| `GBTC_GPU_QUEUE_MS` | `200` | Estimated controller queued-work budget, 50–1000 ms |
| `GBTC_BATCH_NONCES` | `16777216` | Maximum controller batch; fixed synthetic benchmark batch |

The synthetic benchmark keeps two fixed-size batches in flight; the real
controller additionally adapts dispatch size to the queue budget. Therefore
synthetic results alone do not establish pool/UI coexistence performance.

```sh
make test-rust
make rust-container
scripts/start-rust-miner.sh --probe
# After configuring .env and using AC power:
scripts/start-rust-miner.sh
```

## Reproduce the measurements

```sh
python3 scripts/bench-coexist.py \
  --xmrig ../xmrig/build-final/xmrig \
  --cases scripts/while-xmrig-cases.json \
  --alone baseline,candidate \
  --order baseline,candidate,candidate,baseline,local32,local128,unrolled,pruned \
  --seconds 300 --warmup 30 --control-seconds 300 \
  --runtime-image gamble-btc:rust-while-xmrig \
  --output bench-results/while-xmrig-NEW
```

The runner resolves tags to immutable image IDs before starting. Every synthetic
trial has at least five measured minutes and separate warmup. Four offline
RandomX workers run unchanged throughout the XMRig-alone control and loaded
trials. MSR access, cache QoS, affinity, benchmark upload, public pools and
configuration autosave are disabled. Existing huge pages may be used; the runner
does not provision them or change host policy. It only stops its own child and
containers. Other workloads remain untouched.

Raw artifacts contain BTC complete-container cgroup CPU use, main-thread
scheduler/context-switch counters, actual GT clocks/throttle flags and RAPL
energy counters. Package energy includes both miners and other host work; it
must not be described as GPU-only energy. XMRig rates are calculated from its
progress counter over the reported covered interval, which can be shorter than
the enclosing five-minute BTC interval. The runtime follow-up exercises local
Stratum, reconnects, HTTP and SSE for 330 seconds. Its full-app CPU/hash
analysis uses the qualifier's 20-second startup grace, leaving over 300 seconds
of sampled work; XMRig progress analysis starts after 30 seconds. A real wallet and public pool are unnecessary.

Local-size and shader screens are exploratory single runs. Balanced repeated
baseline/candidate ordering supports the host-loop comparison; a single faster
variant does not by itself justify changing the production default. Interleaving
two independent nonces per invocation remains conditional on compiler/occupancy
and measured joint-throughput evidence, as specified in the investigation.

## Verification and results

The final build passes 153 Rust tests, formatting and all-target Clippy with
warnings denied. The C tests and existing shell benchmark contracts also pass.
The final debug image completed a 150.6-second local-pool test with 10 accepted
shares, no invalid/duplicate/stale submissions, two connections and shutdown in
0.40 seconds. No performance rate is inferred from that shorter correctness run.

The production image ID is
`sha256:0b8ca2b0722ee085d77df23178bef987149c96c82a12f263938e651cadeeb6b6`;
the debug image is
`sha256:2a51cf86eb93bacca680d9e843b0f7ee1bca9cf01d0c38af8fe7400d0a9bc54d`.
Build inputs and test logs are recorded in `/tmp/gbtc-while-xmrig/`.

### Sustained host-loop comparison

Each BTC trial measured at least 300 seconds after 30 seconds of warmup. The
loaded comparison used baseline/candidate/candidate/baseline ordering. CPU is
complete-container cgroup CPU time divided by wall time, expressed as a percent
of one core. These are synthetic rates; the local-pool follow-up is separate.

| Condition | BTC MH/s | BTC CPU, % of one core | Mean actual GT MHz | XMRig H/s | Package W |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original Rust, BTC alone | 190.57 | 1.398 | 1,165 | — | 25.47 |
| Queued Rust, BTC alone | 190.50 | 0.296 | 1,164 | — | 24.94 |
| Four-worker XMRig alone | — | — | — | 1,276.5 | 24.94 |
| Original Rust + XMRig, two-run mean | 78.28 | 1.762 | 491 | 1,199.5 | 24.94 |
| Queued Rust + XMRig, two-run mean | 78.98 | 0.388 | 493 | 1,203.1 | 24.94 |

The two loaded candidate runs were 78.9846 and 78.9785 MH/s; baseline runs were
78.6447 and 77.9067 MH/s. The approximately 0.9% higher mean BTC rate is small
relative to uncontrolled host/clock variation and two repeats do not establish
a general speedup. The clear improvement is 78.0% less BTC process CPU, with
both miners retaining comparable throughput. Standalone CPU fell 78.8% while
BTC rate differed by less than 0.04%.

Main-thread voluntary context switches averaged 926.9/s before and 92.7/s after
under XMRig, a 90.0% reduction. Standalone values were 903.6/s and 94.8/s.
Measured queued GPU execution spans covered approximately 99.98% of synthetic
wall time. That describes timestamped dispatch spans, including any GPU stalls;
it is not an independent EU-utilization or silicon-maximum measurement.

The queued miner retained 41.5% of its standalone BTC rate with four XMRig
workers. XMRig retained 94.2% of its standalone rate. Shared package power remains
the principal observed constraint: lowering host wakeups did not restore the
standalone iGPU clock. Package energy includes both workloads; the small
standalone power difference is a single-pair observation, not an established
GPU efficiency improvement. No 350 MH/s capability is established.

XMRig rates use progress-counter differences over the reported covered interval,
about 270 seconds within each five-minute measurement. Raw telemetry and
per-run reports reside in `bench-results/while-xmrig-20261019T0835Z/`.
Run `python3 scripts/analyze-coexist.py` with that directory to regenerate the
CPU, clock, scheduler, RAPL and GPU-span summary.


### Kernel/local-size screens and compiler inspection

Each screen measured at least five minutes with four unchanged XMRig workers.
These are individual shared-desktop runs, not a balanced isolated comparison.
Start-of-run process snapshots show substantial Firefox/extension activity from
`local32` onward; both BTC and XMRig rates fell in some of these intervals.
That workload was left untouched. These data cannot establish a clean tuning
ranking or attribute the observed differences entirely to the shader.

| Candidate variant | BTC MH/s | BTC CPU, % of one core | XMRig H/s |
| --- | ---: | ---: | ---: |
| pre3 / 64, earlier two-run mean | 78.98 | 0.388 | 1,203.1 |
| pre3 / 32 | 68.49 | 0.391 | 1,031.3 |
| pre3 / 128 | 70.82 | 0.381 | 1,052.2 |
| unrolled / 64 | 68.45 | 0.384 | 1,037.0 |
| pruned / 64 | 79.13 | 0.384 | 1,182.3 |

After all timed workloads exited, ANV compiler dumps from the final debug image
were captured with `INTEL_DEBUG=cs` and `MESA_SHADER_CACHE_DISABLE=true`.
All five configurations compiled SIMD16 code using 128 GRF registers and zero
spills/fills. The three pre3 local sizes had the same reported 3,960 instructions
and 19,772 estimated cycles. Unrolled and pruned each reported 4,032 instructions
and 19,856 estimated cycles. These are compiler estimates, not measured GPU cycles.
Identical instruction statistics clearly do not explain the different shared-load
rates. Dumps and exact probe commands are under the evidence directory's
`compiler/` subdirectory. The existing pre3/64 default is retained.

Two-nonce interleaving was not added: the current compiler already allocates 128
GRFs per thread, so additional live state requires a separate register-pressure,
correctness and loaded-throughput experiment. No interleaving gain is claimed.

### Full runtime alongside XMRig

The production image completed a 331.1-second local-pool run with four independently
accepted shares, three connections/authorizations, eight clean and eleven nonclean
jobs, no invalid/duplicate/stale submissions and shutdown in 0.48 seconds, exit 0.
HTTP status requests and slow SSE clients remained active. The controller's last active
status sample showed 3,264 retired batches, two still outstanding, zero poll checks and
7.11 ms cumulative prepared-work wall time. This validates use of the new queue
and cache in the actual controller, separately from the synthetic benchmark.

After the existing 20-second startup grace, a 309.49-second sampled interval
completed **78.76 MH/s** with **0.552% of one CPU core**, averaging 491 MHz.
XMRig reported **1,185.8 H/s** over its separately covered 270.02-second progress
interval. This is a candidate-only application-level check under the later shared
desktop load, not a matched old/new full-runtime comparison. The same source and
image defaults were used throughout. All task-owned miners exited afterward.

The Vulkan conformance harness was additionally compiled inside the exact
container build stage. All five embedded SPIR-V modules were byte-identical
between that test build and the production release build. All 24 conformance
checks passed for every shipped kernel with validation enabled in the debug
container; pre3 also passed in legacy polling mode. Evidence is in
`/tmp/gbtc-while-xmrig/container-conformance/`.

The earlier `while-xmrig-20261019T0829Z` attempt was stopped before its first
measured run finished, to fix legacy wait compatibility; it contributes no rate
or acceptance result. The final input manifest SHA-256 is
`d4933c39363710386e9318e7c8599850e52888fd2eec88976d4be8be507c8634`.
Per-stage timings are wall times; individual stage CPU attribution is not
provided. Other hardware, driver versions and XMRig configurations remain untested.
