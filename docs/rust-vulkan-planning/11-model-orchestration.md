# Part 11 — DeepSeek Orchestrator and Beam Free Max Workers

User's final model choice supersedes earlier Muse Spark instructions.
This file governs future rewrite execution; no implementation worker was launched
as part of revising the plans.

## Fixed roles

| Role | OpenCode model | Effort |
| --- | --- | --- |
| Main orchestrator | DeepSeek V4.1 Flash | Preserve its selected/default effort |
| Implementation subagents | `opencode/beam-free` | `max` |
| Bounded review/test workers, if delegated | `opencode/beam-free` | `max` |

Both `opencode/deepseek-v4.1-flash-free` and
`opencode-go/deepseek-v4.1-flash` were listed by installed OpenCode 1.19.0 during
the planning review. Default main launch below uses the free DeepSeek ID; if the
user already launched Go DeepSeek V4.1 Flash, keep that main session/provider.
Do not switch the main to another DeepSeek generation or to Beam.

`opencode/beam-free` and its `max` variant were present in the inspected catalog.
Catalog presence is not a completed-response or quota-availability test. Recheck
at execution time and run one bounded implementation pilot before scaling.

The old Muse Free -> Go Muse Contributor fallback is retired. No automatic Go,
paid, lower-effort or different-model fallback is authorized for Beam by this
package. On Beam rate/quota exhaustion, preserve progress and report blocked.
Authentication, networking, permission, model-not-found and database errors are
not usage limits. Do not retry endlessly, change accounts or purchase credits.

## Main launch (installed V1 syntax)

From repository root, a user can start the main session with:

```bash
opencode run --model opencode/deepseek-v4.1-flash-free --file docs/rust-vulkan-planning/LAUNCH-PROMPT.md 'Execute the attached rewrite plan, beginning with its startup checks.'
```

No `--variant` is forced on the main agent. Runtime session model identity must
confirm DeepSeek V4.1 Flash before workers are launched. A prompt sentence cannot
change the model of an already-running session; if wrong, save state and report
that it must be relaunched with the correct model.

## Worker preflight

Read-only checks: `opencode --version`, `opencode run --help`,
`opencode models opencode --verbose`, `command -v tmux`, `command -v timeout`.
Inspect provider catalogs sequentially to avoid database contention. Do not dump
provider auth/config. Never modify global model defaults to choose workers.
The user explicitly requires Beam/max; use per-invocation model flags even if a
local generic skill normally preserves last-used model selection.

OpenCode 1.19.0 supports V1 `--dir`, `--model`, `--variant`, `--format json`.
If a future V2 CLI is installed, inspect its matching help/reference first: V2
may require `provider/model#effort` and `--standalone`, with no V1 flags.
Do not invent flags or silently lower effort if `max` is unavailable.

Use the installed `tmux-opencode-cli` skill for owned tmux lifecycle when present.
Its Muse/high defaults are overridden by this user's explicit Beam/max choice.
No companion `opencode-subagents` wrapper is needed: that wrapper's no-model
policy conflicts with the requested explicit worker model.

## Worker packet (orchestrator writes before launch)

Each packet must contain these concrete values, not unresolved angle brackets:

```text
Role: implementer (or read-only reviewer)
Model: opencode/beam-free; effort: max
Workspace: absolute checkout/worktree path
Branch/base: observed branch and source revision
Task: one Txx and named checkbox/substep
Read: AGENTS.md, 09-contracts.md, exact phase sections and source files
Dependency state: shared types and previous outputs needed by this task
Writable paths: exact owned files; none for review
Required outcome: observable behavior and regression cases
Commands: exact build/test commands and expected success/failure conditions
Constraints: no Git staging/commit/push, no host policy/package changes,
             no service replacement, no nested subagents, no unrelated edits
Stop: completed scope, unresolved contract, missing tool/device, or quota/error
Return: exact changed files; commands/exits; tests; limits; next safe step
```

Prefer a task-owned private result directory outside Git, with packet, stdout
JSONL, stderr, exit status and final report. Do not include credentials or raw
live status in packets. Persist evidence across context refresh and reboot.

## V1 worker command

Launch this core command inside an owned tmux worker, using a real packet file:

```bash
opencode run --dir "$GBTC_WORKSPACE" \
  --model opencode/beam-free --variant max --format json \
  < "$GBTC_PACKET_FILE"
```

`GBTC_WORKSPACE` is the absolute observed checkout, and `GBTC_PACKET_FILE` is the
absolute task packet path the orchestrator just wrote. Quote both. Capture the
command's true exit status with pipefail if logging through tee. Do not use
`--continue` to inherit an unrelated session. Do not use an interactive TUI as an
untracked background worker. Use `--auto` only for authorized implementation and
preserve configured denies; read-only reviews do not use it.

Tmux requirements: separate server `tmux -L opencode-cli`, owned marker
`@opencode_owner=tmux-opencode-cli-v1`, exact unique name such as
`oc-gbtc-t04-beam-free-max-01`, `remain-on-exit off`, persistent result directory
and startup gate before execution so ownership is recorded. Bound each worker
with `timeout --signal=TERM --kill-after=30s` and a task-appropriate deadline
(maximum 7d). Use the skill's matching launch reference for the wrapper; replace
its model/effort/session values with Beam/max. If that skill is absent, a simple
owned tmux session can run a task-owned wrapper script implementing the same
marker, logging, timeout and exit-file requirements. Do not skip ownership checks.

Never stop the default tmux server, `kill-server`, or unrelated sessions. Before
interrupt/replacement, verify the exact session's marker, capture its report,
stop that worker, wait for process exit and inspect partial edits. No two workers
write the same scope. Start with one pilot; at most two disjoint CPU-only workers
for this laptop package. Run every GPU operation exclusively.

## Completion and error handling

A worker needs exit 0, valid nonempty JSON events/final text, no unresolved error,
matching workspace/model/effort and terminal success. On V1 check a final
`step_finish` with reason `stop`; on a changed event schema verify documented
terminal session outcome rather than accepting an empty process exit.
The main then inspects diffs and reruns relevant checks before accepting a task.

On quota/rate limit: record the sanitized provider error, task, model/effort,
completed checks and partial diff; stop the worker safely; mark the task blocked
in PROGRESS. Main may continue independent planning/review/host tasks already
within its role, but it must not silently implement using another model.
On ordinary failure: diagnose the cause and issue a bounded repair packet to
Beam/max when available. Avoid concurrent or repeated blind replays of edits.

Main owns root manifests/lockfile reconciliation, shared contracts, PROGRESS,
ledger/gate updates, performance runs and final verification. Workers own their
assigned implementation scopes and return exact manifests; no nested delegation.
