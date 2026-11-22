# Part 6 — Integration, Qualification, and Cutover

Follow T13–T15. Verify the combined result after workers return; a worker summary
is not proof. All commands below run from the repository root unless noted.

## Build/CI tasks

- [ ] Add Rust test/check targets to `Makefile` without breaking the C targets.
  Use `--manifest-path rust/Cargo.toml` and `--locked`; workspace is not root Cargo.
- [ ] Add a pinned Rust builder and deterministic shader compiler to `Dockerfile`.
  Production ENTRYPOINT is the Rust binary; no C host library/binary required.
  Keep a separate C reference build target/image until G5.
- [ ] Install Vulkan loader and Mesa Vulkan ICD in runtime. Probe inside the
  candidate container, recording its ICD/driver, not just host `vulkaninfo`.
  Keep required OpenCL ICD/vendor packages for Rust OpenCL. Run as non-root with
  verified DRM groups. Do not bind-mount host driver libraries as a shortcut.
- [ ] Provide a candidate Compose override with a distinct service/container/image
  and no restart policy during tests. Use no production port for bench/probe;
  a test UI can publish `127.0.0.1:41175:41174`. Preserve cache/device permissions.
- [ ] Use a temporary synthetic env file for Compose validation; do not overwrite
  `.env`. `docker compose config --quiet` validates the existing live contract
  when available; candidate validation supplies its explicit test env file.
- [ ] Fix launcher/stop scripts to select candidate/reference intentionally,
  preserve Ctrl-C/detach/battery behavior and stop only owned containers.
- [ ] Add CI fmt/clippy/host tests, shader compile plus spirv-val, container build
  and mock-pool/HTTP tests. Preserve C CI until retirement. CPU CI may explicitly
  skip device tests; it cannot qualify G2/G5.

## Commands and expected results

```sh
# Pure core first; shader tools and GPU not needed.
cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-core
# Once workspace modules and builder tools exist:
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml --workspace
make test
bash -n scripts/start-miner.sh scripts/stop-miner.sh scripts/bench-run.sh scripts/bench-ab.sh
docker compose config --quiet
docker build -t gamble-btc:rust-vulkan-candidate .
git diff --check
```

Expected: exit 0 and explicit test summaries; host `make test` can skip OpenCL,
which must be reported. Workspace commands require the documented shader tools.
Do not claim a command passed merely because the tool is unavailable.
Run C sanitizers/static analysis for changed C host code; do not destructively
`make clean` a user build merely to recheck unchanged C for a documentation task.

Required target-device invocations after confirming an exclusive GPU window:

```sh
GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-backend-vulkan --test conformance -- --test-threads=1
GBTC_REQUIRE_GPU_TESTS=1 cargo test --locked --manifest-path rust/Cargo.toml -p gbtc-backend-opencl --test conformance -- --test-threads=1
```

Run the Vulkan check once with debug validation enabled and once with the final
candidate configuration. No ignored/skipped required cases. Document exact
candidate-container equivalents after T13 establishes the test runner; do not
invent an unimplemented executable. Probe/bench must use synthetic mode without
credentials. Check both loader/device absence and intended-device success.

## Application integration and soak

With a synthetic mock pool and isolated candidate HTTP port, exercise:

- Authorization/notify, complete correct work and submit fields, accept/reject.
- Difficulty switch, clean/non-clean jobs, stale suppression, reconnect and EOF.
- GET/HEAD/OPTIONS, JSON schema/units, SSE initial history, live updates and slow
  or disconnected clients. UI waiting/active/reconnecting states.
- Repeated starts/stops, SIGTERM under GPU load, no-job idle, timeout and device
  failure paths, bounded queues/counters/RSS. No process/container leaks.
- >=30 minute soak on the candidate with repeated mock job/target changes;
  record memory trend, CPU, errors, stale/duplicate shares and shutdown duration.

A public-pool smoke test requires an authorized live test window/configuration;
acceptance of a rare real share is not guaranteed and is not the only correctness
oracle. If no live pool test occurs, report protocol integration as mock-tested.
Never dump real status fields/credentials in committed evidence.

## C retirement and production cutover

G3 requires a C-independent Rust production path; it does not require deleting
reference files. G4 requires reproducible packaging and integrated tests. G5 adds
Part 5 qualification. Keep default production configuration unchanged until G5
passes and service replacement is authorized. A slower Vulkan port may be
feature-complete while default cutover remains blocked; say both plainly.

After all gates, propose the exact candidate image ID, backend/default change,
configuration migration, service commands and rollback image/config. Rust OpenCL
is available as a performance alternative, but it cannot erase an unqualified
Vulkan performance result. Deleting old C files, merging, pushing and replacing
the running service require applicable authorization; no automatic deletion is
needed to finish the implementation package.

Update README, `.env.example`, `docs/ARCHITECTURE.md`, `docs/HARDWARE_MATRIX.md`,
CHANGELOG, CI, launcher instructions and the compatibility inventory together.
No references to removed GLES options as working production functionality.

## Final report

Report each T01–T15 task and G0–G5 gate as passed, pending, blocked or failed,
with evidence paths. Include exact source/image/toolchain/driver IDs, contracts
and tests, independent GPU conformance, sustained/loaded performance and CPU,
compilation duration, soak/UI status, fallback use, remaining limits and rollback.
State whether 400 MH/s was measured; do not imply it if it was not. Distinguish
implementation completion, hardware qualification and deployment status.
