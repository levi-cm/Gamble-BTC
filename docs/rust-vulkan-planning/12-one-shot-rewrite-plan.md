# Rust/Vulkan Rewrite Implementation Plan — 350 MH/s Campaign

Date: 2026-10-18. Scope: finish the full container rewrite in one sustained agent
run, preserving the partial implementation. This document is the current entry
point and takes precedence over stale state, task ordering, and the older
400 MH/s target in Parts 0–11. Those files remain useful supporting references.

> **For agentic workers:** use the executing-plans skill to work sequentially
> through the checkboxes. Use saved progress across context refreshes. Existing
> worker orchestration in Part 11 applies only when delegation is requested;
> model availability must be checked and no model may be silently substituted.

**Goal:** ship a C-independent Rust miner container with real Vulkan nonce search
on this laptop's Iris Xe, preserve mining/UI/launcher behavior, and measure the
highest sustainable correct hashrate found in a bounded campaign toward 350 MH/s.

**Architecture:** retain `gbtc-core`, repair `gbtc-backend-vulkan`, then add
Stratum, HTTP, controller, executable and container integration. One GPU worker
owns all Vulkan objects; bounded messages carry immutable work snapshots. A Rust
OpenCL compatibility backend preserves the existing NEO comparison/fallback path;
explicit Vulkan selection always runs Vulkan or fails.

**Tech stack:** Rust 1.99.0 and locked Cargo dependencies; `ash` 0.38; GLSL compute
compiled to SPIR-V with `glslc`; Mesa ANV on Xe-LP; `serde`/`serde_json`, `sha2`
as an independent test oracle; dynamically loaded OpenCL; Debian container.
Use ordinary blocking sockets and a small bounded set of threads initially.
No dependency on an async executor is needed for this workload.

## 1. Investigation and current evidence

Inspected branch `igpu-vulkan`, HEAD `a12938f230a3254cdae6d0accb829ccedfcf4a5c`.
There are existing modified bench scripts and `.gitignore`, and untracked Rust,
compatibility, ceiling, planning, and benchmark-test files. Preserve all of them.
Do not reset, clean, recreate the workspace or restart already implemented work.

| Component | Observed implementation | Remaining work |
| --- | --- | --- |
| Production runtime | `src/main.c`, 1845 lines; OpenCL/GLES | Full Rust service and wiring |
| Rust core | SHA, work, target, range and backend types | Coinbase/merkle construction and stronger independent fixtures |
| Vulkan | `src/lib.rs`, 1029 lines; real dispatch, fences, overflow subdivision | Device selection, lifetime/error handling, configuration, stronger conformance, optimized shader |
| Shader | `rust/shaders/sha256d.comp`, 111 lines | Looped schedule with `wv[64]`, fixed local size 64; optimize after qualification |
| Rust miner | Config library, eight integration tests | No `main.rs`, controller, Stratum, HTTP or bench service |
| Container | C build; NEO and GL/OpenCL runtime | Rust build stage, ANV Vulkan ICD and loader, isolated candidate wiring |
| Prior progress | Top says no Rust; table says T06 pending and T07 done | Reconcile against code/checks; no gate inherited from a label |

Live inspection found Linux `7.2.6-1-cachyos`, Intel `8086:9a49`,
`/dev/dri/card1` and `/dev/dri/renderD128`; Vulkan 1.4.354 on Mesa
26.2.4-arch3.1. Host `docker compose` is unavailable. `docker ps` listed no running
containers at inspection time; check again before any benchmark. Host validation
layers do not include `VK_LAYER_KHRONOS_validation`.

Fresh checks during this investigation:

- `cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-core -p gbtc-miner
  --quiet`: passed, 25 core tests and 8 config tests.
- `cargo check --locked --manifest-path rust/Cargo.toml --workspace --all-targets
  --quiet`: passed.
- `GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked --manifest-path rust/Cargo.toml
  -p gbtc-backend-vulkan --test conformance -- --test-threads=1 --nocapture`:
  passed all 12 tests on the real GPU. This is limited existing coverage;
  it does not establish the missing cases or validation-layer cleanliness.
- No new hashrate measurement, production container build or deployment.

Concrete defects/gaps to address before optimization:

1. `device::select_index` accepts names/IDs, not `/dev/dri/renderD128`, despite
   that being the config default. It also permits discrete/virtual fallback and
   contains an unused placeholder queue filter. Select the requested iGPU exactly.
2. Config requires `BTC_ADDRESS` before checking probe/benchmark mode. Neither
   synthetic mode should need a wallet, pool connection, or HTTP listener.
3. `scripts/bench-run.sh` injects nonempty GLES kernel/local-size settings for
   every backend. Rust correctly rejects them. Remove those injections for Rust.
4. Vulkan local size is hardcoded to 64; the advertised setting is not wired.
   Poll values are reparsed/clamped inside the backend instead of validated once.
5. Existing GPU tests use the project's own hash helper as their candidate
   oracle. Range-edge coverage can return early for a missing pivot even in
   required mode; the partial-init test only tests an invalid device request.
6. Timeout paths leave work potentially pending; `Drop` calls unbounded
   `device_wait_idle`. Establish a safe failure/shutdown policy before the service.
7. The controller and protocol generations are not implemented. Backend complete
   candidates are only a word prefilter, never proof of a valid share.

## 2. What can plausibly produce 350 MH/s

C is already a low-level language. Rust changes ownership and maintainability;
GPU machine code and scheduling determine mining speed. Vulkan exposes explicit
submission/synchronization and uses ANV's compiler instead of NEO/IGC, but may
produce slower code. Treat that compiler change as an experiment.

`docs/HARDWARE_MATRIX.md` records 184.022 MH/s at 1.78% container CPU and
186.634–193.994 MH/s full-clock spot samples. These are historical records, not a
fresh matched baseline. Reaching 350 requires roughly 1.80–1.90 times that rate,
or about 45–47% less time per nonce at comparable clocks. Host-language changes,
buffer-copy removal, and tiny launch savings cannot plausibly supply that alone.

At 1300 MHz, 350 MH/s means about 0.269 aggregate hashes per GT clock across the
whole GPU. This ratio is a measurement target, not a per-EU instruction budget or
a theoretical ceiling. Measure EU topology, issued instruction mix, register
pressure, and effective clocks before deriving a throughput model. Bitcoin's
fixed first 64-byte block can be represented by a cached midstate: nonce work is
two SHA-256 compressions, not three fresh compressions per nonce.

Three approaches considered:

| Approach | Benefit | Cost / decision |
| --- | --- | --- |
| Finish existing Rust + raw Vulkan, then optimize shader | Fits requested rewrite, preserves progress, directly tests ANV | Recommended; needs careful unsafe ownership and shader qualification |
| Rewrite Rust host around NEO OpenCL only | Lowest performance risk, retains proven compiler path | Keep as compatibility/control experiment; does not satisfy Vulkan objective alone |
| Native Xe assembly/custom driver submission | Potentially greatest machine-code control | Separate research; unsupported assumptions and much larger validation burden; outside this bounded rewrite |

No CPU mining, dedicated GPU fallback, power-policy changes, overclocking,
driver replacement on the host, or compilation throttling. An ANV win cannot be
assumed from historical OpenCL occupancy or ISA: measure both compiler paths.

## 3. Files and shared contracts

Retain existing names and public core types where correct. Add files only for
clear responsibilities; move code out of the Vulkan monolith as it is repaired.

| Files | Responsibility |
| --- | --- |
| `rust/gbtc-core/src/{backend,sha256,work}.rs`, new `job.rs` | Work snapshots, exact targets, coinbase/merkle/header bytes, nonce cursor |
| `rust/gbtc-backend-vulkan/src/{lib,device}.rs`, new `buffer.rs`, `pipeline.rs`, `batch.rs` | Selected device, RAII ownership, memory, dispatch/completion and overflow |
| `rust/shaders/sha256d.comp`, new `generate_sha256d.py` | Reference shader and deterministic optimized scalar-source generation |
| `rust/gbtc-backend-vulkan/build.rs` | Pinned compiler arguments, variant generation and SPIR-V validation |
| New `rust/gbtc-backend-opencl/` | Rust NEO binding, existing kernels/settings, complete-candidate semantics |
| New `rust/gbtc-stratum/src/{lib,protocol,client}.rs` | Bounded protocol parsing, socket lifecycle, request IDs, job generations |
| New `rust/gbtc-http/src/{lib,status,server}.rs`, `assets/index.html` | Existing dashboard, JSON metrics, SSE and HTTP |
| `rust/gbtc-miner/src/config.rs`, new `controller.rs`, `bench.rs`, `main.rs` | Validated config, application state, unique hash timing, modes/signals |
| `Dockerfile.rust`, `compose.rust.yaml`, Makefile and `.github/workflows/` | Isolated Rust build, runtime, host CI and opt-in device checks |
| Bench/start scripts and their existing tests | Backend-aware env forwarding, finite owned lifecycle, launcher parity |
| New integration tests under the corresponding crates | Mock pool, real sockets, generations, shutdown, selected backend |
| `docs/rust-vulkan-planning/PROGRESS.md`, new `QUALIFICATION.md` | Evidence, remaining checkboxes, result and rollback decision |

Semantic rules (Part 9 supplies the detailed compatibility inventory):

- `WorkBatch` uses `u32` wrapping SHA arithmetic, an immutable midstate/tail,
  target and finite nonce range. Range arithmetic uses `u64`; end may equal 2^32.
- `BatchResult.candidates` means COMPLETE prefilter matches for that range;
  compare full SHA-256d with all target words on the host before submission.
  Validate candidate range and deduplicate by connection/job/extranonce/ntime/nonce.
- Counter capacity overflow causes GPU subdivision/rescan of the same snapshot.
  Count successful unique coverage once; record rescans/extra GPU work separately.
- Retain separate connection, job and target generations. Non-clean jobs remain
  eligible only according to explicit pool semantics; clean jobs invalidate old
  work; difficulty changes apply at the protocol-defined job boundary. Keep the
  target used for each job/share immutable; never read a global current target
  to reinterpret a completed batch.
- Only the GPU owner thread touches its Vulkan handles. Introduce cancellation
  without releasing or reusing pending GPU resources. Do not add unsafe `Send`
  implementations to bypass ownership errors; construct resources in that thread.
- Fixed bounded queue defaults: latest-work mailbox 1, submissions 128, active
  jobs 64, protocol frame 64 KiB, HTTP headers 16 KiB, HTTP clients 16. Reject or
  disconnect on over-limit input; report queue-full/stale discard explicitly.
  Keep 300 one-second metric samples; slow SSE clients get a bounded latest
  snapshot and eventual disconnect, never an unbounded backlog.
- Preserve runtime port 41174, status keys/history and SSE framing. Default to
  loopback. Candidate container binds `0.0.0.0` internally with host loopback
  publishing on a distinct test port. Preserve existing UI rather than redesign it.
- Rust `auto` order is explicitly `vulkan -> opencl`; explicit backends do not
  substitute. Document this change from C's `gles -> opencl -> vulkan` policy.
  Rust production entrypoint must not link or shell out to the C miner.

## 4. Ordered execution checklist

One-shot means one launch that continues through small checked increments,
including resumable progress; it does not mean one unreviewed giant patch.
Complete host work while hardware is unavailable, but keep device gates pending.

### R0 — Reconcile and preserve the starting point

Files: progress/index, compatibility inventory, immutable local evidence.

- [ ] Record current branch/HEAD, all initial dirty paths and available tools.
  Recheck `/dev/dri`, GIDs, AC state, running miners and immutable image IDs.
  Do not print `.env`, wallet or pool credentials.
- [ ] Reconcile old T01–T08 claims with actual files and rerun their checks.
  T06 is partially implemented; T07's twelve passing tests are limited coverage.
  Add this plan's R0–R9 checklist to progress; preserve historical append entries.
- [ ] Run `make test`, `bash tests/test_bench_scripts.sh`, and the three Cargo
  checks in section 1. Serialize GPU checks and record exact device/driver.
- [ ] Preserve a usable C OpenCL image by immutable ID and candidate tag naming.
  Missing Compose is a packaging check to solve in container/CI, not permission
  to install host packages or to claim Compose passed.

### R1 — Repair configuration, identity and benchmark routing

Files: Rust config, Vulkan device module, bench scripts and matching tests.

- [ ] Add failing tests for wallet-free probe/bench, mutually exclusive modes,
  invalid poll/local-size, explicit backend failure, and retired GLES settings.
  Parse mode first; require the address only for actual mining.
- [ ] Match render-node major/minor through `VK_EXT_physical_device_drm` when
  available; otherwise verify PCI identity through sysfs and Vulkan properties.
  Test two same-name devices, wrong node, CPU/discrete devices and valid default.
  Missing reliable identity is an explicit failure, not a name-match fallback.
- [ ] Parse Vulkan poll range 100–50000 microseconds once. Accept local sizes
  8/16/32/64/128/256 only if limits support them; specialize or compile pipeline
  accordingly. Remove unused placeholder queue code.
- [ ] Forward GLES settings only to C/GLES runs, use backend-specific config,
  preserve explicit image/backend/DRM mappings, and extend mocked script tests.
  Synthetic modes use neither production `.env` nor network access.
- [ ] Pass config/device unit tests, `bash tests/test_bench_scripts.sh`, and
  `bash -n scripts/bench-run.sh scripts/bench-ab.sh scripts/bench-current.sh`.

### R2 — Qualify Vulkan ownership and complete hash results

Files: Vulkan modules/tests, core oracle tests, reference shader/build script.

- [ ] Keep a correct single-slot pipeline. Assert the 56-byte push-constant
  layout and candidate-buffer layout; validate every supported local size.
  Preserve transfer reset -> compute -> host barriers and fence completion;
  invalidate noncoherent ranges only after completion, within mapped allocation.
- [ ] Add staged RAII guards for every fallible initialization operation.
  Test failure after buffer, mapping, descriptors, pipeline and fence creation.
  Map device loss separately from timeout. A timeout poisons the slot/backend:
  never reset/free/reuse work still executing. For an unrecoverable GPU hang,
  terminate the candidate process within the application deadline and let process
  teardown reclaim resources; do not promise a bounded `vkDeviceWaitIdle`.
- [ ] Add cancellation checks between finite chunks and fence polls. Stop new
  submissions immediately; drain normal in-flight work safely. Bound ordinary
  shutdown to five seconds, with explicit fatal process exit on an unresponsive
  driver. Test using an injected stalled completion source.
- [ ] Build expected complete candidate sets from independent `sha2` over actual
  80-byte headers, not only the in-tree compression helper. Include nonce byte
  order, target equality, seeded random inputs, partial groups, count 1/0 policy,
  final `0xffffffff`, reset, forced overflow union and out-of-range rejection.
  Make range-edge equality deterministic: choose the final nonce's target word;
  never skip because random search found no pivot.
- [ ] Run GPU tests in required mode with one test thread; fail any mandatory
  skipped case. Run synchronization/core validation in a debug container with
  `VK_LAYER_KHRONOS_validation`, debug messenger capture, and zero errors.
  No host package changes needed. Validate SPIR-V with `spirv-val`.

### R3 — Complete work construction and Stratum

Files: core `job.rs`, new Stratum crate, known-byte fixtures, mock pool tests.

- [ ] Port coinbase/extranonce, coinbase double hash, merkle folding, prevhash
  layout, version/ntime/nbits/nonce serialization. Use explicit byte fixtures
  derived from synthetic jobs and independent hashes. Bound merkle depth and
  reject invalid hex, lengths, difficulty, NaN/infinity and arithmetic overflow.
- [ ] Implement subscribe/authorize/notify/difficulty handling, bounded newline
  framing, partial reads/writes, JSON escaping, request-ID tracking and exact
  submit tuple. A boolean reply must be interpreted in its request context.
- [ ] Implement disconnect/reconnect with bounded exponential backoff, fresh
  connection generation, authorized state and invalidation of old pending submits.
  Test notify before auth, interleaved replies, rejected auth/share, malformed
  frames, changed extranonce, clean/non-clean jobs and difficulty races.
- [ ] Run core and Stratum tests without GPU or internet. Mock pool uses
  synthetic identifiers; assertions compare exact 80-byte headers and submits.

### R4 — Port status/UI and wire the mining controller

Files: HTTP crate/assets; miner `controller.rs`; config and lifecycle tests.

- [ ] Extract the existing UI, port status schema from `build_stats_json`, and
  preserve GET/HEAD/query-string/OPTIONS/400/404/405/favicon behavior and SSE
  initial-history/incremental updates. Verify with real local socket tests.
- [ ] Use bounded client counts, read/write deadlines, reusable serialized
  snapshots and sleeping workers. Test slow SSE readers and abrupt disconnects;
  escape JSON and retain safe DOM `textContent` updates.
- [ ] Wire the single GPU owner to immutable jobs and a `u64` nonce cursor.
  On exhaustion increment extranonce2 and rebuild work; do not silently wrap
  nonce space or mutate a pending snapshot. Roll ntime only under documented
  protocol policy. Exhausted extranonce space waits for fresh work.
- [ ] CPU-verify only returned candidates, apply full target and job eligibility,
  then enqueue deduplicated submits. Mock a backend completing after clean job,
  non-clean job, disconnect, target update, shutdown and queue saturation.
- [ ] Pass real HTTP/SSE tests and deterministic controller race tests. Record a
  sanitized dashboard screenshot during later integrated container qualification.

### R5 — Finish binary, benchmark and Rust OpenCL control path

Files: miner `main.rs`, `bench.rs`, OpenCL crate, workspace manifest/lockfile.

- [ ] Add mode dispatch, signal handling and explicit backend diagnostics.
  Probe performs no hash search; benchmark uses synthetic work without HTTP,
  pool connection or credentials. Assert network-free behavior with tests.
- [ ] Preserve benchmark JSON fields expected by the scripts; add backend,
  immutable config, completed unique hashes, actual measured monotonic seconds,
  rescan counts and optional GPU timestamp duration. Count no failed/pending batch.
  Repeated synthetic sweeps may reuse a range, but count each completed sweep
  once; never multiply by rounds, workgroup width or SHA compressions.
- [ ] Implement Rust OpenCL loading/device selection/event sleep-poll and port
  existing unrolled/pre3 kernels first; retain other public variant settings
  where supported. Use the same core batch/result contract and independent
  conformance suite. No production C FFI, C executable or CPU nonce search.
- [ ] Run workspace fmt/clippy/tests, required backend tests and a short explicit
  Vulkan synthetic run. Compare independent hash vectors before timing anything.

### R6 — Build the complete isolated Rust container

Files: `Dockerfile.rust`, `compose.rust.yaml`, Makefile, launch scripts, CI.

- [ ] Use a pinned Rust builder/toolchain and Cargo lockfile, copy only Rust and
  shader build inputs into the production build stage, and build release mode
  with `--locked`. Include glslc/SPIR-V validation in the builder. Pin external
  dependency versions/checksums and record resolved Mesa/NEO package versions.
- [ ] Runtime installs `libvulkan1`, Intel ANV via `mesa-vulkan-drivers`, DRM
  libraries and the Rust OpenCL fallback's loader/NEO dependencies. GL `iris`
  overrides are not Vulkan ICD selection. Reject software Vulkan devices.
  Keep validation/tools in a debug target, not required by production.
- [ ] Run non-root with actual device permissions and runtime supplemental GIDs.
  Candidate image `gamble-btc:rust-vulkan-candidate`, container
  `gbtc-rust-qualification`, loopback host port 41175 -> container 41174;
  no injected GLES defaults, no privileged container. Preserve AC launcher guard.
- [ ] Add direct Docker build/run test path for this Compose-less host. Validate
  standalone `compose.rust.yaml` with synthetic env in CI where Compose exists.
  Keep the C Dockerfile and `latest` untouched through qualification.
- [ ] Build in a context containing no `src/` C directory. Inspect entrypoint and
  runtime linkage; run no-wallet probe and synthetic benchmark as container user.
  Then run mock pool + dashboard + graceful stop end to end in owned containers.

### R7 — Establish fair measurements before changing the kernel

Files: bench scripts, `docs/CEILING.md`, experiment ledger, qualification report.

- [ ] Recheck no other miner is using the GPU; serialize all device work. Capture
  image IDs, source/shader hashes, compiler/driver versions, devices, work/batch,
  AC state, GT clocks, PL1/thermal flags, process CPU seconds, RSS and timestamps.
  Missing telemetry is `unknown`; do not write sysfs or alter host policy.
- [ ] Measure C OpenCL, Rust OpenCL and Rust Vulkan under matching conditions:
  warmup >=30 s, >=4 balanced pairs, >=60 s measured samples, followed by a
  15-minute confirmation of the winner. Report median, spread and paired deltas.
  Separate cold shader compile/pipeline startup from warmed throughput.
- [ ] Measure a real recorded local compile command alone and with each miner,
  >=3 matched repetitions with balanced cache conditions. Use the same build
  command/parallelism and task-owned build output. If no representative command
  is known, use the repository's Rust release build in a separate target directory
  with its command/cache state recorded; do not claim it represents every workload.
- [ ] CPU accounting sums miner threads/children (or cgroup usage) divided by wall
  seconds. Target <=0.02 cores warmed quiet; 2% means one logical core, not the
  whole eight-thread machine. Report loaded hashrate and build slowdown separately.

### R8 — Bounded Xe-LP shader performance campaign

Files: generated shader variants, build/pipeline selection, ledger.

For each experiment: state hypothesis, change one mechanism, validate complete
candidate sets, inspect compiler output, then benchmark. Keep every attempted
decision including losses. Do not assume OpenCL negative results apply to ANV,
but require a changed mechanism before retrying those ideas.

| Order | Experiment | Evidence / stop rule |
| --- | --- | --- |
| E1 | Generate fully unrolled scalar rounds with a 16-word rolling schedule, constant padding, literal round constants | Compare against current `wv[64]` looped shader; inspect ANV register/spill/ISA evidence where available; retain only measured benefit |
| E2 | Precompute nonce-independent rounds 0–2 of the first remaining compression (`pre3`) | Carry original midstate for feed-forward; port arithmetic from `src/opencl.c`; full byte/hash conformance first |
| E3 | Prune unused second-compression tail for word prefilter | Derive `h64 = e61` before feed-forward; preserve IV addition/byte swap and host full check; reject if ANV already eliminates it |
| E4 | Algebraic CH/MAJ, rotate forms and add reassociation | Inspect actual Xe-LP instructions; no assumed BFN/ADD3/LOP3 support or portable SPIR-V rotate guarantee |
| E5 | Local sizes 8/16/32/64/128/256; optional supported subgroup controls | Query features/limits; pipeline creation must honor selection; compare GRF/spills, effective clocks and wall hashrate |
| E6 | Two nonces/invocation with shared prefix | Try only best scalar baseline first; test both nonce lanes/odd counts; reject spill or latency regression |
| E7 | Batch/poll tuning, then at most two in-flight slots | First measure queue idle gaps; each slot owns immutable state and completion resources; no pipeline complexity without repeatable gain |
| E8 | Isolated container Mesa compiler version comparison | Only with evidence of compiler limitation; keep host driver/power policy unchanged; verify ICD and source identity in each image |

Default budget: E1–E5 screen at most 12 distinct shader configurations total;
advance at most three finalists to full paired measurement. Screen E6–E8 only
when their prerequisites hold, at most four additional configurations. Screening
uses conformance plus 30 s warmup/30 s measurement; screening numbers never
establish a win. A campaign can take hours including builds and final confirmation.
Stop after all justified candidates or two consecutive experiments whose emitted
GPU code is unchanged. Never loop indefinitely trying to force 350 MH/s.

Accept a speed winner only if paired median gain exceeds both 3% and twice
baseline relative MAD, direction agrees in >=3/4 pairs and sustained confirmation
holds. Kernel timestamps explain dispatch cost; application wall hashrate decides.
Report unavailable native ISA tools honestly; SPIR-V is not native ANV ISA.
Do not add speculative percentage gains or call a diagnostic ALU test a ceiling.

### R9 — Final qualification and reviewable cutover package

Files: qualification report, progress/ledger, README/architecture/hardware docs,
changelog/example config; exact candidate and baseline identities.

- [ ] Rerun final fmt/clippy/host tests, meaningful shell tests, SPIR-V validation,
  both required GPU suites, Vulkan debug validation, image build and mock pool/UI
  integration. Run >=30-minute final candidate soak, injecting clean jobs,
  reconnects, target changes, slow SSE readers and SIGTERM.
- [ ] Verify no CPU search, no production dependency on old C sources/executable,
  no invalid/duplicate shares in deterministic mock tests, bounded memory/CPU,
  honest unique-hash accounting, no resource reuse while GPU work is pending.
  A live accepted share is opportunistic evidence, not a statistically practical
  mandatory gate at low solo-mining share rates.
- [ ] Separate outcomes: rewrite correctness complete; Vulkan performance measured;
  350 target achieved/missed/unmeasured; production cutover qualified/pending.
  Target achievement requires >=350 MH/s wall rate over sustained confirmation
  at reported conditions, not a peak sample. Loaded results remain separate.
- [ ] Require <=5% regression against matching C OpenCL baseline in quiet and
  loaded mining and build duration, plus quiet CPU target, for automatic cutover
  readiness. Extend noisy measurements or mark inconclusive. If Vulkan loses,
  deliver the complete Rust/Vulkan container and report the loss; do not disguise
  Rust OpenCL as Vulkan or overwrite the default.
- [ ] Produce concrete start/stop/rollback commands using verified image IDs,
  paths, device mappings and ports. Keep candidate and reference available.
  Update docs to distinguish actual current host from legacy HD 4600 defaults.
  Do not deploy, push, delete C reference or overwrite `latest` as part of this
  planning request. Future implementation follows its applicable authorization.

## 5. Command gates

Run from repository root. Commands for new crates apply after their tasks create
them. Never count an unavailable tool, silent early return or optional test skip
as qualification. Keep final tests serialized with mining stopped.

```sh
make test
make test-scripts
bash tests/test_bench_scripts.sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml --workspace -- --test-threads=1
GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-backend-vulkan -- --test-threads=1 --nocapture
GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-backend-opencl -- --test-threads=1 --nocapture
docker build -f Dockerfile.rust -t gamble-btc:rust-vulkan-candidate .
```

Add a candidate qualification script during R6 with explicit image, debug-image,
DRM nodes/GIDs and port parameters. It must run the container probe, SPIR-V and
debug validation checks, isolated mock-pool integration and soak with exit codes,
using traps to remove only its owned containers. Do not rely on a developer
remembering long ad hoc Docker commands to reproduce final gates.

## 6. Resume and one-run handoff

After every accepted task, append: UTC time; baseline/dirty-state changes; exact
files; tests/exits; device/image/driver when applicable; decisions and failed
experiments; unresolved issues; exact next unchecked step. Keep raw logs and
bench artifacts ignored and sanitized summaries in Git. No invented completion
because the coding worker returned success.

Paste this into the implementation session:

```text
Finish the full Rust/Vulkan iGPU miner rewrite in this checkout in one sustained
run. Read AGENTS.md and docs/rust-vulkan-planning/12-one-shot-rewrite-plan.md,
then inspect actual code and PROGRESS.md. Preserve all existing dirty/untracked
work. This is a partial rewrite: do not recreate or discard the Rust workspace.

Execute R0–R9 sequentially, with small verified increments and progress saved
after each. Continue across context refreshes without routine confirmation.
Use Parts 3/4/6/9 and RUST-COMPATIBILITY.md for detailed contracts; this dated
plan overrides their stale state, target and order. Do not launch worker agents
unless delegation is authorized in this session. If delegated, retain the
explicit model/effort policy in Part 11 with no silent model substitutions.

Deliver a C-independent Rust production binary/container, real Vulkan compute,
Rust OpenCL compatibility path, correct Stratum/job/share lifecycle, existing
status/UI/SSE, launcher/probe/bench modes, and low CPU overhead. Preserve GPU
candidate completeness and independent full-hash checks. Fix the identified
device/config/harness/test gaps before tuning. All nonce search stays on the
selected Iris Xe; never add CPU or discrete-GPU mining fallback.

Measure a fresh C OpenCL control, Rust OpenCL and Rust Vulkan. Run the bounded
E1–E8 campaign and quiet plus real compilation-loaded comparisons. Aim for
350 MH/s sustained wall throughput but report actual results, CPU, clocks,
power limits, noise and failed experiments. Missing 350 does not justify fake
numbers, indefinite tuning, skipped correctness or silently selecting OpenCL.

Use isolated immutable images, owned test containers/ports and synthetic pool
data. Do not modify host packages/drivers/power policy or constrain compilation.
Do not overwrite production/latest, discard user work, push or deploy without
applicable authorization. If a hardware/tool/quota block prevents a gate, finish
independent work, preserve partial results and state exactly what remains.

Finish with QUALIFICATION.md, updated progress/ledger and user docs, commands
and exits, measured result/gap to 350, limitations, candidate identity and concrete
cutover/rollback commands. A plan/checklist or a worker's success message alone
does not establish a complete rewrite.
```
