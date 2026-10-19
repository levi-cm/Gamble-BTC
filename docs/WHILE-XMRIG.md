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
Stratum, reconnects, HTTP and SSE for 330 seconds, with the first 30 excluded from
coexistence analysis. A real wallet and public pool are unnecessary.

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

Sustained results are recorded after the runs complete. Lower host CPU use does not remove
the shared package-power constraint or establish a 350 MH/s capability.
