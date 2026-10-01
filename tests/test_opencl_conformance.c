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

static int cmp_u32(const void *a, const void *b)
{
    uint32_t x = *(const uint32_t *)a, y = *(const uint32_t *)b;
    return (x > y) - (x < y);
}

// Collect nonces whose word0 prefilter passes. Returns the TOTAL number of
// matches (which may exceed max_out); only the first max_out are stored.
static uint32_t cpu_collect_word0(const gbtc_work_batch_t *work, uint32_t target0,
                                  uint32_t *out, uint32_t max_out)
{
    uint32_t total = 0;
    for (uint32_t i = 0; i < work->nonce_count; i++) {
        uint32_t nonce = work->nonce_base + i;
        uint8_t hash[32];
        gbtc_work_hash(hash, work, nonce);
        uint32_t w7 = ((uint32_t)hash[28] << 24) | ((uint32_t)hash[29] << 16) |
                      ((uint32_t)hash[30] << 8) | (uint32_t)hash[31];
        if (bswap32(w7) <= target0) {
            if (total < max_out) out[total] = nonce;
            total++;
        }
    }
    return total;
}

// Pick the first candidate target yielding a small non-empty match set so
// the GPU result can be compared as a complete set (no truncation).
static int pick_sparse_target(const gbtc_work_batch_t *work, uint32_t *target_out,
                              uint32_t *cpu_out, uint32_t *total_out)
{
    static const uint32_t candidates[] = {
        0x00010000u, 0x00020000u, 0x00040000u, 0x00080000u,
        0x00100000u, 0x00200000u, 0x00400000u, 0x00800000u,
    };
    for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]); i++) {
        uint32_t total = cpu_collect_word0(work, candidates[i], cpu_out,
                                           GBTC_MAX_FOUND_NONCES + 1u);
        if (total >= 1 && total <= GBTC_MAX_FOUND_NONCES) {
            *target_out = candidates[i];
            *total_out = total;
            return 0;
        }
    }
    return -1;
}

static int check_sorted_set_equal(const char *label, const char *what,
                                  const uint32_t *cpu, uint32_t cpu_total,
                                  uint32_t *gpu, uint32_t gpu_count)
{
    if (gpu_count != cpu_total) {
        fprintf(stderr, "[%s] %s: gpu count=%u want cpu total=%u\n",
                label, what, gpu_count, cpu_total);
        return 1;
    }
    uint32_t sorted_cpu[GBTC_MAX_FOUND_NONCES + 1u];
    uint32_t sorted_gpu[GBTC_MAX_FOUND_NONCES];
    memcpy(sorted_cpu, cpu, cpu_total * sizeof(*cpu));
    memcpy(sorted_gpu, gpu, gpu_count * sizeof(*gpu));
    qsort(sorted_cpu, cpu_total, sizeof(*sorted_cpu), cmp_u32);
    qsort(sorted_gpu, gpu_count, sizeof(*sorted_gpu), cmp_u32);
    if (memcmp(sorted_cpu, sorted_gpu, cpu_total * sizeof(*sorted_cpu)) != 0) {
        fprintf(stderr, "[%s] %s: candidate set mismatch (missed or extra nonce)\n",
                label, what);
        return 1;
    }
    return 0;
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

    // Case 3: sparse target with a nonzero base -> the GPU must return the
    // COMPLETE match set, proving no matching nonce was missed in range.
    {
        const uint32_t base = 0x12340000u;
        const uint32_t count = 4096u;
        gbtc_make_synthetic_work(&work, count);
        work.nonce_base = base;
        for (int i = 0; i < 8; i++) work.target[i] = 0u;
        uint32_t cpu[GBTC_MAX_FOUND_NONCES + 1u];
        uint32_t target = 0, total = 0;
        if (pick_sparse_target(&work, &target, cpu, &total) != 0) {
            fprintf(stderr, "[%s] sparse case: no suitable target found\n", label);
            failures++;
        } else {
            work.target[0] = target;
            memset(&result, 0, sizeof(result));
            if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
                fprintf(stderr, "[%s] run_batch failed on sparse case\n", label);
                failures++;
            } else {
                if (result.hashes_done != count) {
                    fprintf(stderr, "[%s] sparse hashes_done=%llu want %u\n", label,
                            (unsigned long long)result.hashes_done, count);
                    failures++;
                }
                failures += check_sorted_set_equal(label, "sparse full-set",
                                                   cpu, total, result.nonces, result.count);
            }
        }
    }

    // Case 4: range edge (last 128 nonces up to 0xFFFFFFFF) with target
    // equality: the target equals one nonce's hash word, so that nonce must
    // be reported (<=, not <).
    {
        const uint32_t base = 0xffffff80u;
        const uint32_t count = 128u;
        gbtc_make_synthetic_work(&work, count);
        work.nonce_base = base;
        for (int i = 0; i < 8; i++) work.target[i] = 0u;
        uint32_t pivot = base;
        uint32_t pivot_word = 0xffffffffu;
        int found = 0;
        for (uint32_t i = 0; i < count; i++) {
            uint8_t hash[32];
            gbtc_work_hash(hash, &work, base + i);
            uint32_t w7 = ((uint32_t)hash[28] << 24) | ((uint32_t)hash[29] << 16) |
                          ((uint32_t)hash[30] << 8) | (uint32_t)hash[31];
            uint32_t w = bswap32(w7);
            if (w <= 0x04000000u) {
                pivot = base + i;
                pivot_word = w;
                found = 1;
                break;
            }
        }
        if (!found) {
            fprintf(stderr, "[%s] edge case: no small-word pivot nonce found\n", label);
            failures++;
        } else {
            work.target[0] = pivot_word;
            uint32_t cpu[GBTC_MAX_FOUND_NONCES + 1u];
            uint32_t total = cpu_collect_word0(&work, pivot_word, cpu,
                                               GBTC_MAX_FOUND_NONCES + 1u);
            if (total < 1 || total > GBTC_MAX_FOUND_NONCES) {
                fprintf(stderr, "[%s] edge case: unexpected cpu total=%u\n",
                        label, total);
                failures++;
            } else {
                memset(&result, 0, sizeof(result));
                if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
                    fprintf(stderr, "[%s] run_batch failed on edge case\n", label);
                    failures++;
                } else {
                    int seen = 0;
                    for (uint32_t i = 0; i < result.count; i++) {
                        if (result.nonces[i] == pivot) seen = 1;
                        if (result.nonces[i] < base) {
                            fprintf(stderr, "[%s] edge nonce %u below range\n",
                                    label, result.nonces[i]);
                            failures++;
                        }
                    }
                    if (!seen) {
                        fprintf(stderr, "[%s] edge case: pivot nonce %u missing\n",
                                label, pivot);
                        failures++;
                    }
                    failures += check_sorted_set_equal(label, "edge full-set",
                                                       cpu, total,
                                                       result.nonces, result.count);
                }
            }
        }
    }

    // Case 5: small overflow (128 matches, storage for 15) -> count clamps
    // at GBTC_MAX_FOUND_NONCES while hashes_done still reports full coverage.
    {
        const uint32_t count = 128u;
        gbtc_make_synthetic_work(&work, count);
        for (int i = 0; i < 8; i++) work.target[i] = 0xffffffffu;
        memset(&result, 0, sizeof(result));
        if (gbtc_opencl_backend.run_batch(&work, &result) != 0) {
            fprintf(stderr, "[%s] run_batch failed on small-overflow case\n", label);
            failures++;
        } else {
            if (result.hashes_done != count) {
                fprintf(stderr, "[%s] overflow hashes_done=%llu want %u\n", label,
                        (unsigned long long)result.hashes_done, count);
                failures++;
            }
            if (result.count != GBTC_MAX_FOUND_NONCES) {
                fprintf(stderr, "[%s] overflow count=%u want %u\n", label,
                        result.count, GBTC_MAX_FOUND_NONCES);
                failures++;
            }
        }
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
    failures += check_variant("pre3", NULL);
    if (failures == 0) printf("opencl conformance passed\n");
    return failures ? 1 : 0;
}
