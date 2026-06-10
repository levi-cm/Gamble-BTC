# Architecture

Gamble BTC is currently built as one C program plus a Docker runtime wrapper.
This document records the current shape so future portability work has a stable
baseline.

## Runtime Components

- `miner.c`: contains SHA-256 helpers, Stratum client logic, EGL/GLES GPU setup,
  generated compute shader source, mining loop, and the local HTTP status UI.
- `Dockerfile`: builds the C binary in a Debian build stage and copies it into a
  smaller runtime image with Mesa and cJSON runtime libraries.
- `compose.yaml`: wires the image to the original host's DRM devices, group IDs,
  environment file, restart policy, memory limit, and HTTP status port.

## Data Flow

1. Environment variables provide the BTC address, pool URL, and worker name.
2. The miner connects to the Stratum endpoint and subscribes/authorizes.
3. Stratum jobs are converted into block-header work.
4. CPU code prepares midstate and merkle data.
5. A GLES compute shader searches nonce batches on the iGPU.
6. Matching nonces are submitted back to the pool.
7. Runtime stats are exposed through the local HTTP endpoint on port `41174`.

## Current Assumptions

- Linux host with accessible `/dev/dri` devices.
- Mesa driver stack supports surfaceless EGL and GLES 3.2 compute shaders.
- The current Docker/Compose files target the original Intel HD 4600 setup.
- The HTTP endpoint is intended for trusted local access only.

## Portability Direction

Future multi-iGPU support should separate hardware discovery, runtime device
selection, GPU backend capability checks, and Compose examples. The current
host-specific path should remain available as a known-good baseline while new
targets are added.
