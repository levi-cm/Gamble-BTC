# Gamble BTC

Gamble BTC is an experimental iGPU-only Bitcoin solo miner. The Rust candidate
provides Vulkan compute, a Rust OpenCL compatibility backend, Stratum and the
local status UI. The C runtime and its Docker image remain available as references.

For the candidate build, configuration, tests and rollback commands, see
[the Rust runtime guide](docs/RUST-RUNTIME.md) and
[qualification results](docs/QUALIFICATION.md). The sections below describe the
preserved C workflow unless explicitly marked Rust.

## Current status

- Hardware target: Intel Iris Xe / Tiger Lake integrated GPU.
- Rust production inputs: `rust/`, `Dockerfile.rust`, `compose.rust.yaml`.
- Explicit Rust backends: `vulkan` and `opencl`; `auto` tries them in that order.
- HTTP UI, `/status.json` and SSE retain the original interface. The direct Rust
  launcher uses loopback port 41176; the binary defaults 41174.
- CPU nonce search is out of scope. Host hashes construct work and verify GPU
  candidates.
- 350 MH/s is a research target. It is not an established result or a consequence
  of choosing Rust. Measured performance and remaining gates are recorded in
  `docs/QUALIFICATION.md`.

## Safety Notes

- Mining results are probabilistic. On consumer iGPUs, finding a Bitcoin block
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

- Linux host with Docker.
- Docker Compose if using the Compose workflow or benchmark helper scripts.
- DRM render devices exposed at the paths used in `compose.yaml`.
- Mesa userspace support for the target iGPU.
- Network access to the configured Stratum endpoint.

The included Compose file currently expects:

```text
/dev/dri/card1
/dev/dri/renderD128
video group id 983
render group id 987
```

Those values are intentionally not generalized yet.

## Configuration

Copy the example environment file and edit it for your wallet and worker:

```sh
cp .env.example .env
```

Environment variables:

| Name | Required | Default | Description |
| --- | --- | --- | --- |
| `BTC_ADDRESS` | Yes | none | Bech32 BTC address used as the Stratum username. |
| `POOL_URL` | No | `stratum+tcp://public-pool.io:3333` | Stratum TCP endpoint. |
| `WORKER_NAME` | No | `x` | Worker name sent as the Stratum password. |
| `GBTC_BACKEND` | No | `auto` | iGPU backend selection: `auto`, `gles`, `vulkan`, or `opencl`. |
| `GBTC_PROBE_ONLY` | No | `0` | Set to `1` to print backend availability and exit without mining. |
| `GBTC_DEVICE` | No | `/dev/dri/renderD128` | Render node expected by GPU backends. |
| `GBTC_HTTP_BIND` | No | `127.0.0.1` | IPv4 bind address for direct or host-network status serving; use `0.0.0.0` to opt into trusted-LAN access. |
| `GBTC_STATUS_BIND` | No | `127.0.0.1` | Compose host-port bind; use `0.0.0.0` to opt into trusted-LAN access. |
| `GBTC_BATCH_NONCES` | No | `16777216` | Power-of-two nonce batch size; must be a multiple of 64. |
| `GBTC_BENCH_ONLY` | No | `0` | Set to `1` to run deterministic synthetic work and exit without Stratum. |
| `GBTC_BENCH_SECONDS` | No | `60` | Timed benchmark duration in seconds. |
| `GBTC_BENCH_WARMUP_SECONDS` | No | `5` | Warmup duration before measured benchmark work. |
| `GBTC_GLES_KERNEL` | No | `altbool` | GLES shader variant: `unrolled`, `partial`, `looped`, `altbool`, or `dualnonce`. |
| `GBTC_GLES_LOCAL_SIZE` | No | `16` | GLES compute local size: `auto`, `8`, `16`, `32`, `64`, `128`, or `256`. |
| `GBTC_GLES_AUTOTUNE` | No | `0` | With `GBTC_BENCH_ONLY=1`, benchmark the selected GLES kernel across all local sizes. |
| `MESA_NO_ERROR` | No | `1` | Mesa no-error mode; measured faster on the Iris Xe GLES benchmark path. |

## Run

Build and start the container:

```sh
docker compose up -d --build
```

Or start the finalized Iris Xe runtime directly from zsh. This keeps the
terminal open, prints the one-minute average hashrate every 60 seconds, and
stops the container when you press Ctrl-C or close the terminal:

```sh
scripts/start-miner.sh
```

To start it as a background container with the Docker restart policy and exit
the script:

```sh
scripts/start-miner.sh --detach
```

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

Probe the configured iGPU stack without connecting to the pool:

```sh
GBTC_PROBE_ONLY=1 docker compose run --rm miner
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

### Running Rust alongside XMRig

The `while-xmrig` branch adds queued Vulkan dispatch, blocking fence waits and
lower-wakeup services. Its separate image is `gamble-btc:rust-while-xmrig`; the
launcher uses localhost port 41176. See [implementation and measurements](docs/WHILE-XMRIG.md)
for controls, correctness checks and the four-worker coexistence runner.
