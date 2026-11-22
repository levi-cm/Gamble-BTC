# Part 0 — Scope, Evidence, and Acceptance

## Full rewrite means

- Rust owns configuration, work generation, Stratum networking, candidate checking,
  scheduling, statistics, HTTP/SSE, benchmark mode, and shutdown.
- Vulkan owns GPU SHA-256d nonce search through Rust `ash` bindings and SPIR-V.
- A Rust OpenCL backend preserves the existing fast path and its configuration.
  Calling the vendor OpenCL library through Rust FFI is allowed. Calling the old
  C miner or compiling `src/opencl.c` into the production binary is transitional
  only and cannot satisfy G3.
- GLSL compute and OpenCL C device kernels are allowed GPU languages. “Rust host”
  does not mean rewriting vendor drivers, GLSL compilers, or GPU kernels in Rust.
- Existing shell launch/benchmark helpers remain supported and are updated as
  needed. Build infrastructure and web assets need not become Rust.
- GLES is replaced by Vulkan, with explicit errors and migration instructions for
  GLES-only settings. No silent backend substitution for an explicit selection.

## Current source facts (reviewed 2026-10-17)

| Area | Source to inspect |
| --- | --- |
| Main runtime, work construction, UI, HTTP, mining | `src/main.c` |
| SHA-256 helper and backend types | `src/sha256.*`, `src/backend.*` |
| OpenCL, including existing `pre3` implementation | `src/opencl.c`, `src/opencl.h` |
| Protocol, bench, HTTP config | `src/stratum_protocol.*`, `src/bench.*`, `src/http_config.*` |
| Current tests | `tests/`, `Makefile` |
| Actual container defaults | `Dockerfile`, `compose.yaml`, `scripts/start-miner.sh` |

`AGENTS.md` and README contain historical hardware/default descriptions. The
current Compose source maps card1/renderD128, uses configurable video/render GIDs
with defaults 983/987, and defaults to OpenCL. Runtime without an explicit backend
uses `auto`. Resolve disagreement through source inspection and tests; record it.

## Hardware and historical evidence

Target: Huawei KLVDZ-WXX9, i5-1135G7, Iris Xe Tiger Lake Xe-LP, 16 GiB RAM.
Historical notes identify PCI 8086:9a49 and approximately 80 EUs, nominal maximum
GT clock 1300 MHz, shared package power constraints, i915 and Mesa/ANV support.
These are target context, not a fresh hardware probe. Capture actual host and
container driver versions separately; containers use their own userspace ICDs.
Passing `/dev/dri` does not make a container use the host Mesa userspace.

`docs/HARDWARE_MATRIX.md` records:

- OpenCL/NEO: 184.022 MH/s at 1.78% container CPU in a historical sustained run.
- 186.634–193.994 MH/s full-clock spot samples, not a repeatable A/B median.
- GLES: approximately 104–126 MH/s across different historical stacks.
- Prior SIMD32, looped/dual, pruning and scheduling experiments with limited or
  negative results. `pre3` now exists in source; inspect its evidence before work.

Never call 194 MH/s a measured sustained release baseline. Reproduce the current
baseline under matched conditions. NIR sharing between ANV and GLES does not
establish identical code generation or predict a Vulkan performance number.
Do not assume XeHP/Arc instructions or subgroup sizes are available on Xe-LP.

## Acceptance, in order

1. Correct GPU nonce search: complete candidate sets, no silent overflow loss,
   independent host oracle and real-device conformance.
2. Correct Rust application: no production dependency on the old C host runtime;
   preserved protocol, HTTP/UI, configuration and launch behavior, except the
   documented GLES retirement.
3. Low host overhead: target <=0.02 logical CPU core-equivalents in warmed steady
   mining (2% of one logical CPU), with CPU accounting defined in Part 5.
4. Usable Vulkan implementation and Rust OpenCL fallback. Vulkan is the intended
   primary rewritten backend. If it loses materially to the matched OpenCL
   baseline, document the loss and keep production default cutover pending.
5. Best measured sustained throughput; 400 MH/s stays an unproven stretch goal.
6. Measure mining plus a real background build and build alone. Do not constrain
   compilation to make mining results look better. Explain shared power limits.

## Boundaries

No CPU mining, host power/clock changes, driver replacement, live deployment,
Git publication, new pool protocols, or unrelated UI redesign in this plan.
No exact-hardware claim without live verification. No guaranteed zero slowdown
under compilation: measure both mining loss and compilation overhead honestly.
A microbenchmark measures one workload, not an absolute SHA-256d upper bound.
