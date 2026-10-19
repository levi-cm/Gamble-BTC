# Changelog

All notable project changes should be documented here.

## Unreleased

- Corrected Stratum difficulty, authorization, message validation, and exact
  share-target handling.
- Bounded pool-controlled fields and escaped status JSON strings.
- Made status binding configurable with loopback-safe defaults.
- Added Bitcoin header vectors, mock Stratum coverage, sanitizers, static
  analysis, and CI.
- Added repository hygiene files for future open-source publication.
- Documented the current host-specific Intel HD 4600 container setup.
- Added roadmap notes for future multi-iGPU support.

### Rust iGPU candidate (2026-10-18)

- Added a C-independent Rust service with Vulkan compute and Rust OpenCL
  compatibility, strict Stratum handling, immutable work snapshots, full share
  verification and bounded shutdown.
- Ported the HTTP/status/SSE UI and preserved its routes and history interface.
- Added generated unrolled/precomputed Vulkan shaders, independent GPU
  conformance, separate production/debug containers, launcher and qualification
  tooling. C source and the existing image remain available for rollback.
- Performance qualification is deferred while the other project uses the iGPU;
  no 350 MH/s result or speedup over C is claimed.

## while-xmrig branch

- Add bounded two-slot Vulkan submission with immutable work caching and per-slot overflow recovery.
- Use event-driven GPU/service waits and lifetime-safe share/shutdown wakeups.
- Add GPU/host telemetry and reproducible five-minute four-worker XMRig comparisons.
- Keep reference images separate; use `gamble-btc:rust-while-xmrig` and localhost port 41176.
