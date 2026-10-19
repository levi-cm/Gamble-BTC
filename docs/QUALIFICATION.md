# Rust candidate qualification — 2026-10-18

The full Rust runtime, Vulkan compute backend and Rust OpenCL fallback are
implemented and functionally verified. This candidate remains separate from the
C reference. Performance qualification and the extended 30-minute soak remain
deferred at the user's request while another agent runs an important workload.
That workload was left untouched. No 350 MH/s result or speedup over C is claimed.

## Reproducible build identity

Both images were built from an isolated context containing only `Dockerfile.rust`
and the Rust inputs, without the C `src/` directory.

| Image | Immutable local image ID |
| --- | --- |
| `gamble-btc:rust-vulkan-candidate` | `sha256:efd5ae39648051b28b55d8343d5a1c5859943fc52ada08cda4196350ee368817` |
| `gamble-btc:rust-debug` | `sha256:636fba369c22cc3ca2b391cb77813ee64fdf7278b9e7b53894ceeb7bd15abf0e` |
| Preserved C `gamble-btc:latest` | `sha256:24805ee252831efe2e4a4923bd3a636f9dc0b0b6b2fbb27d541f997b8bb645b2` |

The local input manifest is
`/tmp/gbtc-rewrite-20261018/final-source-manifest.txt`; its SHA-256 is
`d289363ce945b79e2526700a21766203bfd821b9bf2221019331455915366952`.
The manifest matches the final source inputs. Temporary logs and ignored
`bench-results/` are local evidence and are not committed release artifacts.

## Tested stack

- Huawei KLVDZ-WXX9, Intel i5-1135G7, Iris Xe Tiger Lake `8086:9a49`, 16 GiB RAM.
- CachyOS Linux `7.2.6-1-cachyos`, host Mesa `26.2.4-arch3.1`, Docker `29.9.0`.
- `/dev/dri/renderD128`, render group 987; host card `/dev/dri/card1`, group 983.
- Rust 1.99.0; Debian trixie container, Mesa ANV 26.1.6,
  NEO 26.35.39758.10 and IGC 2.41.5. AC power online.

Docker bridge creation fails on this host with `veth operation unsupported`.
Verification used network-none probes and host-network local-pool tests without
changing host configuration. Compose syntax was checked with a temporary
Compose v2.40.3 binary; the bridge-based service was not deployed.

## Functional results

`make test-rust` passed all 127 tests, formatting and workspace/all-target Clippy
with warnings denied. The preserved C `make test` suite and benchmark script
contracts also passed.

The final production image also passed fallback probes: requesting unavailable
validation layers with `GBTC_BACKEND=auto` selected OpenCL and reported the
reason; explicit `GBTC_BACKEND=vulkan` exited with an error instead of silently
changing backends. Launcher shell checks, Compose configuration, Python syntax
and the source manifest check passed.

In the debug container, each of the five Vulkan variants (`looped`, `unrolled`,
`pre3`, `pruned`, `boolean`) passed all 17 GPU conformance tests with validation
enabled. Four Vulkan unit tests, including staged initialization failures, and
all eight OpenCL conformance tests passed. No real Vulkan validation errors were
observed; the callback unit test intentionally emits a synthetic error.

The local pool independently reconstructed and SHA-256d checked complete
submitted 80-byte headers. It exercised job and difficulty changes, status
requests, slow SSE readers and bounded shutdown. The production Vulkan run also
forced reconnection. These tests used synthetic local work, no real wallet and
no public pool.

| Final image/backend | Duration | Accepted/submitted | Invalid/duplicate/stale | Shutdown |
| --- | --- | --- | --- | --- |
| Production Vulkan/pre3 | 151.5 s | 4/4 | 0/0/0 | 1.083 s, exit 0 |
| Debug Vulkan/pre3, validation | 35.7 s | 1/1 | 0/0/0 | 0.478 s, exit 0 |
| Production OpenCL | 35.8 s | 1/1 | 0/0/0 | 0.578 s, exit 0 |

The production Vulkan run completed two connections/authorizations and collected
148 status samples. Local evidence resides in
`bench-results/rust-production-integration/`,
`bench-results/rust-debug-integration/` and
`bench-results/rust-opencl-integration/`. Each contains `summary.json`,
`status.jsonl` and `miner.log`. Build/test logs are under
`/tmp/gbtc-rewrite-20261018/`.

## Remaining qualification

Quiet paired C/Rust measurements, sustained hashrate confirmation, remaining
kernel/local-size/poll tuning, unrestricted compilation coexistence measurements
and the extended 30-minute soak are deferred. Shared-load exploratory screens
cannot establish a performance winner. Public-pool acceptance and other hardware
are untested. The original image and launcher remain available for rollback.

See [the runtime guide](RUST-RUNTIME.md) for build, probe, launch and qualification
commands, and [the ceiling notes](CEILING.md) for bounded performance evidence.

## Subsequent while-xmrig branch

The 2026-10-19 implementation, 153-test qualification, container-built GPU
conformance and sustained four-worker XMRig results are recorded in
[WHILE-XMRIG.md](WHILE-XMRIG.md). Its image tags and launcher port are separate
from the historical reference images listed above.
