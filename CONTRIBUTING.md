# Contributing

Gamble BTC is early-stage experimental software. Contributions should keep the
project honest about what is supported today and should avoid presenting future
GPU support as complete until it is tested.

## Priorities

- Preserve the current Intel HD 4600 host-specific path until a replacement is
  tested.
- Prefer small, reviewable changes over broad rewrites.
- Document hardware, driver, and kernel assumptions when adding support.
- Keep real wallet addresses, private configuration, and local runtime state out
  of commits.

## Local Workflow

1. Copy `.env.example` to `.env` and use a test or throwaway BTC address while
   developing.
2. Build with `docker compose build`.
3. Run with `docker compose up -d`.
4. Check logs with `docker compose logs -f`.
5. Keep runtime-specific changes separate from portable source changes.

## Pull Requests

Before proposing a change, include:

- What hardware and driver stack you tested.
- The Docker/Compose command used for verification.
- Whether the change affects mining behavior, GPU initialization, Stratum
  handling, or only documentation/configuration.

## Style

- Keep C code plain and dependency-light.
- Prefer explicit error handling and clear log messages.
- Do not hide host-specific assumptions; document them.
