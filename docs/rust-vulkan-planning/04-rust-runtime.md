# Part 4 — Full Rust Host Runtime and OpenCL Parity

Execute T04 and T08–T12 from Part 10. Preserve useful behavior; fix proven C bugs
with explicit regression cases rather than reproducing them as compatibility.
No production crate may invoke the old C miner or build it with a C compiler.

## T04: pure core first

Inspect `src/sha256.*`, `src/bench.*`, `src/stratum_protocol.*`, and work-building
functions `compute_merkle_root` / `build_header_midstate` in `src/main.c`.
Implement Rust wrapping-u32 compression, byte-level SHA-256d, exact target compare,
header serialization, coinbase/merkle construction and batch types from Part 9.
Use an independent SHA library and known Bitcoin header vectors as the oracle.
Do not derive every expected output from the port being tested.

Tests: empty/short/multi-block SHA, SHA-256d, header midstate vs full header hash,
coinbase extranonce widths, zero/multiple merkle branches, target equality and
neighbor values, invalid compact/difficulty inputs, nonce exhaustion and byte
orders. Use seeded randomized equivalence tests against independent full hashes.

## T08: configuration and Rust OpenCL backend

Implement one configuration parser in `gbtc-miner/src/config.rs`; keep shell and
Compose settings distinct from application settings. Complete Part 9's inventory
before claiming compatibility. Test defaults, empty values, malformed numeric
input, overflow, backend-specific validation and no pool access in probe/bench.

Port `src/opencl.c` behavior through Rust OpenCL bindings. Keep current kernel
variants `unrolled`, `looped`, `dual`, `pre3`, platform preference, local size,
SIMD and sleep-poll controls. Reuse validated device kernel semantics; preserve
the original midstate in `pre3` feed-forward. Do not redo prefix precomputation
as if it did not exist. The Rust OpenCL backend implements the same complete
candidate/overflow contract as Vulkan. Use event dependencies, flush, sleep-poll,
bounded waits and RAII cleanup. Never silently reintroduce one-core blocking spin.
Run shared complete-set conformance on the intended Intel NEO ICD. Rusticl support
requires its own evidence. A temporary C adapter is permitted only before G3 and
must be removed from production linkage before that gate.

## T09: protocol parser, then mock TCP client

Implement bounded newline-delimited JSON-RPC messages and explicit states:
Disconnected -> Connecting -> Subscribing -> Authorizing -> Ready -> Backoff.
Accept notifications interleaved with replies. Correlate replies by ID; handle
error objects, false authorization, unrelated/duplicate IDs, partial TCP frames,
multiple messages/read, invalid JSON, overlong lines and EOF.

Preserve current limits unless a tested reason changes them: extranonce1 <=16
bytes, extranonce2 <=8 bytes, job ID <=63 bytes, coinbase parts <=511 bytes each,
merkle branches <=32. Reject invalid hex/lengths before allocation/copy. Never
cast an unbounded pool value to a buffer index or counter.

Implement `mining.subscribe`, `mining.authorize`, `mining.notify`,
`mining.set_difficulty`, and `mining.submit`. Difficulty must be positive and
finite; use the exact supported conversion semantics backed by C fixtures and
independent target vectors. Preserve large/small/fractional cases without
silently rounding through a new representation. Treat `set_extranonce` as an
explicit tested implementation or documented unsupported method, never a silent
partial update. New protocol extensions are outside scope.

Use immutable job snapshots with connection, job and target generations. Record
job ID, extranonce2, ntime, target and the bytes actually hashed. Apply difficulty
changes to subsequent work; never recheck old candidates under a mutable current
target. `clean_jobs=true` invalidates earlier jobs. `false` permits still-valid
older jobs within a bounded registry; use the newest for new batches. Reconnect
invalidates all work and pending replies from the previous connection. Distinguish
new-work cancellation from whether an old candidate remains eligible to submit.

Bound pending submissions (initially 64), submission queue, job registry, line
size and connect/read/write timeouts. On congestion, apply backpressure or an
explicit counted failure; never silently discard a found valid share. Backoff
must sleep and be interruptible. Accepted/rejected counters follow matched submit
replies exactly once. Do not log wallet, credentials, full pool messages or
unsanitized job data. Tests use synthetic identities.

Mock-pool tests: handshake with early notifications, fragmented reads, malformed
and oversized messages, target change during a batch, clean/non-clean jobs,
reordered submit replies, rejection, pending limit, disconnect/reconnect,
timeout and shutdown. Assert exact submitted hex fields and generation rules.
No public pool or real wallet is needed for these tests.

## T10: HTTP/status and existing UI

Extract existing HTML/JS into `gbtc-http/assets/index.html`; preserve the layout,
chart, labels and escaping behavior. Replace C string-literal tests with runtime
HTTP/JSON/SSE tests. Text presence is insufficient to prove browser behavior.
Part 9 lists routes and fields. Preserve units, history ordering, initial history
and incremental SSE behavior after inspecting `sse_client_thread`.

Use bounded history and broadcast capacity, bounded clients and header/request
limits, slow-client disconnect/backpressure and no per-client busy polling.
Publish immutable snapshots; never hold the mining lock over socket writes.
Serialize untrusted strings with JSON/HTML safety and render via `textContent`.
Preserve loopback direct binding; container binding must work behind explicit
loopback host publication. Test HEAD/OPTIONS/404/405 and disconnected SSE clients.
Capture the candidate UI with synthetic mock-pool data and check chart updates,
initial waiting state and reconnection. Save a sanitized screenshot if available.

## T11–T12: controller, benchmark, lifecycle

One GPU worker owns its backend. Async networking/HTTP must not block on GPU waits;
use a dedicated blocking worker plus bounded messages. Begin with one in-flight
batch. Controller uses a u64 nonce cursor up to 2^32, splits ranges safely, advances
extranonce only after the range is exhausted or work changes, and rejects
extranonce wrap/repeated work. ntime rolling is not invented without pool rules.

Snapshot generation state before dispatch and validate submit eligibility after
completion. Cancel new dispatches promptly on clean jobs/difficulty changes or
shutdown; never reuse an in-flight buffer. No work before successful authorization
and a valid job/target. Backend errors produce explicit bounded recovery or a
nonzero exit, no tight retry loops or hidden CPU mining fallback.

Benchmark mode uses deterministic synthetic work, monotonic timing, warmup outside
the measured interval, exact successful unique nonce counts and structured JSON
with backend/image/config metadata. No wallet, DNS/pool connection or live miner
is needed. Runtime auto policy prefers a usable integrated Vulkan device then
Rust OpenCL, with a logged sanitized reason; explicit requests fail on errors.
Default cutover remains subject to G5.

SIGINT/SIGTERM: stop dispatch, cancel reconnect, close listeners/SSE, resolve or
abort queues with an explicit count, wait for safe GPU completion, release
resources and exit within a documented timeout. Test no-job idle, active work,
blocked network, GPU timeout and repeated start/stop. A fake backend tests races
and bookkeeping without requiring a GPU.

Done: Rust-only host binary, both backend implementations, passing pure/mock
contracts, actual-GPU conformance, UI and lifecycle evidence. CPU and throughput
qualification still requires Part 5.
