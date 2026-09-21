// OpenCL backend conformance: every nonce the GPU reports must verify on the
// CPU reference path, and hashes_done must equal the requested batch size.
// Skips (exit 0) when no OpenCL GPU is present, so `make test` stays green
// on GPU-less hosts.
#include "backend.h"
#include "bench.h"
#include "opencl.h"
#include "sha256.h"

#include <stdio.h>
#include <string.h>

extern const gbtc_backend_t gbtc_opencl_backend;

static uint32_t bswap32(uint32_t x)
{
    return (x >> 24) | ((x >> 8) & 0x0000ff00u) |
           ((x << 8) & 0x00ff0000u) | (x << 24);
}

int main(void)
{
    char reason[512] = "";
    if (gbtc_opencl_backend.probe(reason, sizeof(reason)) != 0) {
        printf("opencl conformance skipped: %s\n", reason);
        return 0;
    }
    printf("opencl device: %s\n", reason);
    if (gbtc_opencl_backend.init(reason, sizeof(reason)) != 0) {
        fprintf(stderr, "opencl init failed: %s\n", reason);
        return 1;
    }
    printf("opencl init: %s\n", reason);

    int failures = 0;
    gbtc_work_batch_t work;
    gbtc_backend_result_t result;

    // Case 1: all-ones target -> every nonce matches; only first 15 stored.
    gbtc_make_synthetic_work(&work, 4096);
    for (int i = 0; i < 8; i++) work.target[i] = 0xffffffffu;
    memset(&result, 0, sizeof(result));
    if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
        fprintf(stderr, "run_batch failed on all-match case\n");
        return 1;
    }
    if (result.hashes_done != 4096) {
        fprintf(stderr, "hashes_done=%llu want 4096\n",
                (unsigned long long)result.hashes_done);
        failures++;
    }
    if (result.count != GBTC_MAX_FOUND_NONCES) {
        fprintf(stderr, "count=%u want %u on all-match case\n",
                result.count, GBTC_MAX_FOUND_NONCES);
        failures++;
    }
    for (uint32_t i = 0; i < result.count; i++) {
        uint32_t n = result.nonces[i];
        if (n < work.nonce_base || n >= work.nonce_base + 4096) {
            fprintf(stderr, "nonce %u out of range\n", n);
            failures++;
            continue;
        }
        uint8_t hash[32];
        gbtc_work_hash(hash, &work, n);
        uint32_t w7 = ((uint32_t)hash[28] << 24) | ((uint32_t)hash[29] << 16) |
                      ((uint32_t)hash[30] << 8) | (uint32_t)hash[31];
        if (bswap32(w7) > work.target[0]) {
            fprintf(stderr, "nonce %u fails word0 target on CPU recheck\n", n);
            failures++;
        }
    }

    // Case 2: zero word0 target -> expect no matches.
    gbtc_make_synthetic_work(&work, 4096);
    work.target[0] = 0x00000000u;
    memset(&result, 0, sizeof(result));
    if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
        fprintf(stderr, "run_batch failed on zero-target case\n");
        return 1;
    }
    if (result.count != 0) {
        fprintf(stderr, "count=%u want 0 on zero-target case\n", result.count);
        failures++;
    }
    if (result.hashes_done != 4096) {
        fprintf(stderr, "hashes_done=%llu want 4096 (zero-target)\n",
                (unsigned long long)result.hashes_done);
        failures++;
    }

    gbtc_opencl_backend.shutdown();
    if (failures == 0) printf("opencl conformance passed\n");
    return failures ? 1 : 0;
}
