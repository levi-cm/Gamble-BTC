// OpenCL backend conformance: every nonce the GPU reports must verify on the
// CPU reference path, and hashes_done must equal the requested batch size.
// Beyond all-match/no-match, sparse-target cases compare the COMPLETE
// expected candidate set (proving no match was missed in that range), plus
// nonzero bases, the 0xFFFFFFFF range edge, and target equality.
// Overflow is defined, not silent: when more than GBTC_MAX_FOUND_NONCES
// nonces match, the GPU stores only the first 15 and count is clamped, so a
// count of 15 with a full hashes_done means "possibly truncated".
// Skips (exit 0) when no OpenCL GPU is present, so `make test` stays
// green on GPU-less hosts.
#define _POSIX_C_SOURCE 200809L
#include "backend.h"
#include "bench.h"
#include "opencl.h"
#include "sha256.h"
#include "gpu_conformance_common.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern const gbtc_backend_t gbtc_opencl_backend;

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
    int failures = gbtc_conform_run_cases(&gbtc_opencl_backend, kernel, 128u);
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
    failures += check_variant("pre3", NULL);
    if (failures == 0) printf("opencl conformance passed\n");
    return failures ? 1 : 0;
}
