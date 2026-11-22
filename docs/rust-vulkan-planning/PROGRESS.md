# Rewrite Progress and Resume State

## Current inspection — 2026-10-18 (planning only)

The historical opening state below is stale. Use
[the current rewrite plan](12-one-shot-rewrite-plan.md) for the R0–R9 execution
sequence and 350 MH/s campaign. Existing Rust core, real Vulkan batches/shader
and miner config are present; production Rust service and packaging are missing.
T06 is partially implemented despite its old pending label. Do not restart it.

Fresh verification: 25 core tests and 8 config tests passed; locked workspace
all-target check passed; 12 required Vulkan conformance tests passed on Intel
8086:9a49 / Mesa 26.2.4-arch3.1 with one test thread. Existing test coverage is
limited: no new validation-layer, hashrate, container or production gate passed.
Host Compose is unavailable. No implementation, deployment, commit or push was
performed in this planning turn. See the dated plan for concrete code/test gaps.

Next execution action: R0 reconcile/preserve, then R1 config/device/harness fixes.
All pre-existing dirty and untracked implementation files were preserved.

## Historical progress (retained)

## Current state

- Last update: 2026-10-17 planning revision.
- Scope of completed work: reviewed/reworked planning documents only.
- Observed branch: `igpu-vulkan`; recheck at execution time.
- Pre-existing state: planning folder untracked; no tracked changes shown by the
  initial status inspection. No staging/commit/push performed.
- Source remains the C miner; Vulkan is still a placeholder.
- No Rust workspace, rewrite worker, hardware benchmark or service deployment
  was created/run by this planning task.
- Main model for execution: DeepSeek V4.1 Flash; free ID used by default launch.
- Implementation/review workers: `opencode/beam-free`, effort `max`.
- OpenCode 1.19.0 CLI/catalog inspected; Beam/max present. Availability of an actual
  completion and remaining quota are NOT verified by listing the catalog.
- Earlier Muse/xhigh and Go fallback policy superseded by Beam/max.
- Next task: T01 recon + compatibility inventory, then T02 baseline/harness and
  T03 scaffold. Launch instructions are in LAUNCH-PROMPT and Part 11.

## Task board

| Task | State | Evidence / remaining |
| --- | --- | --- |
| T01 Recon/inventory | done | docs/RUST-COMPATIBILITY.md created; make test exit 0; worker Beam max session oc-gbtc-t01-recon-beam-free-max-01 exit 0; no runtime changes || T02 Harness/baseline | done | bench scripts hardened; tests/test_bench_scripts.sh exit 0; make test exit 0; no real GPU run (pending) || T03 Workspace | done | rust workspace + gbtc-core smoke tests; cargo test and --locked exit 0 |
| T04 Core | done | gbtc-core shared types/hashes/targets/work; cargo test --locked passes (25 tests) |
| T05 Vulkan foundation | done | gbtc-backend-vulkan crate; probe/dispatch tests pass; spirv-val vulkan1.1 pass |
| T06 Vulkan batches | pending | implementation not started |
| T07 Vulkan conformance | done | 12 required-GPU tests pass with --test-threads=1; clippy -D warnings clean |
| T08 Config/Rust OpenCL | partial | gbtc-miner config crate done (8 tests); OpenCL backend still pending |
| T09 Stratum | pending | implementation not started |
| T10 HTTP/UI | pending | implementation not started |
| T11 Controller | pending | implementation not started |
| T12 Binary/lifecycle | pending | implementation not started |
| T13 Packaging/CI | pending | implementation not started |
| T14 Measurements | pending | implementation not started |
| T15 Final qualification | pending | implementation not started |

All G0–G5 gates are pending. A completed plan is not a completed rewrite.

## Append after each accepted worker step

```text
UTC timestamp:
Task/checkbox and actual worker model/effort/session:
Observed branch/base; initial vs new dirty paths:
Changed files and shared-interface changes:
Commands/exits and observed checks:
Device/image/driver and workload if relevant:
Gate/ledger updates:
Partial edits, failures or quota state to preserve:
Exact next action and required inputs:
```

Store raw worker events privately outside Git. Link sanitized evidence here.
Do not paste credentials or private live responses into resume notes.

## Planning verification (2026-10-17)

- Reviewed all 12 original planning files against current source/tests and build
  wiring; revised them and added contracts, task sequence, model policy and resume
  state (16 documents total).
- Local Markdown links, code-fence balance, final newlines and trailing whitespace
  checked across all 16 documents: passed.
- Verified ordered unique T01–T15 task headings and model policy in launch/index/
  conventions/progress documents: passed.
- `git diff --check`: exit 0; folder remains untracked, so the separate content
  checks above cover its files directly.
- Only the planning folder changed in repository status. No runtime tests were
  run for these documentation-only edits; no model completion pilot was launched.

## Accepted worker step — 2026-10-17 UTC

UTC timestamp: 2026-10-17 (task execution)
Task/checkbox: T01 recon + compatibility inventory (all checkboxes)
Actual worker model / effort / session: opencode/beam-free / max / oc-gbtc-t01-recon-beam-free-max-01 (result dir /tmp/opencode/gbtc-results/oc-gbtc-t01-recon-beam-free-max-01)
Observed branch/base: igpu-vulkan / a12938f230a3254cdae6d0accb829ccedfcf4a5c
Initial vs new dirty paths: only untracked docs/rust-vulkan-planning/ initially; new untracked docs/RUST-COMPATIBILITY.md
Changed files and shared-interface changes: docs/RUST-COMPATIBILITY.md (new, 332 lines). No source or PROGRESS interface changes.
Commands/exits and observed checks:
- worker exit code 0, final step_finish reason stop, no error events
- make test -> exit 0
- git diff --check (tracked) -> no output
- git status --short -> only untracked planning docs + RUST-COMPATIBILITY.md
Device/image/driver: no device test run in T01; inventory records vulkaninfo Intel Iris Xe TGL GT2 / Mesa 26.2.4-arch3.1 and OpenCL NEO device from make test.
Gate/ledger updates: G0 still pending until T03; T01 documentation complete.
Partial edits, failures or quota state to preserve: none; worker completed normally.
Exact next action and required inputs: T02 baseline/benchmark harness (bench scripts/tests) using docs/RUST-COMPATIBILITY.md and Part 2; no GPU run unless authorized.

## Accepted worker step — 2026-10-17 UTC (T02)

UTC timestamp: 2026-10-17
Task/checkbox: T02 safe baseline and benchmark harness (all checkboxes)
Actual worker model / effort / session: opencode/beam-free / max / oc-gbtc-t02-bench-harness-beam-free-max-01
Observed branch/base: igpu-vulkan / a12938f230a3254cdae6d0accb829ccedfcf4a5c
Initial vs new dirty paths: modified scripts/bench-run.sh, scripts/bench-ab.sh, scripts/bench-current.sh; new tests/test_bench_scripts.sh and docs/CEILING.md.
Changed files and shared-interface changes: bench harness only; no source changes.
Commands/exits:
- bash -n scripts/*.sh tests/test_bench_scripts.sh -> 0
- bash tests/test_bench_scripts.sh -> 0
- make test -> 0
Device/image/driver: no real GPU/docker run; measurement pending per docs/CEILING.md.
Gate/ledger updates: G1 baseline evidence still pending/blocked for real numbers; harness now trustworthy.
Partial edits, failures or quota state to preserve: none.
Exact next action and required inputs: T03 workspace scaffold; no GPU run required.

## Accepted worker step — 2026-10-17 UTC (T03)

UTC timestamp: 2026-10-17
Task/checkbox: T03 workspace and reproducible build skeleton (all checkboxes)
Actual worker model / effort / session: opencode/beam-free / max / oc-gbtc-t03-workspace-beam-free-max-01
Observed branch/base: igpu-vulkan / a12938f230a3254cdae6d0accb829ccedfcf4a5c
Changed files: rust/Cargo.toml, rust/rust-toolchain.toml, rust/gbtc-core/Cargo.toml, rust/gbtc-core/src/lib.rs, rust/gbtc-core/tests/smoke.rs, rust/Cargo.lock; .gitignore updated to ignore rust/target/.
Commands/exits:
- cargo test --manifest-path rust/Cargo.toml -p gbtc-core -> 0
- cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-core -> 0
Device/image/driver: none.
Gate/ledger updates: G0 still pending until integration; T03 complete.
Partial edits, failures or quota state to preserve: none.
Exact next action and required inputs: T04 core hashes/targets/work/shared types.

## Accepted worker step — 2026-10-17 UTC (T04)

UTC timestamp: 2026-10-17
Task/checkbox: T04 core hashes, targets, work and shared types (all checkboxes)
Actual worker model / effort / session: opencode/beam-free / max / oc-gbtc-t04-core-beam-free-max-01
Observed branch/base: igpu-vulkan / a12938f230a3254cdae6d0accb829ccedfcf4a5c
Changed files: rust/gbtc-core/src/{lib.rs,backend.rs,sha256.rs,work.rs}; rust/gbtc-core/Cargo.toml; rust/gbtc-core/tests/core_oracle.rs; rust/Cargo.lock.
Commands/exits:
- cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-core -> 0 (25 tests)
Device/image/driver: none.
Shared interfaces frozen: WorkBatch, BatchResult, ComputeBackend, BackendError, BackendInfo, JobSnapshot, NonceRange/Cursor, target/difficulty conversion, work_hash, scan helpers.
Gate/ledger updates: G0 progress; core host tests green.
Partial edits, failures or quota state to preserve: none.
Exact next action and required inputs: T05 Vulkan foundation (requires GPU/shader compiler); host-only crate checks can proceed. T09/T10 may later run as disjoint CPU-only workers after interfaces frozen.

## Accepted worker step — 2026-10-17 UTC (T05)

UTC timestamp: 2026-10-17
Task/checkbox: T05 Vulkan probe and resource foundation (all checkboxes)
Actual worker model / effort / session: opencode/beam-free / max / oc-gbtc-t05-vulkan-foundation-beam-free-max-01
Observed branch/base: igpu-vulkan / a12938f230a3254cdae6d0accb829ccedfcf4a5c
Changed files: rust/Cargo.toml; rust/gbtc-backend-vulkan/*; rust/shaders/sha256d.comp.
Commands/exits:
- cargo test --locked -p gbtc-backend-vulkan -> 0 (8 tests)
- GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked -p gbtc-backend-vulkan -> 0
- spirv-val --target-env vulkan1.1 on generated spv -> 0
GPU: Iris Xe (8086:9a49), Mesa 26.2.4; probe + 1000-element dispatch verified.
Gate/ledger updates: G2 progress (foundation).
Partial edits, failures or quota state to preserve: none.
Exact next action and required inputs: T06 Vulkan SHA-256d batch path.

## Accepted worker step — 2026-10-17 UTC (T07)

UTC timestamp: 2026-10-17
Task/checkbox: T07 required Vulkan conformance (all checkboxes)
Actual worker/model/effort: orchestrator direct cleanup (clippy/fmt) + prior worker results; no new worker.
Commands/exits:
- cargo fmt --check -> 0
- cargo clippy --locked --workspace --all-targets -- -D warnings -> 0
- GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked -p gbtc-backend-vulkan --test conformance -- --test-threads=1 -> 0 (12 tests)
GPU: Intel Iris Xe (8086:9a49), Mesa 26.2.4.
Notes: validation layer unavailable on host; GBTC_VULKAN_VALIDATION=1 path returns explicit error.
Gate/ledger updates: G2 progress.
Exact next action: T08 config + Rust OpenCL.

## Accepted worker step — 2026-10-17 UTC (T08a)

UTC timestamp: 2026-10-17
Task/checkbox: T08a config/miner crate (host-only)
Actual worker model / effort / session: opencode-go/beam-free / max / oc-gbtc-t08a-config-beam-free-max-01
Changed files: rust/Cargo.toml, rust/gbtc-miner/*.
Commands/exits:
- cargo test --locked -p gbtc-miner -> 0 (8 tests)
- cargo fmt --check -> 0
- cargo clippy --locked --workspace --all-targets -- -D warnings -> 0
- make test -> 0
Limits: OpenCL backend not yet implemented; config is host-only.
Exact next action: T08b OpenCL backend or T09 Stratum (interfaces frozen).

## Implementation started — 2026-10-18

- User authorized execution of R0–R9; OpenCode Free may be used until exhausted,
  then Codex may finish directly. OpenCode Go/paid fallback is unavailable.
- Working in place on `igpu-vulkan` / `a12938f`; preserving all existing dirty
  scripts, `.gitignore`, untracked Rust and planning files.
- Baseline `gamble-btc:latest` and `gamble-btc:ocl-cand-pre3` both identify
  image `24805ee25283`. No live Docker containers at initial check.
- `make test`: exit 0, including NEO OpenCL conformance.
- `bash tests/test_bench_scripts.sh`: exit 0.
- Beam/max R1 writer started in owned tmux session
  `oc-gbtc-r1-beam-free-max-01`; private results under
  `/tmp/gbtc-rewrite-20261018/r1`. No Go fallback.

| Step | Current state |
| --- | --- |
| R0 | baseline preserved; checks in progress |
| R1 | worker implementing config/device/harness fixes |
| R2 | pending |
| R3 | pending |
| R4 | pending |
| R5 | pending |
| R6 | pending |
| R7 | pending |
| R8 | pending |
| R9 | pending |

### R0/R1 accepted — 2026-10-18

- Beam/max R1 exit 0 with nonempty final response and terminal `stop`.
- Independent checks: config 14 tests passed; device-selection 8 tests passed;
  workspace all-target check passed; mocked bench script tests passed.
- Wallet-free modes, mutually-exclusive mode error, Vulkan numeric validation,
  render-node identity and integrated-only device selection implemented.
- Bench script injects GLES settings only for GLES. Main moved script tests to
  a task-owned copied tree, preventing deletion of real benchmark artifacts.
- R2 GPU lifecycle/conformance and R3 core/Stratum workers started with disjoint
  scopes. Main added Stratum member/serde_json and Vulkan oracle dev dependency,
  resolving Cargo.lock. All GPU work remains serialized.
- Remaining R1 integration: local-size pipeline wiring is in R2.

### R2/R3/R4 integration review — 2026-10-18

- R2 worker completed 17 Vulkan conformance cases and local-size specialization.
  Main review caught an invalid validation-callback pointer and incomplete
  partial resource cleanup. Added stable synchronized callback storage, staged
  initialization ownership, six injected failure stages, and mapped-allocation
  fixes. New regression failed before repair; required host GPU suite passed.
- Debug container `gamble-btc:rust-debug-base` built. Four backend unit tests
  (including initialization failure cleanup) and 17 conformance cases passed
  inside it with `GBTC_VULKAN_VALIDATION=1` and no real validation errors.
  One deliberate synthetic callback message is expected in its unit test.
- R3 initial 11 mock-pool tests passed. Review required bounded delivery,
  handshake/submission deadlines and strict malformed input. Repair worker
  completed with tests/clippy passing; main distinguished actual submitted,
  matched replies, local discards and unresolved disconnects in Event API.
- R4 HTTP/UI implemented. Main added regressions for malformed HTTP versions
  and stop during partial headers; seven HTTP/SSE tests pass. Socket writes
  no longer hold the shared status lock; client handles drain at shutdown.
- Main clippy for Vulkan/Stratum/HTTP all-targets passed before final integration.
- R5 Rust OpenCL and runtime/controller workers launched in disjoint scopes.
- Docker default bridge creation fails on this host (veth operation unsupported);
  use network-none probes and explicit loopback/host-network tests. No host edits.
- Note: inherited benchmark-script tests removed bench-results on their first
  run; changed tests now copy scripts into a task-owned temp tree. Historical
  ignored benchmark artifacts are not relied on for new measurements.

### R5/R6 and shader integration — 2026-10-18

- Runtime repair Free worker hit HTTP400 prompt_rendering_failed; a fresh scoped
  Free worker completed the repair. Main independently reviewed and added RAII
  service joins, watchdog regressions and finalnonce controller coverage.
- OpenCL worker repeatedly stalled. Main stopped only its owned tmux/process
  tree and found a nonadvancing string-trim loop, undersized extension string
  query and a dual-kernel variable collision. Repairs now pass all eight required
  OpenCL conformance tests on the Iris Xe/host NEO.
- Main added async output read completion, sleeping polling, transfer storage
  retained on every error, bounded queue teardown and strict requested identity.
- Required host Vulkan tests passed for looped/unrolled/pre3/pruned/boolean:
  each four backend tests, 17 conformance tests and eight device-selection tests.
- End-to-end local synthetic pool mining with Vulkan passed 35seconds, job
  changes/status/SSE, exit0, SIGTERM shutdown0.514s. No share observed in this
  short run; it is smoke evidence, not share or speed qualification.
- Beam shader worker generated four variants. Main wired explicit selection,
  88byte push constants and CPU prefix3. Host ANV native disassembly is available:
  looped spills34/fills10; unrolled and pre3 zero spills. Pruned emits the same
  instruction statistics as unrolled, so ANV already removes the unused tail.
- Separate candidate Dockerfile/Compose, CI and direct launcher added. Production
  image build in progress; original C image/tags preserved.

### Independent review adjudication — 2026-10-18

- Beam Free read-only reviewer exited0 with findings. Main checked each against
  current code before changes. OpenCL and Vulkan both reject poisoned contexts
  at the start of dispatch; repeated cancellation cannot overwrite pending host
  storage or reuse the in-flight slot as suggested. SSE slot lifetime is bounded
  in count and intentionally persistent in time for legitimate readers.
- Nonce swap is deliberate: SHA candidate coordinates are big-endian message
  words, while Stratum submits the numeric little-endian header nonce. The C
  formatting is not an oracle for correct wire serialization. Independent local
  pool hashing remains the end-to-end acceptance evidence.
- Repeated notify generations no longer advance for identical retained work.
  Main synchronized protocol/controller handling, preserved all header fields in
  identity, and raised the checked push-constant minimum from56 to88bytes.
- Full host tests/release/image rebuild are deferred until the serialized
  screening run ends, avoiding compilation contamination from this task.
- New external interference observed: `jcalc` is using the same iGPU and PL1 is
  active. Current screens are shared-load exploratory evidence, not quiet gates.
  User was asked whether they can pause their workload; it has not been stopped.

### Scope update from user — 2026-10-18

- User explicitly prioritizes completing the full rewrite over hashrate work
  while another agent runs the more important jcalc project. Leave that workload
  running. No signal was sent to it, including between the earlier stop request
  and its revocation.
- Defer quiet paired trials, remaining local-size/poll tuning, long speed winner
  confirmation and compilation coexistence measurements. Do not claim those
  gates passed from the contaminated screens.
- Final functional checks use the Rust-only production/debug builds, all shader
  conformance and bounded local pool integration/reconnect/share verification.
  Extended30minute soak is documented as a remaining qualification gate.

### Final functional delivery — 2026-10-18

- R0–R6 implemented: independent Rust host, core, Vulkan, Stratum, HTTP,
  OpenCL fallback and controller/lifecycle integration.
- R7 has shared-load exploration only. R8 shader variants are implemented;
  further tuning remains deferred under the latest user instruction.
- R9 functional verification passed: 127 host tests; all five Vulkan variants
  passed container conformance with validation; Rust OpenCL conformance passed.
  Final production Vulkan local-pool integration independently accepted 4/4
  shares across two connections, with no invalid, duplicate or stale shares.
  Debug Vulkan and production OpenCL each independently accepted 1/1 shares.
- Production/debug images built from Rust-only inputs; C reference/latest image
  preserved. No deployment or host configuration changes were made.
- See `docs/QUALIFICATION.md` for exact identities, stack, evidence and limits.
  Sustained performance, compilation coexistence and extended soak gates remain
  deferred; no 350 MH/s or C performance comparison is claimed.
