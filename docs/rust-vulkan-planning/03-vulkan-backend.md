# Part 3 — Correct Vulkan Compute Backend

Execute T05–T07 sequentially. Start with one unrolled shader, one queue, one
in-flight batch. Optimization variants and pipelining come after conformance.
Use the shared semantic contract and byte-order fixtures in Part 9.

## T05: device and resource ownership

Inspect `src/backend.h`, `src/opencl.c`, `tests/test_opencl_conformance.c` and
`rust/gbtc-core/src/backend.rs`. Implement `gbtc-backend-vulkan` without changing
Stratum or HTTP. Core must not import Vulkan handles.

- [ ] Load Vulkan dynamically and enumerate physical devices. Reject CPU Vulkan
  implementations such as lavapipe; no CPU fallback mining is allowed.
- [ ] Honor explicit `GBTC_DEVICE`. On Linux, use physical-device DRM properties
  when exposed, or a verified PCI/device mapping. Fail with a useful reason if
  the requested render node cannot be matched. Never select by an `Iris` string
  alone. Default policy selects an integrated GPU; log stable identity.
- [ ] Require a compute queue and Vulkan 1.1 baseline features actually used.
  One ordinary compute-capable queue is sufficient; a dedicated queue is optional.
- [ ] Query limits: workgroup sizes/invocations/counts, storage buffers, push
  constants, memory types and non-coherent atom alignment. Fail before allocation
  on unsupported configuration. Do not require int64 or subgroup operations in
  the baseline shader.
- [ ] Report device/API/driver properties, and unsupported capability reasons.
  Subgroup-size control is optional; enable only after feature/size checks.
- [ ] Own instance/device/queue/pool/buffers/pipeline/fence with explicit cleanup.
  Clean partial initialization in reverse order. Document each unsafe block's
  pointer lifetime, size, alignment and synchronization obligations.

Check: probe on selected Iris Xe identifies the intended render device; no pool
connection, HTTP listener or mining dispatch in probe mode. Missing loader/device
is an actionable failure for an explicit probe, not a success message.

## T06: simple shader and synchronized batch

Input shader data uses an explicit std430-compatible layout. Choose scalar u32
arrays for midstate/tail/target to avoid vec3 alignment traps. Assert Rust field
offsets and lengths against a documented layout. Push constants carry nonce base
and count. Use wrapping u32 SHA arithmetic and exact padding/byte order fixtures.
The dispatch uses ceil(count/local_size), with bounds guard before nonce addition.
Validate local size and dispatch group limits. A partial final group is allowed.

Output contains a candidate counter, fixed-capacity candidate array and explicit
overflow detection (counter > capacity or a separate flag). Capacity is bounded
and documented; the old capacity 15 is a historical ABI, not permission to lose
shares. Clamp host reads to capacity and validate returned nonce ranges.

Required single-slot ordering:

1. Wait for prior slot completion before reusing its data or fence.
2. Write job input; flush aligned non-coherent mapped ranges when required.
3. Reset output using ordered transfer fill (or a completed host write).
4. Record transfer-write -> compute-read/write dependency for the reset. For
   host-written inputs, establish host-write visibility to shader reads.
5. Bind pipeline/descriptors, push batch arguments and dispatch.
6. Record shader-write -> host-read dependency, or shader-write -> transfer-read,
   copy to staging, then transfer-write -> host-read when using staging buffers.
7. Submit with a fence. Sleep-poll fence status initially (default ~1000 us),
   bounded by shutdown checks and a finite diagnostic timeout. A proven low-CPU
   blocking wait is optional after measurement.
8. Only after fence completion, invalidate aligned non-coherent read ranges and
   read the output. Coherent memory still requires completion ordering.

Never poll a shader counter while the GPU may be writing it. A mapped pointer,
volatile read or atomic shader increment does not replace Vulkan synchronization.
Do not call `vkDeviceWaitIdle` every batch. It may be used during orderly teardown
with a bounded failure policy. No reuse/free/reset of pending resources.

CPU recheck every candidate with the full 256-bit share target. On overflow,
subdivide and rescan the same immutable job/range until complete; at a one-nonce
range completion is finite. Use the GPU for rescans. Deduplicate submissions and
count unique nonce coverage once in performance totals; record extra GPU work
separately. Never report truncated results as a successful complete batch.

## T07: GPU conformance, then benchmark

Implement `tests/conformance.rs` and a shared case generator in core tests/test
support. Require an independent CPU oracle for small finite ranges; host scans
are correctness tests only. Compare sorted COMPLETE candidate sets when the
buffer can hold all matches; test overflow recovery separately.

Cases:

- Known 80-byte header vector and digest; random seeded headers/midstates/tails.
- Nonzero bases, count 1, count below local size, non-multiple count, zero count
  policy, and final nonce `0xffffffff` without wrap. Reject range end >2^32.
- Target equality, word endianness, no-match fixture verified by the oracle,
  sparse matches and all-match sets. Zero target alone is not a mathematical
  guarantee of no match for arbitrary data.
- Candidate prefilter equality plus false positives that the full target rejects.
- Counter reset between batches; repeat dispatches; forced capacity overflow;
  complete rescan union; no duplicate nonces; correct unique hash accounting.
- Job/difficulty generation changes and cancellation while a batch is pending.
- Unsupported local sizes/features, wrong device, failed shader/pipeline creation,
  buffer mapping errors, timeout/device loss and partial-init cleanup.

Use `GBTC_REQUIRE_GPU_TESTS=1` for mandatory device checks (new test-only flag).
Without it an absent GPU may produce an explicit skip; in required mode absence,
wrong device or skip fails. On a present supported device, shader/sync errors are
failures. Run validation layers in a separate debug invocation and report zero
validation errors. Disable layers for performance runs.

Run tests for the baseline local size first; extend to supported sizes
8/16/32/64/128/256 only after correctness. Initial baseline uses no optional
subgroup controls. Never infer native ISA from SPIR-V alone or use IGC dumps as
ANV machine-code evidence.

Done: real Vulkan dispatch, complete candidate semantics, validated ownership and
synchronization, actual-device conformance evidence. G2 is pending if skipped.
