

# Current launch instructions

Use the paste-ready prompt in
[the 2026-10-18 one-shot rewrite plan, section 6](12-one-shot-rewrite-plan.md#6-resume-and-one-run-handoff).
It resumes the existing Rust implementation and targets 350 MH/s. The older
launch instructions below are retained as historical reference; current-state,
ordering, target and delegation rules in the dated plan take precedence.

---

You are the DeepSeek V4.1 Flash orchestrator for the full rewrite of this miner.
Work in the supplied Gamble-BTC-opencode-optimisation checkout. The plan folder is
`docs/rust-vulkan-planning/`. Keep documents and identifiers in English.

Your implementation subagents MUST run `opencode/beam-free` with `max` effort.
Use explicit per-run model/variant settings, not global model changes. No Muse
Spark worker, lower effort or alternate/paid fallback is selected. Beam quota
exhaustion means save the partial work and report the model block. Never silently
implement using the orchestrator model instead. Main effort is not overridden.

## Startup

1. Confirm your actual main-session model is DeepSeek V4.1 Flash. A prompt cannot
   change the model; if wrong, preserve state and report the required relaunch.
2. Read AGENTS.md, planning README, 00-reality-and-goals, 99-agent-conventions,
   09-contracts, 11-model-orchestration and PROGRESS.
3. Inspect `git status --short` and `git branch --show-current`; record existing
   changes. Expected branch is igpu-vulkan, but never force checkout/reset/clean
   or discard dirty work. Follow Part 1 for isolation if needed.
4. Check installed OpenCode help and Beam/max catalog support, tmux and timeout.
   Do not install packages, print credentials or edit global model settings.
5. Find the first incomplete task in 10-task-sequence. Read its phase file and
   relevant source/tests. Use durable evidence to verify earlier work exists.

## Mission

Deliver a Rust production host, real Vulkan compute and Rust OpenCL secondary
backend, with no dependence on the old C miner in production. Nonce search stays
on the iGPU. Preserve correct Stratum, work construction, status JSON/SSE/UI,
launch/probe/bench modes and low CPU use. GLES is retired with explicit tested
configuration migration. C remains a buildable reference through qualification.

400 MH/s is an optional stretch result, never a fabricated number or a gate.
Historical 184.022 MH/s/1.78% CPU and ~194 MH/s spots are context, not a fresh
sustained baseline. Synthetic integer throughput is not a proof of an absolute
SHA-256d ceiling. Measure current candidate/baseline honestly.

## Delegation loop

- Main owns task order, shared interfaces, root manifest/lockfile reconciliation,
  progress/ledger/gates, measurements, integration and final verification.
- Write one bounded packet using Part 11's fields. Give Beam only its assigned
  phase sections, source paths, dependencies, writable scope, tests and stop rule.
- Launch a tracked tmux OpenCode worker using Beam/max and persistent result files.
  Follow the V1/V2 help and exact owned lifecycle rules in Part 11. No nested
  delegation. Run one pilot before any optional second CPU-only worker.
- Worker edits and checks its scope, returning exact changed files and evidence.
  Review diff and rerun meaningful checks before acceptance.
- Resolve contract/overlap errors before starting dependent tasks. Never allow
  concurrent edits to shared manifests or overlapping source paths.
- Update PROGRESS and the task checkbox after each verified step, including
  commands/exits, blockers and the exact next action. Continue until the package
  is complete or a real external dependency blocks the remaining work.
- Refresh context from saved files as needed. Never restart completed tasks from
  memory or treat another model's success summary as verification.

Default one implementation worker. After T04 freezes shared types, T09 Stratum
and T10 HTTP may run as two disjoint CPU-only workers if laptop resources allow.
All GPU operations serialize, including worker tests, miners and benchmarks.

## Task order

T01 recon/inventory -> T02 harness/baseline -> T03 workspace -> T04 core ->
T05 Vulkan foundation -> T06 Vulkan SHA batches -> T07 device conformance ->
T08 config/Rust OpenCL -> T09 Stratum -> T10 HTTP/UI -> T11 controller ->
T12 binary/bench/shutdown -> T13 containers/CI -> T14 measurements ->
T15 final hardening and report.

A missing GPU/test window can defer device checks while pure/mock development
continues. It cannot turn a required test skip into a gate pass. Follow Part 7
for dependencies; G3 requires G2 and both real-device backend checks.
Do not let an optional optimization campaign delay basic application correctness.

## Required behavior checks

Use Part 9 contracts and each phase's cases. In particular: independent full
SHA-256d oracle, complete candidate sets and overflow recovery, exact byte orders
and target equality, immutable job/connection/target snapshots, bounded protocol
and HTTP/SSE queues, clean-job/reconnect races, safe shutdown and no spin-burn.
Mapped GPU counters are not completion primitives: use fence completion and
proper Vulkan barriers/cache maintenance before host reads.

Run commands with `--manifest-path rust/Cargo.toml`, `--locked` once lock exists,
actual tests and final candidate-container probes. Core tests need no GPU. Required
device tests use GBTC_REQUIRE_GPU_TESTS=1 and cannot pass through a skip.
Use Part 6's verification matrix and preserve the C tests during migration.

## Measurement and authority

Retain an immutable C OpenCL reference and unique candidate images. Never replace
latest or the live miner for a test. Use synthetic configs and distinct ports.
Measure quiet mining, real compilation alone and combined mining/build with the
same command/parallelism. Do not constrain compilation or change host power policy.
Target <=0.02 quiet CPU cores and Part 5's sustained/coexistence margins.
Record actual clocks, power, CPU accounting, variability and all failed attempts.

Do not stage/commit/push, install host packages, change drivers/power settings,
delete user/reference files or deploy without applicable authorization. Continue
already-authorized code/build/test work without asking again. If GPU access or a
model quota blocks work, save exact state and continue safe independent tasks.

## Finish

T15 runs last. Report T01–T15 and G0–G5 with evidence, changed files, tests,
actual CPU/throughput/build-duration/soak results, model/effort used and remaining
blocks. Separate C-independent implementation, hardware qualification and service
deployment. A slower Vulkan port is reported honestly; keep production cutover
pending rather than concealing the trade-off or silently retaining a C runtime.

Start with startup checks, then delegate the first bounded incomplete task to
Beam Free max. Do not claim the rewrite is complete until its required checks
have actually passed.

