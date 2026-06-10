# Architecture

Gamble BTC is built as one iGPU-only C runtime binary plus a Docker runtime
wrapper. The source has started moving toward backend boundaries while
preserving the original HD 4600 GLES path.

## Runtime Components

- `src/main.c`: contains Stratum client logic, work construction, the GLES iGPU
  backend, generated compute shader source, mining loop, and the local HTTP
  status UI.
- `src/sha256.*`: reusable SHA-256/SHA-256d helpers used by deterministic tests
  and future work-construction cleanup.
- `src/backend.*`: shared backend types, backend-name parsing, and unavailable
  placeholders for future Vulkan/OpenCL iGPU backends.
- `src/gles_tuning.*`: GLES kernel/local-size parsing and shader-source
  generation for the benchmarkable shader variants.
- `src/bench.*`: benchmark configuration parsing and deterministic synthetic
  work generation for no-Stratum performance runs.
- `Makefile`: builds the runtime binary and host-side deterministic tests.
- `Dockerfile`: builds the C binary in a Debian build stage and copies it into a
  smaller runtime image with Mesa and cJSON runtime libraries.
- `compose.yaml`: wires the image to the original host's DRM devices, group IDs,
  backend environment controls, restart policy, memory limit, and HTTP status
  port.

## Data Flow

1. Environment variables provide the BTC address, pool URL, and worker name.
2. The miner connects to the Stratum endpoint and subscribes/authorizes.
3. Stratum jobs are converted into block-header work.
4. Host-side helper code prepares midstate and merkle data.
5. Backend selection probes `vulkan`, `gles`, then `opencl`; only GLES is
   implemented in the current code.
6. A selected GLES compute shader variant searches nonce batches on the iGPU.
7. Matching nonces are submitted back to the pool.
8. Runtime stats, backend, and device fields are exposed through `/status.json`
   and the SSE-backed local UI on port `41174`.

When `GBTC_BENCH_ONLY=1`, startup stops after backend initialization and runs
fixed synthetic work through `backend->run_batch`; it prints JSONL benchmark
summaries and never opens Stratum or the HTTP status server.

## Current Assumptions

- Linux host with accessible `/dev/dri` devices.
- Mesa driver stack supports surfaceless EGL and GLES 3.2 compute shaders.
- The current Docker/Compose files target the original Intel HD 4600 setup.
- The HTTP endpoint is intended for trusted local access only.
- CPU mining backends are intentionally not implemented. The project goal is to
  keep nonce search on an integrated GPU with minimal CPU load.

## Portability Direction

Future multi-iGPU support should move the remaining Stratum/work/HTTP sections
out of `src/main.c`, add real Vulkan compute, keep GLES as the compatibility
backend, and record verified hardware in `docs/HARDWARE_MATRIX.md`. The current
host-specific HD 4600 path should remain available as a known-good baseline
while new targets are added.
