# GTX 1060 optimization preparation — 2026-10-01

The 48-hour OpenCode campaign has **not** started. Its handoff is
[GTX-1060-48H-OPENCODE-PROMPT.md](GTX-1060-48H-OPENCODE-PROMPT.md).
All repository work targets `gtx-1060-6gb`; publication must never target `main`.

## Two retained ideas

1. C host with CUDA Driver API and Pascal `sm_61` CUDA/PTX kernels.
2. NVIDIA-specific OpenCL tuning in the existing C miner, as control and rollback.

Keep safety, correctness, low CPU, job lifecycle and thermal protection common
to both. Rust host rewrites, Vulkan, DirectX, GLES and additional frameworks
are excluded from this campaign under the user's two-idea limit. This does not
establish a universal API or language speed ranking.

## Actual short screening

GTX 1060 6GB; driver 580.95.05. Synthetic header SHA-256d, local size 64,
2^24 nonces/batch, 5 seconds warmup and approximately 30 seconds timed per run.
The live miner was stopped, and GPU workloads were serialized.

| Ordered experiment | MH/s | Timed CUDA host CPU, one core | Maximum sampled temperature |
| --- | ---: | ---: | ---: |
| OpenCL unrolled | 616.29 | Not measured by this harness | 71 C |
| Plain CUDA translation | 621.96 | 0.653% | 73 C |
| Explicit CUDA rotates/lop3 | 624.75 | 0.644% | 74 C |
| CUDA tail-round pruning | 622.51 | 0.614% | 74 C |
| OpenCL pre3 | 609.82 | Not measured by this harness | 74 C |
| Explicit CUDA repeat | 625.55 | 0.616% | 75 C |
| OpenCL unrolled repeat | 609.74 | Not measured by this harness | 74 C |

The small apparent CUDA gain is provisional: ordered short runs, heat/boost
changes and different host harnesses limit comparison. It does not demonstrate
1 GH/s, sustained thermal safety or live CUDA mining. CUDA was not integrated
into the application. The current live application remains OpenCL.

All three CUDA kernels used 40 registers/thread, with zero reported spills and
local storage. Each had 2610 static SASS instruction lines. Plain code already
compiled to funnel shifts and ternary logic. Explicit final-round pruning did
not reduce that count; compiler dead-code elimination already removes unused
results. A large speedup requires more than selecting another API.

The three CUDA variants passed 18 independent Python hashlib comparisons:
complete sparse prefilter sets, equality, zero and nonzero bases, and a range
ending at UINT32_MAX. A small actual nvprof kernel/memcpy trace succeeded;
compute-sanitizer memcheck reported zero errors. These are finite screening
checks, not complete protocol or 48-hour acceptance evidence.

Temperature sampling every two seconds peaked at 75 C. No sampled SW/HW thermal
slowdown indication was active. The screen had a process-independent stop on
>75 C, active thermal flags or excessive runtime. No clock, power, voltage or
fan settings were changed; no driver replacement, reset or reboot occurred.

## Installed programming tools

### Host

GCC/build-essential, Clang/LLVM 19, lld, pkg-config, CMake, Ninja, ccache,
gdb/Valgrind, ShellCheck/cppcheck, sysstat/pidstat, strace, jq, Python development
and venv support, C EGL/GLES/cJSON and OpenCL development headers/loader/clinfo.
Docker, Git and tmux are available. A native `make -j2` completed without warnings.
OpenCode 1.18.34 is installed at `/home/nico/.local/bin/opencode`. No provider,
model preference or agent configuration was changed, and no agent was launched.

### Developer container

`gamble-btc-toolbox:cuda12.9` uses a pinned CUDA 12.9.1 Ubuntu 24.04 base. It
contains nvcc 12.9.86, ptxas, cuobjdump/nvdisasm, nvprof, compute-sanitizer,
cuda-gdb, GCC 13.3, Clang/LLVM, build dependencies, diagnostics, host Rust/Cargo
1.75 with formatter/clippy, and Vulkan/SPIR-V inspection tools.

```sh
scripts/dev-toolbox.sh scripts/check-dev-tools.sh
scripts/dev-toolbox.sh bash
scripts/dev-toolbox.sh make
```

The inventory passed against the actual GTX 1060. Use explicit `sm_61` compiler
flags. CUDA 13 cannot compile Pascal. Bundled Nsight Compute cannot profile
Pascal; use the verified nvprof route. Nsight Systems is not installed or
required. GPU hardware counter permissions remain a separate capability check.
The container mounts this checkout using the host UID/GID; do not start its GPU
experiments while the live miner is hashing.

## Implemented VPN route

Compose gives the miner a dedicated authenticated userspace Tailscale sidecar
and local SOCKS5 endpoint. Its ordinary C TCP connects and DNS use strict
proxychains; they do not fall back to direct host networking. Startup checks
that the exact proxy path exits through Mullvad.

- Preferred exit: `max-vprouter.dinosaur-dojo.ts.net`.
- Automatic fallback: `nico-vpn.dinosaur-dojo.ts.net`.
- Failback requires three consecutive primary reachability checks.
- Mullvad verification occurs on selection and at least every 60 seconds.
- Neither usable: readiness disappears and the last selected exit remains set.
- The miner guard checks readiness every 10 seconds, stops old work on route
  change/loss, and Docker restarts it with a fresh Stratum connection.
- State and readiness have separate named volumes; the miner reads readiness
  without access to the private Tailscale identity.

Mock boundary tests exercise the production manager: primary selection,
unreachable-primary fallback, hysteresis failback, primary non-Mullvad fallback,
and both-offline denial while retaining the exit preference. Miner guard tests
prove changed, missing and stale readiness terminate old work and request restart.
An isolated unavailable SOCKS endpoint failed without direct curl fallback.
Real exit switching was also checked using local primary-reachability fault
injection. The miner reconnected and resumed hashing on fallback, then did so
again after automatic failback (restart count 0 → 1 → 2). No shared router or
host Tailscale configuration was changed.

The proxy wrapper is not a firewall for arbitrary raw-socket or UDP programs.
Normal Docker sidecar **recreation** must be accompanied by miner recreation
through `docker compose up -d`, because the shared namespace owner changes.
Ordinary exit failover happens within one sidecar and does not require recreation.
Pool outages unrelated to route changes still need robust C Stratum reconnect
handling during the optimization campaign; the existing reader can retain stale
work after a disconnected socket.

## Build and verification limits

- Compose validation and fresh runtime/embedded-UI builds succeeded.
- All eight C test executables passed, including actual NVIDIA OpenCL
  conformance for unrolled, looped, dual, SIMD-16 and pre3 configurations.
- `make test` as a whole **fails** in the preexisting legacy Intel launcher's
  PTY key-handling test: expected exit 143, got 137, with the key hint absent.
  The launcher is not the deployed Compose route. Do not claim a green full suite.
- The new shell scripts pass ShellCheck and syntax checks.
- Local HTTP status, advancing completed hashes, proxied Mullvad verification,
  Stratum connection and the served embedded UI were checked live.
- A separate peer in the same tailnet fetched the miner's HTTP UI successfully.
- No browser is available in this session; rendered interaction was not checked.
- No accepted public-pool share was observed in the short preparation window.

Raw finite-probe source, SASS, logs and measurements are ignored under
`bench-results/hypothesis-20261001/`. Promote required prototype source into
tracked code before making it a maintained backend. Keep baseline, winner and
rollback builds; delete only owned obsolete artifacts. The obsolete owned
`gamble-btc:verification-20261001` image was removed after
replacement C/GPU checks passed in the developer image. No repository artifact
family exceeded 250 MiB; the current small C build and screening records were
retained. Other applications' dangling images and shared caches were preserved.

## Sources

- [Pascal tuning guide](https://docs.nvidia.com/cuda/archive/12.9.1/pascal-tuning-guide/index.html)
- [PTX instructions](https://docs.nvidia.com/cuda/archive/12.9.1/parallel-thread-execution/index.html)
- [CUDA 13 removed architectures](https://docs.nvidia.com/cuda/archive/13.0.0/cuda-toolkit-release-notes/index.html#deprecated-architectures)
- [CUDA profiler hardware support](https://docs.nvidia.com/cuda/archive/12.9.1/profiler-users-guide/index.html)
- [Rust NVPTX target support](https://doc.rust-lang.org/rustc/platform-support/nvptx64-nvidia-cuda.html)
- [Vulkan compute shaders](https://docs.vulkan.org/spec/latest/chapters/shaders.html)
- [Direct3D 12 programming guide](https://learn.microsoft.com/en-us/windows/win32/direct3d12/directx-12-programming-guide)
- [Tailscale userspace routing](https://tailscale.com/docs/concepts/userspace-networking)
