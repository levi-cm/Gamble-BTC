# Full Rust + Vulkan Rewrite — Planning Index

**Current entry point (2026-10-18):**
[One-shot rewrite plan and launch prompt](12-one-shot-rewrite-plan.md).
It audits the existing partial Rust implementation, defines the remaining R0–R9
steps, and updates the campaign to the requested **350 MH/s** target. Its current
state, ordering and target supersede older statements below.

Revision: 2026-10-17. These are implementation instructions, not completed work.
Keep the documents and code identifiers in English. The intended worker is a
cheaper, simpler coding model. Execution uses DeepSeek V4.1 Flash as orchestrator and `opencode/beam-free`
with `max` effort for implementation workers. See Part 11 for launch rules.

## Mission

Rewrite the production miner host in Rust and implement real Vulkan compute.
Keep nonce search on the iGPU. Preserve Stratum, work construction, status UI,
launch scripts, and low CPU use. Retain OpenCL as a secondary backend implemented
through Rust bindings, so the full rewrite does not depend on the old C miner.
Replace GLES with Vulkan; document the GLES configuration migration explicitly.
400 MH/s is a stretch experiment, not a promised outcome or completion gate.

The current source is C with GLES and OpenCL. Vulkan is a placeholder in
`src/backend.c`. The checked branch during this planning review was `igpu-vulkan`.
The folder was already untracked; this revision does not stage or commit it.
Recheck branch, source, and dirty state at implementation time.

## Read and execute

1. Read repository `AGENTS.md`, this index, `00-reality-and-goals.md`, and
   `99-agent-conventions.md`.
2. Read `PROGRESS.md` and use the first incomplete task in `10-task-sequence.md`.
3. Read only that task's phase file and the relevant part of `09-contracts.md`.
4. Implement one small step, run its check, inspect the diff, save progress.
5. Continue sequentially. Refresh context from saved files when needed.
6. Use `LAUNCH-PROMPT.md` to start or resume the whole job.

Do not implement the entire package in one edit. Do not start a fresh framework
or redesign the UI. Use bounded Beam subagents; default one writer. All GPU operations are serialized.

## Files

| File | Purpose |
| --- | --- |
| [00-reality-and-goals.md](00-reality-and-goals.md) | Scope, historical evidence, acceptance |
| [01-branch-and-toolchain.md](01-branch-and-toolchain.md) | Safe workspace, crate layout, reproducible tools |
| [02-phase0-ceiling.md](02-phase0-ceiling.md) | Baseline and optional throughput model |
| [03-vulkan-backend.md](03-vulkan-backend.md) | Device, memory, synchronization, shader correctness |
| [04-rust-runtime.md](04-rust-runtime.md) | Core, Stratum, HTTP, OpenCL, controller |
| [05-hashrate-campaign.md](05-hashrate-campaign.md) | Bounded experiments and workload measurements |
| [06-integration-verification.md](06-integration-verification.md) | Containers, CI, qualification, final report |
| [07-gates-and-risks.md](07-gates-and-risks.md) | Gate definitions and blocked-work behavior |
| [08-experiment-ledger.md](08-experiment-ledger.md) | Evidence and experiment decisions |
| [09-contracts.md](09-contracts.md) | Shared types, byte order, compatibility checklist |
| [10-task-sequence.md](10-task-sequence.md) | Small ordered implementation tasks |
| [11-model-orchestration.md](11-model-orchestration.md) | Exact model roles, worker launches and quota handling |
| [99-agent-conventions.md](99-agent-conventions.md) | Worker instructions and evidence rules |
| [PROGRESS.md](PROGRESS.md) | Durable resume state and verification history |
| [LAUNCH-PROMPT.md](LAUNCH-PROMPT.md) | Paste-ready start/resume instructions |

## Dependency order and status

| Gate | Deliverable | Initial state |
| --- | --- | --- |
| G0 | Recon, compatibility inventory, scaffold, isolated baseline identity | pending |
| G1 | Baseline evidence or explicit measurement block; no ceiling proof required | pending |
| G2 | Vulkan backend and complete GPU conformance | pending |
| G3 | Rust runtime, Rust OpenCL, protocol/UI/config parity | pending |
| G4 | Build, container, CI, integration and soak qualification | pending |
| G5 | Performance qualification and production cutover readiness | pending |

Execute G0 -> G1 -> G2 -> G3 -> G4 -> G5. A hardware block at G1 permits
host-only development; G2 and later GPU claims remain pending until tested.
A performance shortfall does not cancel the rewrite or authorize deleting C.
Keep C buildable as a reference through qualification. Final Rust production
artifacts must build and run without the C project sources or C miner library.
Physical removal of reference files is separate from production cutover.
