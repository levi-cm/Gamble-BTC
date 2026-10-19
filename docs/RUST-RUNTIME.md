# Rust iGPU candidate

The Rust workspace provides a complete service independent of `src/`: Vulkan
compute, Rust OpenCL compatibility, Stratum, work construction, HTTP/status/SSE
and a wallet-free benchmark. C and its image remain available as references.
See [QUALIFICATION.md](QUALIFICATION.md) for the measured release state; a working
rewrite alone does not establish a performance win or 350 MH/s.

## Build and run

Host build requires Rust 1.99, Python 3, `glslc`, `spirv-val`, the Vulkan loader
and a supported integrated GPU driver. OpenCL dynamically loads its ICD.

```sh
make test-rust
make rust-release
make rust-container
scripts/start-rust-miner.sh --probe
scripts/start-rust-miner.sh --bench
```

The container build copies only `rust/` into the compiler stage. It does not
compile or link the reference C sources. Production runs as uid 1000 with the
render node's group added by the launcher. Docker supplies ANV/Mesa from Debian
backports and pinned Intel NEO/IGC packages; host drivers are not modified.
The separate `debug` build target adds Vulkan validation layers and tools.

For real mining, create `.env` from `rust.env.example`, set your address and pool,
then run `scripts/start-rust-miner.sh`. The launcher preserves the AC-power
check; `GBTC_ALLOW_BATTERY=1` explicitly opts into battery operation. The launcher uses host networking and
binds the status listener to `127.0.0.1:41176` by default. Stop with Ctrl+C or
`docker stop --time 6 gbtc-rust-while-xmrig`. The original C container is not stopped
or replaced by this launcher. Run only one miner during performance tests.

For Compose installations, `docker compose -f compose.rust.yaml build` and
`docker compose -f compose.rust.yaml up -d` use a separate service, container,
image and loopback port. The bridge workflow needs working host veth support.
This laptop currently requires the direct launcher because bridge creation fails.

## Configuration and migration

- `GBTC_BACKEND=vulkan|opencl|auto`: explicit choices fail if unavailable. `auto`
  tries Vulkan then OpenCL and logs/reports its fallback reason.
- `GBTC_DEVICE` must identify the actual integrated GPU. Vulkan matches DRM
  render identity; OpenCL uses sysfs PCI identity and unified-memory GPU selection.
  Neither backend admits a CPU mining device or silently selects a discrete GPU.
- `GBTC_VULKAN_KERNEL=looped|unrolled|pre3|pruned|boolean` selects embedded SPIR-V.
  `pre3` is the candidate default and `looped` is the original reference. `pre3`
  moves three nonce-independent
  compression rounds to the host once per batch.
- Vulkan local size accepts 8/16/32/64/128/256; OpenCL also accepts 512 and `auto`.
  Poll intervals accept 100–50000 microseconds and sleep while awaiting completion.
- OpenCL preserves `unrolled|looped|dual|pre3`, SIMD `auto|8|16|32` and optional
  `GBTC_OPENCL_PLATFORM`. SIMD support is driver-dependent.
- `GBTC_BATCH_NONCES` bounds work per dispatch/control iteration. Larger batches
  trade job-change latency for fewer queue submissions; nonce ranges never wrap.
- `GBTC_PROBE_ONLY=1` and `GBTC_BENCH_ONLY=1` require no wallet and open neither
  pool nor HTTP sockets. They are mutually exclusive and print JSON results.
- Nonempty `GBTC_GLES_*` values and `GBTC_BACKEND=gles` produce migration errors.
  The candidate launcher/Compose explicitly clear inherited GLES settings.
- `GBTC_HTTP_BIND` is an IPv4 bind address and `GBTC_HTTP_PORT` defaults 41174
  inside the binary. The direct launcher defaults 41176 to avoid the C UI.
- Plain TCP Stratum is supported. TLS URLs and `mining.set_extranonce` are
  rejected explicitly. Pool DNS resolution uses the system resolver; shutdown
  watchdog bounds a resolver/driver that fails to return after a stop request.

`/`, `/index`, `/index.html`, `/status.json`, `/events`, `/favicon.ico` and OPTIONS
preserve the C UI routes. Status retains the original fields and 300-point history,
adding local `discarded` and `unresolved` counters. `submitted` means written to
the pool socket; `accepted`/`rejected` require matched pool replies. HTTP clients
are bounded to 16, with deadlines for partial headers and socket writes. Use this
unauthenticated endpoint on a trusted interface.

## Verification and reproducible measurements

```sh
make test-rust-gpu
python3 scripts/qualify-rust.py --image gamble-btc:rust-while-xmrig --seconds 1800
python3 scripts/bench-rust.py --kernel pre3 --warmup 30 --seconds 60 \
  --output bench-results/my-pre3-run
```

GPU tests require the real integrated device and compare complete candidate sets
against independent SHA-256d, including overflow and range boundaries. The local
qualification server independently reconstructs submitted 80-byte headers, changes
jobs/difficulty, forces reconnects, queries status, and keeps slow SSE readers.
It does not send a wallet or contact a public pool.

The measurement tool resolves an immutable Docker image ID, records wall hashes,
process cgroup CPU time and readable GT clock/PL1/AC telemetry. It runs containers
serially with no CPU cap. Missing telemetry is null. Screening results are not
paired performance qualification. Do not compile during quiet measurements.
Keep real unrestricted compilation trials and build-alone controls separate.

For validation, build the debug image and pass it plus `--validation`:

```sh
docker build --network host --target debug -f Dockerfile.rust -t gamble-btc:rust-while-xmrig-debug .
python3 scripts/qualify-rust.py --image gamble-btc:rust-while-xmrig-debug --validation --seconds 150 --require-shares
```

The final production image does not include validation layers.

## Architecture and failure behavior

`gbtc-stratum` owns the pool socket and bounded event/share channels. The controller
owns immutable connection/job/target snapshots, nonce/extranonce cursors and a
bounded retained-job registry. It drains events before and after each batch, checks
full 256-bit hashes on the host and rejects stale, duplicate or out-of-range results.
Host hashes are correctness checks and work construction, never CPU nonce search.

`gbtc-backend-vulkan` owns a single fenced output slot, explicit transfer/compute/host
barriers and noncoherent cache invalidation. OpenCL chains nonblocking transfers,
kernel and read events, retaining host storage until queue drain even after errors.
Output overflow recursively subdivides and counts covered nonces once. Cancelled
or uncertain GPU completion poisons the backend, preventing reuse; bounded teardown
or the process shutdown watchdog terminates a driver that cannot drain.

`gbtc-http` owns a bounded set of client threads and one sampler. Socket writes
release the shared status lock. Signal handlers only set a stop flag; normal exit
joins services and drops GPU resources. A shutdown-only 5 second watchdog is the
last resort for a blocked worker or driver.

## Cutover and rollback

Keep `gamble-btc:latest` unchanged until qualification is reviewed. To try the
candidate, stop the reference through its existing launcher first, then use the
separate Rust launcher. To roll back, stop `gbtc-rust-while-xmrig` and use the
existing C launcher/image. Do not run `docker compose down` against the other
service's configuration or retag the candidate as `latest` during evaluation.

For queued dispatch, event-driven waits and four-worker XMRig coexistence, see
[WHILE-XMRIG.md](WHILE-XMRIG.md). Previous reference image IDs remain recorded in
[QUALIFICATION.md](QUALIFICATION.md).
