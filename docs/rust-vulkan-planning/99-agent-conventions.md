# Part 99 — Worker Rules and Handoff Discipline

Read README, Part 0, Part 9, Part 11 and PROGRESS before starting a new task.
The task packet specifies the writable scope. Follow repository AGENTS and the
current user's instructions; latest explicit user choices override stale plans.

## Working loop

1. Confirm workspace/branch/dirty state and assigned task/dependencies.
2. Read only relevant source/tests and the assigned plan section.
3. Identify one behavior and a meaningful check. For code changes, add a failing
   regression test where useful, implement the small change and run the test.
4. Inspect the diff for scope, correctness, private data and unintended changes.
5. Return exact paths, commands/exits, results and the next safe step.
6. Orchestrator verifies and updates PROGRESS, ledger, gates and task checkboxes.

Keep task steps small. Do not refactor unrelated code, rename shared interfaces
independently, generate empty placeholder modules, or broaden scope to hide a
failure. A test skip is not a pass. No fake command output or performance results.
No “done except tests” status for a task that requires those tests.

## Model roles and concurrency

DeepSeek V4.1 Flash is the main. Implementation workers use `opencode/beam-free`
with `max` effort. Main delegates implementation in bounded packets, reviews and
integrates results. No Muse model fallback remains. If Beam quota exhausts, save
state and report the block. No silently chosen alternate model or lower effort.
Do not require Codex-only agent APIs, vendor-specific skills or nested agents.

Default one writer; optionally two disjoint CPU-only workers after shared types
are frozen. All GPU mining, conformance, profiling and benchmarks serialize.
Use separate worktrees only if they preserve dirty work and reduce real conflict.
Main reconciles dependency manifests/lockfile; workers never stage/commit/push.

## Baseline, privacy and live state

Keep existing C source/tests usable as the reference. Preserve best-known images
and variants; no overwrite of `gamble-btc:latest` during experiments. Candidate
names/ports must be distinct and cleanup must target only owned resources.
Do not stop or replace an existing miner without an authorized test window.
If another process uses the GPU, defer device runs and continue pure/mock checks.

Do not print `.env`, credentials, wallets, full status JSON or raw pool traffic.
Read runtime secrets only through necessary existing application mechanisms;
use synthetic configurations for tests and sanitize persisted evidence.
Do not commit raw logs, caches, target/build output or private runtime data.
Do not change host power/clock policy, drivers or packages through this plan.
No global cleanup, broad process kills or deleting pre-existing user files.

## Evidence and performance

Use Part 5 definitions and ledger. Separate current vs historical evidence,
wall vs device throughput, unique nonce tests vs repeated overflow work, quiet vs
loaded workload and host vs container ICD. Report actual clocks and CPU units.
Do not reject all real power-limited runs; classify them by the claim they support.
Do not infer native code from SPIR-V or a global silicon ceiling from a microkernel.
A model's confidence is not evidence of a gate pass.

## Handoff format

```text
Task / checkbox:
Status: passed | in-progress | blocked | failed
Actual worker model / effort / session:
Changed files:
Checks: command -> exit status and observed result
GPU: actual device/driver or explicit not run
Evidence paths:
Unresolved contract/error:
Next safe step:
```

A context refresh starts by rereading PROGRESS and the current task file; do not
restart completed work or trust an unchecked checkbox. Preserve failed attempts
and partial edits when resuming. Main owns final acceptance and reports honestly
which of implementation, qualification and deployment are complete.
