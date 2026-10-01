#!/usr/bin/env bash
set -euo pipefail
# Execute through scripts/dev-toolbox.sh. This inventory does not launch hashing.
for tool in gcc make clang llvm-objdump nvcc ptxas cuobjdump nvdisasm nvprof \
    compute-sanitizer cuda-gdb clinfo python3 jq pidstat shellcheck cppcheck \
    gdb valgrind strace rustc cargo glslangValidator spirv-dis; do
    command -v "$tool" >/dev/null || { printf 'Missing tool: %s\n' "$tool" >&2; exit 1; }
done
nvcc --version | tail -n 2
gcc --version | head -n 1
rustc --version
cargo --version
clinfo -l
nvidia-smi --query-gpu=name,driver_version,compute_cap --format=csv,noheader
printf 'Toolkit inventory passed. Compile CUDA kernels explicitly for sm_61.\n'
printf 'Pascal profiling: use nvprof; bundled Nsight Compute does not support this GPU.\n'
