# Part 8 — Experiment and Measurement Ledger

No rewrite benchmark has been performed by this planning revision. Keep historical
repository evidence distinct from newly observed data. Append records; do not
rewrite a rejection into a success or delete inconvenient measurements.

## Decisions

- PENDING: not executed.
- KEEP: intended benefit repeats, correctness holds, CPU/workload constraints met.
- REJECT: mechanism disproven, regression, or adequately measured lack of benefit.
- INCONCLUSIVE: insufficient/noisy/unmatched observations or missing telemetry.
- BLOCKED: execution cannot proceed; record the dependency and next safe action.

A throttled run can be valid loaded-workload evidence. It cannot establish an
unthrottled result. An unchanged GPU binary can still accompany a host-overhead
improvement; name the mechanism honestly.

## Record template

```text
ID / date / task:
Question and one changed mechanism:
Source revision / exact changed paths:
Baseline image ID / candidate image ID:
Actual backend / kernel / public config:
Host and container driver/compiler versions:
Device identity / AC / workload / warmup / run order:
Correctness command / exit / complete-set and overflow results:
Source, SPIR-V, native binary hashes (state which; unavailable is unknown):
CPU accounting source / CPU seconds / wall seconds / core-equivalents:
Hash count definition / throughput median / spread / paired deltas:
Clocks / throttle samples / telemetry gaps:
Build command / parallelism / cache state / build-alone and combined durations:
Stale, overflow, rescan and duplicate counts:
Decision / reason / uncertainty:
Sanitized evidence paths:
Next action:
```

Raw logs remain local and untracked unless reviewed and sanitized. Committed
summaries use synthetic identifiers and public tuning values, never real wallets,
private pool URLs or complete live status/pool message dumps.

## Rewrite ledger

| ID | Task | Question | State | Evidence |
| --- | --- | --- | --- | --- |
| EX-000 | T02 | Current C OpenCL baseline and harness attribution | PENDING | none |
| EX-001 | T14 optional | Synthetic ALU workload/model, not an upper-bound proof | PENDING | none |
| EX-002 | T07 | Correct Vulkan baseline conformance | PENDING | none |
| EX-003 | T08 | Rust OpenCL parity and wait CPU | PENDING | none |
| EX-004 | T12 | Rust vs C application overhead | PENDING | none |
| EX-010 | T14 | Existing pre3 savings on Vulkan | PENDING | none |
| EX-011 | T14 | ANV second-SHA pruning | PENDING | none |
| EX-012 | T14 | Schedule/arithmetic | PENDING | none |
| EX-013 | T14 | Workgroup/subgroup | PENDING | none |
| EX-014 | T14 | Shared-prefix dual nonce | PENDING | none |
| EX-015 | T14 | Two-slot pipeline, only if gaps exist | PENDING | none |
| EX-016 | T14 | Isolated compiler comparison | PENDING | none |
| EX-017 | T14 | Poll/batch trade-off | PENDING | none |
| EX-020 | T14 | Compilation coexistence matrix | PENDING | none |
| EX-021 | T15 | Final sustained/soak qualification | PENDING | none |

## Historical routing (not fresh results)

Read `docs/HARDWARE_MATRIX.md` for measured context and caveats:

- OpenCL/NEO 184.022 MH/s at 1.78% container CPU is a historical sustained result.
- 186.634–193.994 MH/s are full-clock spot samples, not a matched sustained median.
- SIMD32, looped/dual, larger batches, existing IGC tail pruning and scheduling
  flags have negative/limited prior evidence; state changed conditions to revisit.
- `pre3` exists in source; inspect the current historical ledger before planning
  its implementation or claiming it is novel.

A historical value never fills a new EX record's measurement fields automatically.
