# Repository Guidelines

## Project Structure & Module Organization

This repository is a compact experimental Bitcoin solo-miner prototype. The main
implementation lives in `miner.c`: CPU SHA-256 helpers, Stratum handling,
EGL/GLES setup, generated compute shader code, mining control, and the local HTTP
status UI. Container build and runtime wiring are in `Dockerfile` and
`compose.yaml`. Documentation lives under `docs/`, with support process files in
`.github/`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, and `README.md`.

## Build, Test, and Development Commands

- `cp .env.example .env`: create local runtime configuration; replace the sample
  BTC address.
- `docker compose config --quiet`: validate Compose syntax and interpolation.
- `docker compose build`: compile `miner.c` in the Debian build stage and create
  `gamble-btc:latest`.
- `docker compose up -d`: start the hardware-specific miner container.
- `docker compose logs -f`: follow runtime, Stratum, EGL/GLES, and mining logs.
- `docker compose down`: stop and remove the running service.

The status UI is exposed at `http://localhost:41174/` when the service is
running. The current Compose file is host-specific and expects `/dev/dri/card0`,
`/dev/dri/renderD128`, group `44`, and group `992`.

## Coding Style & Naming Conventions

Follow `.editorconfig`: UTF-8, LF endings, final newline, and trimmed trailing
whitespace. Use 4-space indentation for C and 2-space indentation for Markdown
and YAML. Keep C code plain, explicit, and dependency-light. Prefer descriptive
static helper names, direct error handling, and clear log messages. Do not hide
hardware, driver, or container assumptions.

## Testing Guidelines

There is no dedicated test harness yet. For now, verify changes with
`docker compose config --quiet` and `docker compose build`; inspect GCC warnings.
Runtime verification should include the tested iGPU, kernel, Mesa/vendor driver,
Docker version, device paths, Stratum behavior, logs, and status page behavior.
Future tests should prioritize deterministic CPU-side hash and Stratum helpers
before hardware-gated GPU smoke tests.

## Commit & Pull Request Guidelines

This directory does not include Git history, so no local commit convention can be
derived. Use imperative commit subjects such as `Document HD 4600 runtime`.
Pull requests should follow `.github/PULL_REQUEST_TEMPLATE.md`: include a
summary, scope, verification commands, tested hardware/driver stack, and any
host-specific or untested paths. Attach logs or screenshots when behavior changes
the status UI.

## Security & Configuration Notes

Never commit `.env`, real wallet addresses, logs with private runtime details, or
local build artifacts. The HTTP status endpoint has no authentication; keep it on
a trusted network unless a reverse proxy or auth layer is added.
