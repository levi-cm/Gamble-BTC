# Part 10 — Ordered Tasks for Small-Context Workers

DeepSeek orchestrates; Beam Free max workers implement. See Part 11 for exact
model/launch rules. This is an execution checklist, not evidence of completed code.
One worker owns one task or a named substep. Use fresh bounded packets and retain
progress. The orchestrator reviews the diff and reruns the relevant checks.

For each checkbox: inspect the named input, write a meaningful failing regression
case when changing behavior, implement the smallest working increment, run its
check and save evidence. Split a checkbox further if it cannot be verified in one
bounded session. Do not send a worker “implement everything in this folder.”

## T01 — Recon and compatibility inventory

Inputs: AGENTS, Parts 0/1/9, source tree, tests, Docker/Compose/scripts/CI.
Output: `docs/RUST-COMPATIBILITY.md`, PROGRESS (orchestrator-owned).

- [ ] Record branch, commit, dirty paths, image identity and available tool versions.
- [ ] Map runtime functions, settings, routes and test coverage to Part 9 inventory.
- [ ] Resolve README/AGENTS/Compose disagreements and define GLES migration.
- [ ] Check inventory covers runtime AND script/Compose settings, not C constants.

Check: inspect source references and `git diff --check`; no runtime change.

## T02 — Safe baseline and benchmark harness

Inputs: Part 2, bench scripts and existing script tests.
Write scope: bench scripts, `tests/test_bench_scripts.sh`, baseline docs.

- [ ] Add mocked backend/image/env forwarding and exit/timeout/output tests.
- [ ] Remove OpenCL forcing for cross-backend runs; preserve explicit selection.
- [ ] Add balanced order, correct GPU telemetry mapping and exact cleanup.
- [ ] Run mocked tests and syntax checks; record baseline or hardware block.

Check: `bash tests/test_bench_scripts.sh`; `bash -n` changed scripts. GPU run only
in an exclusive authorized window. G1 records actual state, not an invented rate.

## T03 — Workspace and reproducible build skeleton

Inputs: Part 1 and T01 inventory.
Write scope: `rust/Cargo.toml`, lock/toolchain files and initial core crate.

- [ ] Create workspace and explicit tested Rust/tool dependencies.
- [ ] Create minimal core crate with one executable host test.
- [ ] Run `cargo test --manifest-path rust/Cargo.toml -p gbtc-core`, generate lock.
- [ ] Rerun with `--locked` and document the builder environment.

Check: core tests pass without loader, GPU or shader compiler. G0 requires T01/T03.

## T04 — Core hashes, targets, work and shared types

Inputs: Part 4 core section, Part 9, C helper/protocol tests and work builder.
Write scope: `rust/gbtc-core/`, independent byte-order fixtures.

- [ ] Define common batch/result/error/info and immutable job/range types.
- [ ] Add independent SHA-256/SHA-256d and known-header vectors; port compression.
- [ ] Add coinbase/merkle/header-byte and exact target/difficulty vectors.
- [ ] Add randomized full-header equivalence, equality, range and exhaustion tests.

Check: `cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-core`.
Orchestrator freezes shared interfaces before GPU/protocol workers consume them.

## T05 — Vulkan probe and resource foundation

Inputs: Part 3 device section, Part 1 shader build, T04 interfaces.
Write scope: `rust/gbtc-backend-vulkan/`, initial `rust/shaders/sha256d.comp`.

- [ ] Add dynamic loader, physical-device identity and capability/limit checks.
- [ ] Add owned resource initialization/cleanup with documented unsafe invariants.
- [ ] Add reproducible shader compilation and a minimal checked dispatch fixture.
- [ ] Test unavailable/wrong device, unsupported limits and partial-init failure.

Check: crate host checks/build, spirv-val and exclusive GPU probe/dispatch;
no hash-performance claim. Missing GPU is recorded, not passed.

## T06 — Single-slot SHA-256d batch path

Inputs: Part 3 batch section, Part 9 byte-order fixtures, T05 resources.
Write scope: Vulkan pipeline/buffer modules and baseline shader/tests.

- [ ] Define/assert buffer layout and push constants; port unrolled SHA-256d.
- [ ] Implement barriers, flush/invalidate, fence sleep-poll and bounded waits.
- [ ] Implement bounded output and overflow subdivision/rescan with dedup.
- [ ] Test candidate full checking, ranges and unique-hash accounting.

Check: small known-header and complete-set device cases, debug validation clean.
No optional variants, subgroup tuning or multiple in-flight slots yet.

## T07 — Required Vulkan conformance

Inputs: Part 3 conformance cases, existing C GPU tests, independent core oracle.
Write scope: Vulkan conformance tests and minimal shared core test support.

- [ ] Add seeded complete-set, target equality and endianness tests.
- [ ] Add partial-workgroup/end-of-range, counter reset and overflow union tests.
- [ ] Add repeated dispatch, cancellation, timeout and cleanup coverage.
- [ ] Add required-GPU mode; run on exact device/driver plus validation layers.

Check: Part 6 required Vulkan command; G2 passes only with actual-device evidence.

## T08 — Configuration and Rust OpenCL

Inputs: Part 4 OpenCL/config section, Part 9 inventory, `src/opencl.c`.
Write scope: `rust/gbtc-backend-opencl/`, `rust/gbtc-miner/src/config.rs` and tests.

- [ ] Test default/invalid/explicit backend settings and GLES migration errors.
- [ ] Port OpenCL selection, kernel variants and event/sleep-poll ownership.
- [ ] Implement complete-candidate overflow handling using shared semantics.
- [ ] Run shared conformance on NEO, plus CPU/wait checks; remove production C FFI.

Check: config tests, Part 6 required OpenCL command and production link/build
inspection. Split config and OpenCL into separate sequential packets if needed.

## T09 — Stratum parser and client

Inputs: Part 4 Stratum section, core types and C protocol tests.
Write scope: `rust/gbtc-stratum/`; no HTTP/controller edits.

- [ ] Add bounded frame/parser tests, then parser implementation.
- [ ] Add mock subscribe/authorize/notify/difficulty and ID-interleaving tests.
- [ ] Implement generation snapshots, bounded submits and reconnect/backoff.
- [ ] Test disconnect/stale/clean-job/difficulty races and exact submit fields.

Check: `cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-stratum`.
No real pool or wallet; fake backend/job generator allowed.

## T10 — Status, SSE and existing UI

Inputs: Part 4 HTTP section, Part 9 route/schema, C HTTP and embedded UI.
Write scope: `rust/gbtc-http/`; no GPU code.

- [ ] Extract existing UI assets and add synthetic status serialization fixtures.
- [ ] Add actual HTTP GET/HEAD/OPTIONS/error tests and bounded SSE behavior.
- [ ] Verify waiting/active/history/reconnect behavior and safe string rendering.
- [ ] Test slow/disconnected clients, bounded memory and no hot polling.

Check: `cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-http`;
record sanitized UI screenshot/manual browser evidence when available.

## T11 — Mining controller and fake-backend races

Inputs: Part 4 controller section; T04/T08/T09/T10 public interfaces.
Write scope: `rust/gbtc-miner/src/controller.rs` and lifecycle tests.

- [ ] Wire bounded worker channels and immutable job snapshots.
- [ ] Implement u64 range cursor, extranonce exhaustion and target generations.
- [ ] Validate candidates and eligible submissions after work changes.
- [ ] Test clean/non-clean jobs, reconnect, overflow, cancellation and duplicates.

Check: deterministic fake-backend/controller tests; no GPU required for races.
Do not “fix” shared type mismatches independently: return them to orchestrator.

## T12 — Binary, synthetic benchmark and shutdown

Inputs: T11 controller, config, both GPU backends, HTTP and Stratum.
Write scope: `rust/gbtc-miner/src/main.rs`, `bench.rs`, integration tests.

- [ ] Wire explicit and auto backend policy; no hidden CPU or C-host fallback.
- [ ] Add probe/bench modes without credentials, HTTP or pool side effects.
- [ ] Add exact unique-hash timing/output, finite limits and signal cleanup.
- [ ] Run mock application end-to-end, GPU checks and actual status UI.

Check: workspace tests/checks plus explicit probe/bench on candidate device.
G3 requires all parity tests, both backends and C-independent production build.

## T13 — Container, scripts and CI

Inputs: Part 6 build section; T12 executable and migration inventory.
Write scope: Dockerfile, Makefile, candidate Compose override, scripts and CI.

- [ ] Add Rust/shader builder and Vulkan/OpenCL runtime dependencies.
- [ ] Preserve C reference target/image; create isolated candidate wiring.
- [ ] Update launch/probe/stop semantics and test actual env/device/image routing.
- [ ] Add host/shader/container CI and required-GPU local qualification commands.

Check: Part 6 commands, non-root candidate probe, isolated mock pool + HTTP port.
Never invoke `docker compose up` against the live service to “test packaging.”

## T14 — Measure and bounded optimization

Inputs: Part 5, baseline identity, correct integrated candidate.
Write scope: one named experiment's files, harness fixes and sanitized evidence.

- [ ] Reproduce matched C OpenCL, Rust OpenCL and Vulkan sustained baselines.
- [ ] Run mandatory compilation coexistence matrix and CPU accounting.
- [ ] Attempt justified ranked experiments one at a time; reject disproven ones.
- [ ] Record every attempt and choose best qualified config without hiding losses.

Check: complete conformance before measuring each kernel, repeated paired results
and sustained confirmation. A quota/model block permits no fabricated result.

## T15 — Final hardening and qualification (last)

Inputs: all prior reports, Part 6 and gates.
Write scope: final integration fixes, docs, PROGRESS, ledger and decision log.

- [ ] Review combined diffs/interfaces, production linkage and config migration.
- [ ] Run full host/build/shader/GPU/mock integration suite and >=30 minute soak.
- [ ] Update README/architecture/hardware/changelog/examples and final report.
- [ ] Record G4/G5 honestly, plus concrete cutover/rollback commands for review.

Check: no skipped mandatory GPU cases counted as passing, no fake measurements,
no leftover production C-host dependency, all known limitations stated.
Do not merge, push, deploy, delete C reference or change the live default merely
because the implementation task finished.

## Bounded concurrency

Default: one implementation worker at a time. After T04 freezes interfaces,
T09 and T10 can run as two independent CPU-only workers with disjoint scopes.
The orchestrator may keep them sequential to preserve laptop headroom. Do not
parallelize GPU work, overlapping edits, root Cargo/lockfile edits, or shared
contract edits. Orchestrator integrates all dependency/lockfile changes.
