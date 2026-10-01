// CUDA backend conformance: identical gates to the OpenCL backend through
// the shared core in gpu_conformance_common.h (complete sparse candidate
// sets, nonzero base, 0xFFFFFFFF edge, equality, defined overflow).
// Skips (exit 0) when no CUDA device is present, so `make test` stays green
// on GPU-less hosts. The cubin path defaults to the shipped Pascal build;
// GBTC_CUDA_CUBIN overrides it for A/B trials.
#define _POSIX_C_SOURCE 200809L
#include "backend.h"
#include "bench.h"
#include "cuda.h"
#include "sha256.h"
#include "gpu_conformance_common.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern const gbtc_backend_t gbtc_cuda_backend;

static int check_block(const char *block)
{
    setenv("GBTC_CUDA_BLOCK", block, 1);

    char reason[512] = "";
    if (gbtc_cuda_backend.init(reason, sizeof(reason)) != 0) {
        fprintf(stderr, "[block %s] init failed: %s\n", block, reason);
        return 1;
    }
    printf("[block %s] init: %s\n", block, reason);
    char label[64];
    snprintf(label, sizeof(label), "cuda-block-%s", block);
    // Edge/overflow probes must be a multiple of the block size.
    uint32_t small = (uint32_t)strtoul(block, NULL, 10);
    if (small < 128u) small = 128u;
    int failures = gbtc_conform_run_cases(&gbtc_cuda_backend, label, small);
    gbtc_cuda_backend.shutdown();
    return failures;
}

int main(void)
{
    // Default to the in-tree Pascal cubin when the caller did not override
    // the path (e.g. plain `make test` from the repository root).
    if (!getenv("GBTC_CUDA_CUBIN") || !getenv("GBTC_CUDA_CUBIN")[0]) {
        setenv("GBTC_CUDA_CUBIN", "src/cuda/gbtc_mine.sm61.cubin", 1);
    }
    char reason[512] = "";
    if (gbtc_cuda_backend.probe(reason, sizeof(reason)) != 0) {
        printf("cuda conformance skipped: %s\n", reason);
        return 0;
    }
    printf("cuda device: %s\n", reason);

    int failures = 0;
    failures += check_block("64");
    failures += check_block("256");
    if (failures == 0) printf("cuda conformance passed\n");
    return failures ? 1 : 0;
}
