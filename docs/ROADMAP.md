# Roadmap

This project is not ready for broad open-source release yet. The immediate goal
is to document and preserve the current working container while preparing for a
larger hardware-support push.

## Phase 1: Repository Readiness

- Add safe ignore rules for local wallet/runtime configuration.
- Document the current Intel HD 4600 host-specific setup.
- Add license, contribution, security, and changelog files.
- Keep the current runtime behavior unchanged.

## Phase 2: Hardware Support Matrix

Track integrated GPU support explicitly instead of implying portability.

Initial targets to investigate:

- Intel iGPUs across relevant Mesa drivers.
- AMD APUs through Mesa.
- Apple integrated GPUs where the software stack makes sense.
- Other Linux-accessible integrated GPU stacks if they can expose usable compute.

Each target should record:

- Hardware model.
- Kernel version.
- Mesa or vendor driver version.
- Required container devices and groups.
- EGL/GLES capability output.
- Verified build and run command.
- Observed hashrate and stability notes.

## Phase 3: Runtime Generalization

- Add a GPU capability probe before mining starts.
- Make device paths and group handling configurable.
- Provide separate Compose examples for tested hardware families.
- Avoid changing the known Intel HD 4600 path until replacement coverage exists.

## Phase 4: Testing and Release Prep

- Add deterministic CPU-side tests for hash and Stratum helpers.
- Add container build checks in CI.
- Add hardware-gated smoke-test documentation.
- Review README claims against the verified support matrix before publishing.
