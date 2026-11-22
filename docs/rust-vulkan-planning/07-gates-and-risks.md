# Part 7 — Gates, Decisions, and Risks

Gate definitions here are authoritative. Do not redefine them in a worker packet.
Unknown/skipped/blocked is not passed. No 450 MH/s microbenchmark threshold exists.

| Gate | Evidence required | On incomplete/failure |
| --- | --- | --- |
| G0 | Dirty-state/baseline identity, tools, compatibility inventory and scaffold | Fix recon/scaffold before dependent code |
| G1 | Audited harness, baseline results or explicit hardware block | Host code may continue; performance claims blocked |
| G2 | Real Vulkan device conformance, full-set/overflow, validation and lifecycle | Host/mock tasks may continue; no GPU success claim |
| G3 | C-independent Rust runtime plus Rust OpenCL, mock protocol, config, HTTP/UI parity and device tests | Keep C reference and production settings |
| G4 | Combined CI/build/container checks, integration, soak and docs | No release-ready claim |
| G5 | Part 5 sustained/loaded/CPU qualification and concrete rollback/cutover plan | No automatic primary/default production cutover |

G3 host-only work may progress while G2 waits for hardware. Passing G3 itself
requires G2 and the Rust OpenCL device checks. G4 requires G3; G5 requires G4 and
actual performance measurements. Optional experiments do not block G4 if rejected
with evidence or deferred with a reason; mandatory correctness cases do.

## Performance decision rules

The 400 MH/s target is optional research. Missing it does not fail a correct full
rewrite. The baseline is measured at implementation time, not the best historical
spot sample. Target <=0.02 quiet CPU core-equivalents and the non-regression
margins in Part 5. A statistically inconclusive result stays inconclusive.
If Vulkan is materially slower, complete the Rust application and report the
trade-off. Do not quietly turn the mission into a C/OpenCL project or mark G5
passed. Keep production cutover pending user acceptance of the measured trade-off.

## Model and worker gates

Follow `11-model-orchestration.md`: DeepSeek V4.1 Flash orchestrates;
`opencode/beam-free` with `max` implements/reviews. No alternate-model fallback
is selected. Missing catalog entries, auth failures and permission errors
are not quota evidence. Save worker edits and handoff before retry/replacement.
Never run two replacement writers against the same files.

## Risk register

| Risk | Mitigation |
| --- | --- |
| Cheaper model loses state or expands scope | One bounded task packet; explicit files/tests; PROGRESS after each check |
| Beam Free model missing or unavailable | Recheck catalog; record model block; do not fabricate availability |
| Quota exhausts mid-edit | Confirm quota cause, stop exact worker, inspect partial edits, record blocked state; no model substitution |
| Vulkan synchronization/data race | One slot first, explicit barriers/cache maintenance, validation and stress cases |
| Candidate overflow drops shares | Complete-set contract, subdivision/rescan, dedup and unique coverage accounting |
| Endianness/difficulty bug | Byte-level known vectors, independent hash/target oracle, equality tests |
| Job or connection changes misattribute shares | Immutable snapshots and separate connection/job/target generations |
| Rust still depends on C host | Inspect production link/build graph and run build without C source inputs |
| PL1/heat/build load misleads benchmarks | Matched sustained workloads; report clocks/power and variance |
| Host/container ICD mismatch | Probe inside exact candidate image and select the intended DRM device |
| Explicit unsupported backend silently substituted | Explicit request errors; fallback only under documented auto policy |
| UI or slow SSE client increases CPU/memory | Bounded snapshots/history/queues, slow-client and soak tests |
| Reference or live service overwritten | Immutable baseline image, isolated candidate names/ports, owned cleanup |

## Actions outside this planning request

This turn authorizes editing these planning documents only. Future implementation
launch needs the user's request to execute it. Package instructions are not an
instruction to this planning session to run workers or alter the live system.
During implementation, do authorized code/build/test work without repeated
confirmation. Preserve existing authorizations. Host package/power/driver changes,
service replacement, publication and destructive reference deletion require
applicable authorization. A blocked GPU window does not block pure/mock work.

## Decision log

| Date | Gate/decision | State | Evidence |
| --- | --- | --- | --- |
| 2026-10-17 | Planning audit | Documentation revised; implementation not run | PROGRESS.md |
| 2026-10-17 | Model policy | User selected DeepSeek V4.1 Flash + Beam Free max; no alternate fallback | 11-model-orchestration.md |
| 2026-10-17 | G0–G5 | All pending | No implementation evidence yet |
