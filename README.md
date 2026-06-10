# Gamble BTC

Gamble BTC is an experimental single-file Bitcoin solo miner that uses an
integrated GPU through EGL surfaceless rendering and a GLES 3.2 compute shader.
The current container is tuned for a local Intel HD 4600 / Mesa crocus setup and
talks to a Stratum pool such as `public-pool.io`.

This project is not production mining software. It is an educational prototype
for exploring GPU compute, Bitcoin block-header hashing, Stratum work handling,
and containerized access to Linux DRM devices.

## Current Status

- Current target: Intel HD 4600 class iGPU using Mesa crocus.
- Runtime shape: one C binary in a Debian-based Docker image.
- Control surface: HTTP status page and server-sent events on port `41174`.
- Mining mode: solo Stratum flow with the configured BTC address as username.
- Compose file: intentionally host-specific for the original development
  machine.

The next major scope expansion is to support the major integrated GPU families
instead of only this one host/GPU path. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Safety Notes

- Mining results are probabilistic. On consumer iGPUs, finding a Bitcoin block
  is extremely unlikely.
- Do not commit your real `.env`. This repo ignores it by default.
- Verify your BTC address before running. Submitted shares use the configured
  address and worker name.
- The HTTP status port has no authentication. Keep it on a trusted network or
  add a reverse proxy/auth layer before exposing it.
- This is not financial advice and does not guarantee earnings.

## Requirements

- Linux host with Docker Compose.
- DRM render devices exposed at the paths used in `compose.yaml`.
- Mesa userspace support for the target iGPU.
- Network access to the configured Stratum endpoint.

The included Compose file currently expects:

```text
/dev/dri/card0
/dev/dri/renderD128
video group id 44
render group id 992
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
| `POOL_URL` | No | `stratum+tcp://public-pool.io:21496` | Stratum TCP endpoint. |
| `WORKER_NAME` | No | `x` | Worker name sent as the Stratum password. |

## Run

Build and start the container:

```sh
docker compose up -d --build
```

Follow logs:

```sh
docker compose logs -f
```

Open the local status page:

```text
http://localhost:41174/
```

Stop the container:

```sh
docker compose down
```

## Development

Build the binary through Docker:

```sh
docker compose build
```

The image build compiles `miner.c` with GCC and links against EGL, GLESv2,
cJSON, pthreads, and math libraries.

For direct host builds, install equivalent development packages for your
distribution, then compile with the same flags used in the Dockerfile.

## Repository Layout

```text
.
|-- miner.c          # Miner, Stratum client, GPU kernel generation, HTTP status UI
|-- Dockerfile       # Multi-stage container build
|-- compose.yaml     # Host-specific runtime wiring
|-- .env.example     # Safe example configuration
|-- docs/            # Architecture and roadmap notes
```

## License

Gamble BTC is licensed under the MIT License. See [LICENSE](LICENSE).
