# Part 9 — Shared Contracts and Compatibility Inventory

T01 produces `docs/RUST-COMPATIBILITY.md` from source and tests. This file defines
the rewrite's common boundaries; it is not a claim that all behavior is inventoried.
Workers must read it before defining independent copies of shared types.

## Core semantic types

Define these in `rust/gbtc-core/src/backend.rs` and `work.rs` before GPU/runtime
workers start. Use concrete simple types, not a framework or async backend trait.

```rust
pub struct WorkBatch {
    pub midstate: [u32; 8],
    pub tail3: [u32; 3],
    pub target: [u32; 8],
    pub nonce_base: u32,
    pub nonce_count: u32,
}
pub struct BatchResult {
    pub candidates: Vec<u32>,
    pub unique_hashes: u64,
    pub overflow_rescans: u32,
}
pub trait ComputeBackend {
    fn run_batch(&mut self, work: &WorkBatch) -> Result<BatchResult, BackendError>;
}
```

`BackendError` is a project error enum defined in the same module with explicit
invalid-input, unavailable, timeout, device-lost and driver-failure cases and a
sanitized diagnostic. Initialization/probe constructors stay backend-specific.
Use a shared `BackendInfo` struct for selected backend/API/device/driver strings.
`run_batch` returns COMPLETE prefilter candidates, including overflow recovery,
or an error. Internal raw GPU output can differ from this public semantic result.
Bound all internal capacity and reject impossible/oversized requests; normal
large batches must not allocate one host element per tested nonce.

`nonce_count` >0; validate `u64(base)+u64(count) <= 1<<32`. No wrapping range.
A whole nonce space is multiple batches, with u64 cursor and u32 batch counts.
GPU tail invocations outside count do no work. `unique_hashes` counts covered
nonces once; diagnostic raw GPU/rescan counters stay separate.

Keep immutable application `JobSnapshot` separate from GPU `WorkBatch`. It owns
connection/job/target generations, job ID, extranonce2 bytes, ntime, target,
header bytes and any validity state needed for submission. Candidates are never
paired with mutable “current job” fields after dispatch.

## Byte order and target rules

The C `target[0]` is the most-significant comparison limb. Its GPU prefilter uses
`bswap32` of the final SHA output word7. Do not interpret it as SHA state word0.
Prove the mapping using `gbtc_target_to_raw_hash`, `gbtc_raw_hash_meets_target`,
`gbtc_work_hash` and `tests/test_stratum_protocol.c` before freezing Rust fixtures.

Create a table in RUST-COMPATIBILITY with each field's wire hex, serialized header
bytes, CPU u32 meaning, GPU u32 meaning and submit hex meaning. Include version,
prevhash, merkle root, ntime, nbits, nonce, midstate and all target limbs.
Use an independent byte-level SHA-256d on the original 80-byte header. Compare
full hash interpreted by Bitcoin target rules; equality is a match. A one-word
GPU prefilter is necessary but not sufficient. Never guess byte order from a
variable suffix such as `_be` or a display hash string.

## Configuration ownership and disposition

Inspect all `getenv`, helper parsing, script defaults and Compose interpolation,
not just README lines. `rg` matches constants too: distinguish actual settings
from enums/header guards. Inventory columns: name, owner, default, accepted input,
invalid behavior, Rust disposition, regression test and source reference.

| Family | Required disposition |
| --- | --- |
| BTC_ADDRESS, POOL_URL, WORKER_NAME | Preserve mining meanings and defaults; synthetic test data only |
| GBTC_BACKEND | Preserve auto/opencl; implement vulkan; explicit gles errors with migration instructions |
| GBTC_PROBE_ONLY, GBTC_BENCH_ONLY | Preserve no-pool mode and meaningful exit status |
| GBTC_DEVICE | Honor selected render device, never silently pick another |
| GBTC_BATCH_NONCES | Preserve validated public range/quantum unless a documented migration improves it |
| GBTC_BENCH_SECONDS, GBTC_BENCH_WARMUP_SECONDS | Preserve units/default/error behavior |
| GBTC_HTTP_BIND | Preserve direct loopback default and explicit bind parsing |
| GBTC_OPENCL_KERNEL/LOCAL_SIZE/SIMD/POLL_US/PLATFORM/DEBUG | Preserve via Rust OpenCL with source-derived accepted values |
| GBTC_GLES_KERNEL/LOCAL_SIZE/AUTOTUNE | Retired GLES-only controls: actionable migration error when active; remove injected defaults from Rust launcher/Compose |
| GBTC_DRI_CARD/DRI_RENDER, VIDEO_GID/RENDER_GID | Compose ownership and device mapping, preserve actual current names |
| GBTC_VIDEO_GID/RENDER_GID, GBTC_IMAGE/CONTAINER_NAME/ENV_FILE | Script ownership; reconcile with Compose aliases explicitly |
| GBTC_STATUS_BIND/STATUS_URL/STATUS_INTERVAL | Inspect README vs actual scripts/Compose and record supported behavior |
| GBTC_ALLOW_BATTERY, GBTC_SHADER_CACHE | Preserve launcher guard/cache path behavior |
| GBTC_IGC_FORCE_SIMD / IGC_ForceOCLSIMDWidth | Preserve script-to-driver mapping; no shader feature assumption |
| MESA_*, RUSTICL_ENABLE | Keep only applicable backend/container meanings; `iris` GL override is not a Vulkan ICD selector |

New additive settings: `GBTC_VULKAN_LOCAL_SIZE` (initial 64 if supported),
`GBTC_VULKAN_POLL_US` (initial 1000, positive bounded sleep), and
`GBTC_VULKAN_VALIDATION` (debug-only opt-in; fail clearly if requested but absent).
Freeze accepted ranges and failure cases in T05/T06 and config tests.
`GBTC_REQUIRE_GPU_TESTS=1` is test-runner-only and forbids required-test skips.
Do not rename existing settings merely to make them uniform.

GLES retirement is a declared compatibility change, not “all variables keep their
meaning.” Remove obsolete GLES defaults from candidate Compose/scripts before
running the Rust binary, and provide a short old-config -> new-config example.
OpenCL controls may remain configured while Vulkan is selected; validate their
values when that backend is actually selected, according to inventory decisions.

## HTTP/status schema

Source of behavior: `build_stats_json`, `sse_client_thread`, `http_thread`, and
`index_html` in `src/main.c`; `tests/test_ui_contract.c` checks only source text.
Port actual response behavior, not C code substrings.

Routes on process port 41174: GET/HEAD `/` and `/status.json`, GET `/events`,
favicon empty response, OPTIONS, malformed-request 400, unknown 404 and method
405. Inspect current path/query matching, headers, SSE event/data framing,
history initialization, cadence and connection close behavior; put fixtures in
`rust/gbtc-http/tests/http_contract.rs`. Preserve existing public numeric types
and units; never emit NaN/Infinity as invalid JSON.

Required status keys from current serialization:

```text
mh min max avg samples uptime submitted accepted rejected lifetime_hashes
 diff job ntime nbits merkle_branches pool worker addr
 backend backend_api device_path device_vendor device_name driver_name fallback_reason
 net_diff net_hashrate eta_block_s prob_block_per_day prob_block_24h history
```

`history` presence follows the current initial/incremental rules; inspect them.
Test synthetic strings with quotes, control characters and script-like text.
Stats remain finite with zero hashrate/no job. Browser updates use `textContent`.
Real status contains private fields: integration fixtures must be synthetic.

## Contract test owners

| Boundary | Owner/task |
| --- | --- |
| Bytes/hash/targets/ranges/shared types | Core T04 |
| Complete candidates and synchronization | Vulkan T06/T07, OpenCL T08 |
| Environment and backend selection | Config T08, controller T11 |
| TCP framing/IDs/jobs/difficulty/submission | Stratum T09 |
| HTTP schema/SSE/UI behavior | HTTP T10 |
| Race, cancellation, counters, signal handling | Controller T11/T12 |
| Script/device/env/image and port wiring | Packaging T13 |

If a contract must change, orchestrator updates this file, inventory, dependent
workers and tests together. Do not let workers independently invent interfaces.
