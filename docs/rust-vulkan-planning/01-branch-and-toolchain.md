# Part 1 — Workspace, Toolchain, and Build Layout

Read `09-contracts.md` before defining types. Follow tasks T01–T03 in Part 10.

## T01 inputs and safe workspace

Inspect `git status --short`, `git branch --show-current`, `git diff --stat`,
`git log -5 --oneline`, repository instructions, and tracked build/test files.
Record pre-existing changes and the baseline commit/image identity in PROGRESS.
Do not run checkout/reset/clean/stash automatically. If already on `igpu-vulkan`,
continue there. If elsewhere, inspect available branches/worktrees and use an
isolated branch/worktree that preserves all user changes; document its identity.
Do not switch a dirty checkout merely because the launch prompt names a branch.
No automatic commit, staging, or push is required by this plan.

Inventory tool availability read-only: `rustc --version`, `cargo --version`,
`docker --version`, `docker compose version`, `command -v glslc`,
`command -v spirv-val`, `command -v vulkaninfo`. Missing tools are not permission
to install host packages. Prefer an isolated build container for missing tools.
Debian package names in Dockerfiles are not Arch host installation commands.

## Target tree

```text
rust/
  Cargo.toml                   # workspace, resolver, shared dependencies
  Cargo.lock                   # commit for reproducible application builds
  rust-toolchain.toml          # exact installed/tested Rust release
  gbtc-core/src/{lib,work,target,sha256,backend}.rs
  gbtc-core/tests/              # vectors and byte-order/target tests
  gbtc-backend-vulkan/src/{lib,device,buffer,pipeline}.rs
  gbtc-backend-vulkan/tests/conformance.rs
  gbtc-backend-opencl/src/{lib,device,kernel}.rs
  gbtc-backend-opencl/tests/conformance.rs
  gbtc-stratum/src/{lib,protocol,client}.rs
  gbtc-stratum/tests/mock_pool.rs
  gbtc-http/src/{lib,status}.rs
  gbtc-http/assets/index.html
  gbtc-http/tests/http_contract.rs
  gbtc-miner/src/{main,config,controller,bench}.rs
  gbtc-miner/tests/              # mock-backend lifecycle/config tests
  shaders/sha256d.comp
  shaders/include/             # only if actual shared shader code needs it
```

Create files as their tasks need them. Do not create empty future modules just
to match this tree. Keep core independent of Vulkan, OpenCL, HTTP and networking.
Keep old `src/`, `tests/`, and build targets functional during migration.

## Dependency policy

- `ash` with dynamic loader support; no mandatory Vulkan link dependency for
  core/host tests. Use one compute queue and simple owned allocations initially.
- `gpu-allocator` only if direct allocation becomes materially harder; do not
  require a second abstraction to manage a few buffers.
- `serde`, `serde_json`, Tokio with selected features; `axum` plus its compatible
  HTTP dependencies for bounded HTTP/SSE. Choose compatible releases once and
  lock them. Do not mix copied APIs from different crate major versions.
- Use a small independent SHA-256 library in tests for known-answer checking.
  Production core needs midstate access; port the existing explicit compression
  helper and prove it against independent byte-level SHA-256d vectors.
- Rust OpenCL bindings must support existing device selection and event polling.
  Pick one maintained binding or a narrow dynamic FFI module; justify the choice
  in PROGRESS. Do not implement a general GPU framework.

## Reproducible shader build

Use one shader compiler path: `glslc` in the build container, invoked from the
Vulkan crate's `build.rs`. Pin the chosen Rust toolchain and record compiler,
SPIR-V target and package versions. Compile to `OUT_DIR`; embed SPIR-V with
`include_bytes!`, with safe aligned word decoding for Vulkan module creation.
Emit `cargo:rerun-if-changed` for shader files and included files. Fail the build
with a useful diagnostic if the compiler is missing; never use a stale binary.
Validate generated SPIR-V with `spirv-val --target-env vulkan1.1` for the initial
Vulkan 1.1 path. A higher target requires capability checks and a documented
minimum-version change. Host-only package tests must not require shader tools.

## Containers

Keep a uniquely tagged baseline image and immutable image ID. Build candidates
with `docker build -t gamble-btc:rust-vulkan-candidate .`; never overwrite latest
for experiments. Extend Docker in Part 6 after the binary works locally/in a
builder. Install container Vulkan loader and Mesa Vulkan ICD explicitly; GLES
Mesa packages alone are insufficient. Driver/compiler experiments get separate
image tags. Keep NEO/IGC pins for the OpenCL comparison.

Done: dirty state recorded, compatibility inventory started, workspace compiles,
core host tests run, toolchain/shader path reproducible. No performance claim.
