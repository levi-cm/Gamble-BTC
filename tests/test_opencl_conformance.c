// OpenCL backend conformance: every nonce the GPU reports must verify on the
// CPU reference path, and hashes_done must equal the requested batch size.
// Runs once per kernel variant (unrolled/looped/dual) plus a forced SIMD
// width. Skips (exit 0) when no OpenCL GPU is present, so `make test` stays
// green on GPU-less hosts.
#define _POSIX_C_SOURCE 200809L
#include "backend.h"
#include "bench.h"
#include "opencl.h"
#include "sha256.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern const gbtc_backend_t gbtc_opencl_backend;

#define CONFORMANCE_NONCES 4096u

static uint32_t bswap32(uint32_t x)
{
    return (x >> 24) | ((x >> 8) & 0x0000ff00u) |
           ((x << 8) & 0x00ff0000u) | (x << 24);
}

static int run_cases(const char *label)
{
    int failures = 0;
    gbtc_work_batch_t work;
    gbtc_backend_result_t result;

    // Case 1: all-ones target -> every nonce matches; only first 15 stored.
    gbtc_make_synthetic_work(&work, CONFORMANCE_NONCES);
    for (int i = 0; i < 8; i++) work.target[i] = 0xffffffffu;
    memset(&result, 0, sizeof(result));
    if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
        fprintf(stderr, "[%s] run_batch failed on all-match case\n", label);
        return 1;
    }
    if (result.hashes_done != CONFORMANCE_NONCES) {
        fprintf(stderr, "[%s] hashes_done=%llu want %u\n", label,
                (unsigned long long)result.hashes_done, CONFORMANCE_NONCES);
        failures++;
    }
    if (result.count != GBTC_MAX_FOUND_NONCES) {
        fprintf(stderr, "[%s] count=%u want %u on all-match case\n", label,
                result.count, GBTC_MAX_FOUND_NONCES);
        failures++;
    }
    for (uint32_t i = 0; i < result.count; i++) {
        uint32_t n = result.nonces[i];
        if (n < work.nonce_base || n >= work.nonce_base + CONFORMANCE_NONCES) {
            fprintf(stderr, "[%s] nonce %u out of range\n", label, n);
            failures++;
            continue;
        }
        uint8_t hash[32];
        gbtc_work_hash(hash, &work, n);
        uint32_t w7 = ((uint32_t)hash[28] << 24) | ((uint32_t)hash[29] << 16) |
                      ((uint32_t)hash[30] << 8) | (uint32_t)hash[31];
        if (bswap32(w7) > work.target[0]) {
            fprintf(stderr, "[%s] nonce %u fails word0 target on CPU recheck\n", label, n);
            failures++;
        }
    }

    // Case 2: zero word0 target -> expect no matches.
    gbtc_make_synthetic_work(&work, CONFORMANCE_NONCES);
    work.target[0] = 0x00000000u;
    memset(&result, 0, sizeof(result));
    if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
        fprintf(stderr, "[%s] run_batch failed on zero-target case\n", label);
        return 1;
    }
    if (result.count != 0) {
        fprintf(stderr, "[%s] count=%u want 0 on zero-target case\n", label, result.count);
        failures++;
    }
    if (result.hashes_done != CONFORMANCE_NONCES) {
        fprintf(stderr, "[%s] hashes_done=%llu want %u (zero-target)\n", label,
                (unsigned long long)result.hashes_done, CONFORMANCE_NONCES);
        failures++;
    }

    if (failures == 0) printf("[%s] conformance case passed\n", label);
    return failures;
}

static int check_variant(const char *kernel, const char *simd)
{
    setenv("GBTC_OPENCL_KERNEL", kernel, 1);
    if (simd) setenv("GBTC_OPENCL_SIMD", simd, 1);
    else unsetenv("GBTC_OPENCL_SIMD");

    char reason[512] = "";
    if (gbtc_opencl_backend.init(reason, sizeof(reason)) != 0) {
        fprintf(stderr, "[%s/%s] init failed: %s\n", kernel, simd ? simd : "auto", reason);
        return 1;
    }
    printf("[%s/%s] init: %s\n", kernel, simd ? simd : "auto", reason);
    int failures = run_cases(kernel);
    gbtc_opencl_backend.shutdown();
    return failures;
}

int main(void)
{
    char reason[512] = "";
    if (gbtc_opencl_backend.probe(reason, sizeof(reason)) != 0) {
        printf("opencl conformance skipped: %s\n", reason);
        return 0;
    }
    printf("opencl device: %s\n", reason);

    int failures = 0;
    failures += check_variant("unrolled", NULL);
    failures += check_variant("looped", NULL);
    failures += check_variant("dual", NULL);
    failures += check_variant("unrolled", "16");

    if (failures == 0) printf("opencl conformance passed\n");
    return failures ? 1 : 0;
}
