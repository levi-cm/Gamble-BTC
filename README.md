# Gamble BTC

Gamble BTC is an experimental GPU Bitcoin solo miner. The `gtx-1060-6gb`
branch configures the container to expose an NVIDIA GTX 1060 6GB and selects
the existing OpenCL backend. It talks to a Stratum pool such as `public-pool.io`.

This project is not production mining software. It is an educational prototype
for exploring GPU compute, Bitcoin block-header hashing, Stratum work handling,
and containerized GPU access.

## Current Status

- Current branch target: NVIDIA GeForce GTX 1060 6GB using OpenCL.
- Runtime shape: one C binary in a Debian-based Docker image.
- Backend shape: GLES and OpenCL are implemented; Vulkan is an unavailable
  placeholder. Compose selects OpenCL and filters for the NVIDIA platform.
- Control surface: HTTP status page, `/status.json`, and server-sent events on
  port `41174`.
- Mining mode: solo Stratum flow with the configured BTC address as username.
- Compose file: uses Docker GPU passthrough (`gpus: all`) and needs the NVIDIA
  Container Toolkit installed on the host.
- CPU mining is intentionally not implemented. The project exists to keep mining
  work on the GPU, where it can run with much lower CPU load than a CPU miner.
  Host-side hashing is only used for work construction and tests.

## Safety Notes

- Mining results are probabilistic. On consumer GPUs, finding a Bitcoin block
  is extremely unlikely.
- This is not a CPU miner. Use an existing CPU miner if you want CPU mining; it
  is intentionally outside this project's scope.
- Do not commit your real `.env`. This repo ignores it by default.
- Verify your BTC address before running. Submitted shares use the configured
  address and worker name.
- The HTTP status port has no authentication and binds to loopback by default.
  Use LAN binding only on a trusted network; add authentication before broader
  exposure.
- This is not financial advice and does not guarantee earnings.

## Requirements

- Linux host with Docker and NVIDIA drivers.
- NVIDIA Container Toolkit and Docker Compose with `gpus: all` support.
- NVIDIA OpenCL support exposed to containers.
- Network access to the configured Stratum endpoint.

## Configuration

Copy the example environment file and edit it for your wallet and worker:

```sh
cp .env.example .env
```

Compose runs a dedicated Tailscale sidecar with a local SOCKS5 endpoint. All
miner TCP connections and pool DNS use a strict proxychains chain through it.
The preferred exit is `max-vprouter.dinosaur-dojo.ts.net`; if unavailable or its
Mullvad verification fails, the sidecar automatically tries
`nico-vpn.dinosaur-dojo.ts.net`. It returns to the primary after three successful
reachability checks. If neither works, readiness is denied and mining stops.
A route change restarts mining with a fresh Stratum connection. No notification
is sent for fallback, and the host's Tailscale configuration is unchanged.

For a new deployment, build and start the sidecar, authenticate it in your
intended tailnet, then start mining:

```sh
docker compose build
docker compose up -d tailscale
docker compose exec tailscale tailscale up --hostname=gamble-btc-gtx1060
docker compose up -d miner
```

Keep the `tailscale-state` volume private; it stores the node identity. Compose
sets `GBTC_SOCKS5_HOST=127.0.0.1`, `GBTC_SOCKS5_PORT=1055`, and requires Mullvad
verification. For direct Docker runs, supply a reachable numeric IPv4 SOCKS
address and port yourself. `ALL_PROXY` alone does not route this C program's
sockets. GPU-only probes and synthetic benchmarks do not use the network.

Check the exact miner path without printing exit IP or location:

```sh
docker compose exec miner sh -c 'set -- /tmp/gbtc-proxychains.*;
  exec proxychains4 -q -f "$1" curl --fail --silent https://am.i.mullvad.net/json' \
  | jq '{mullvad_exit_ip}'
```

The libc proxy wrapper routes this miner's ordinary TCP sockets; it is not a
network sandbox for arbitrary raw-socket programs. Sidecar health is checked
at startup and the miner guard checks fresh verified readiness every 10 seconds.

Environment variables:

| Name | Required | Default | Description |
| --- | --- | --- | --- |
| `BTC_ADDRESS` | Yes | none | Bech32 BTC address used as the Stratum username. |
| `POOL_URL` | No | `stratum+tcp://public-pool.io:3333` | Stratum TCP endpoint. |
| `WORKER_NAME` | No | `x` | Worker name sent as the Stratum password. |
| `GBTC_SOCKS5_HOST` | For live mining | Compose: `127.0.0.1` | Numeric SOCKS5 IPv4 address; Compose fixes the sidecar endpoint. |
| `GBTC_SOCKS5_PORT` | No | Compose: `1055` | SOCKS5 port; live mining has no direct fallback. |
| `GBTC_EXIT_PRIMARY` | No | `max-vprouter.dinosaur-dojo.ts.net` | Preferred verified Tailscale exit. |
| `GBTC_EXIT_FALLBACK` | No | `nico-vpn.dinosaur-dojo.ts.net` | Automatic fallback verified exit. |
| `GBTC_BACKEND` | No | `opencl` | GPU backend selection. This branch uses OpenCL. |
| `GBTC_PROBE_ONLY` | No | `0` | Set to `1` to print backend availability and exit without mining. |
| `GBTC_OPENCL_PLATFORM` | No | `NVIDIA` | OpenCL platform name filter used to select the NVIDIA driver. |
| `GBTC_HTTP_BIND` | No | `0.0.0.0` | Address inside the container; Compose keeps the host port bound to `127.0.0.1`. |
| `GBTC_STATUS_BIND` | No | `127.0.0.1` | Compose host-port bind; use `0.0.0.0` to opt into trusted-LAN access. |
| `GBTC_BATCH_NONCES` | No | `16777216` | Power-of-two nonce batch size; must be a multiple of 64. |
| `GBTC_BENCH_ONLY` | No | `0` | Set to `1` to run deterministic synthetic work and exit without Stratum. |
| `GBTC_BENCH_SECONDS` | No | `60` | Timed benchmark duration in seconds. |
| `GBTC_BENCH_WARMUP_SECONDS` | No | `5` | Warmup duration before measured benchmark work. |
| `GBTC_GLES_KERNEL` | No | `altbool` | GLES shader variant: `unrolled`, `partial`, `looped`, `altbool`, or `dualnonce`. |
| `GBTC_GLES_LOCAL_SIZE` | No | `16` | GLES compute local size: `auto`, `8`, `16`, `32`, `64`, `128`, or `256`. |
| `GBTC_GLES_AUTOTUNE` | No | `0` | With `GBTC_BENCH_ONLY=1`, benchmark the selected GLES kernel across all local sizes. |

## Run

Build and start the container:

```sh
docker compose up -d --build
```

The legacy `scripts/start-miner.sh` launcher still targets Intel hardware.
Use Compose on this GTX 1060 branch until that launcher is migrated.

Follow logs:

```sh
docker compose logs -f
docker logs -f gamble-btc
```

Open the local status page:

```text
http://localhost:41174/
http://localhost:41174/status.json
```

Compose keeps the host port on loopback while the process listens inside the
container for port forwarding. Set `GBTC_STATUS_BIND=0.0.0.0` only when LAN
access is intended.

Probe the configured NVIDIA OpenCL stack without connecting to the pool:

```sh
docker compose run --rm --no-deps -e GBTC_PROBE_ONLY=1 miner
```

Run the current GLES benchmark path without connecting to the pool:

```sh
scripts/bench-current.sh
```

Run the full GLES kernel/local-size matrix and store JSONL results locally:

```sh
scripts/bench-gles-matrix.sh
```

Stop the container:

```sh
docker compose down
scripts/stop-miner.sh
```

`scripts/stop-miner.sh` is only needed for detached/background runs; the default
`scripts/start-miner.sh` session stops its own container on exit.

## Development

Build the binary through Docker:

```sh
docker build -t gamble-btc:latest .
```

Or use the Compose workflow when Docker Compose is installed:

```sh
docker compose build
```

The image build compiles the `src/` C sources with GCC and links against EGL,
GLESv2, cJSON, pthreads, and math libraries.

Run deterministic helper tests locally:

```sh
make test
```

For direct host builds, install equivalent development packages for your
distribution, then compile with the same flags used in the Dockerfile.

## GTX 1060 optimization handoff

The [48-hour OpenCode prompt](docs/GTX-1060-48H-OPENCODE-PROMPT.md) restricts
optimization to C/CUDA/PTX and NVIDIA OpenCL. It includes thermal protection,
correctness gates, local Git checkpoints, branch-only pushes and owned cleanup.
The preparation does not start an agent or a 48-hour run.

The developer tools are installed in `gamble-btc-toolbox:cuda12.9`:

```sh
scripts/dev-toolbox.sh bash
scripts/dev-toolbox.sh make
```

CUDA 12.9 supports Pascal `sm_61`; CUDA 13 does not compile this architecture.
The toolbox supplies compilers, CUDA binary inspection, Pascal-compatible
`nvprof`, OpenCL headers/loader, diagnostics and host Rust tools. Nsight Compute
is bundled but cannot profile this Pascal GPU. No host driver replacement or
reboot is needed. Keep synthetic GPU runs separate from the live miner.

## Repository Layout

```text
.
|-- src/             # Miner runtime, SHA helpers, and backend selection helpers
|-- tests/           # Host-side deterministic C tests
|-- Makefile         # Local and Docker build entrypoint
|-- Dockerfile       # Multi-stage container build
|-- compose.yaml     # Host-specific runtime wiring
|-- .env.example     # Safe example configuration
|-- docs/            # Architecture and roadmap notes
```

## License

Gamble BTC is licensed under the MIT License. See [LICENSE](LICENSE).
