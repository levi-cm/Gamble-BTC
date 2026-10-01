#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mkdir -p bench-results/toolbox/cargo
image="${GBTC_TOOLBOX_IMAGE:-gamble-btc-toolbox:cuda12.9}"
if ! docker image inspect "$image" >/dev/null 2>&1; then
    docker build -f Dockerfile.toolbox -t "$image" .
fi
args=(run --rm --init --gpus all
    --user "$(id -u):$(id -g)"
    -e "NVIDIA_DRIVER_CAPABILITIES=compute,utility,graphics"
    -e CARGO_HOME=/workspace/bench-results/toolbox/cargo
    --mount "type=bind,source=$PWD,target=/workspace"
    --workdir /workspace)
if [[ -t 0 && -t 1 ]]; then args+=(-it); fi
if [[ $# == 0 ]]; then set -- bash; fi
exec docker "${args[@]}" "$image" "$@"
