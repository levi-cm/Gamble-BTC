// Overflow recovery: when a batch reports more matches than fit in output
// storage, rescanning quantum-aligned subranges must recover the complete
// candidate set with no double counting. GPU-gated: skips when neither
// backend is available. Uses the backend that probes successfully.
#define _POSIX_C_SOURCE 200809L
#include "backend.h"
#include "bench.h"
#include "cuda.h"
#include "opencl.h"
#include "sha256.h"
#include "gpu_conformance_common.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern const gbtc_backend_t gbtc_cuda_backend;
extern const gbtc_backend_t gbtc_opencl_backend;

static int cmp_u32(const void *a, const void *b)
{
    uint32_t x = *(const uint32_t *)a, y = *(const uint32_t *)b;
    return (x > y) - (x < y);
}

int main(void)
{
    const gbtc_backend_t *backend = NULL;
    uint32_t quantum = 0;
    char reason[512] = "";
    if (gbtc_cuda_backend.probe(reason, sizeof(reason)) == 0) {
        if (!getenv("GBTC_CUDA_CUBIN") || !getenv("GBTC_CUDA_CUBIN")[0]) {
            setenv("GBTC_CUDA_CUBIN", "src/cuda/gbtc_mine.sm61.cubin", 1);
        }
        setenv("GBTC_CUDA_BLOCK", "64", 1);
        if (gbtc_cuda_backend.init(reason, sizeof(reason)) != 0) {
            fprintf(stderr, "cuda init failed: %s\n", reason);
            return 1;
        }
        backend = &gbtc_cuda_backend;
        quantum = 64;
    } else if (gbtc_opencl_backend.probe(reason, sizeof(reason)) == 0) {
        setenv("GBTC_OPENCL_KERNEL", "unrolled", 1);
        if (gbtc_opencl_backend.init(reason, sizeof(reason)) != 0) {
            fprintf(stderr, "opencl init failed: %s\n", reason);
            return 1;
        }
        backend = &gbtc_opencl_backend;
        quantum = 64;
    } else {
        printf("overflow recovery skipped: %s\n", reason);
        return 0;
    }
    printf("overflow device: %s (quantum=%u)\n", reason, quantum);

    // Find a target giving 16..64 matches over 4096 nonces: batch-level
    // overflow, but small chunks stay complete.
    gbtc_work_batch_t work;
    gbtc_make_synthetic_work(&work, 4096u);
    for (int i = 0; i < 8; i++) work.target[i] = 0u;
    uint32_t cpu_all[4096u];
    uint32_t cpu_total = 0, target = 0;
    static const uint32_t cands[] = {0x00080000u, 0x00100000u, 0x00200000u, 0x00400000u,
                                     0x00800000u, 0x01000000u, 0x02000000u};
    for (size_t i = 0; i < sizeof(cands) / sizeof(cands[0]); i++) {
        cpu_total = gbtc_conform_cpu_collect_word0(&work, cands[i], cpu_all, 4096u);
        if (cpu_total > GBTC_MAX_FOUND_NONCES && cpu_total <= 64u) {
            target = cands[i];
            break;
        }
    }
    if (!target) {
        fprintf(stderr, "no suitable overflow target (total=%u)\n", cpu_total);
        backend->shutdown();
        return 1;
    }
    work.target[0] = target;

    gbtc_backend_result_t result = {0};
    if (backend->run_batch(&work, &result) != 0) {
        fprintf(stderr, "batch failed\n");
        backend->shutdown();
        return 1;
    }
    if (!result.overflow || result.raw_count != cpu_total || result.count != GBTC_MAX_FOUND_NONCES) {
        fprintf(stderr, "want overflow raw=%u count=15, got overflow=%d raw=%u count=%u\n",
                cpu_total, result.overflow, result.raw_count, result.count);
        backend->shutdown();
        return 1;
    }
    printf("batch overflow confirmed: raw=%u stored=%u\n", result.raw_count, result.count);

    // Recover: partition into quantum-aligned chunks (same policy as the
    // miner: batch/16 rounded down to quantum).
    uint32_t chunk = work.nonce_count / 16u;
    chunk -= chunk % quantum;
    uint32_t recovered[4096u];
    uint32_t nrec = 0;
    uint64_t covered = 0;
    for (uint32_t off = 0; off < work.nonce_count; off += chunk) {
        uint32_t n = work.nonce_count - off;
        if (n > chunk) n = chunk;
        gbtc_work_batch_t sub = work;
        sub.nonce_base = work.nonce_base + off;
        sub.nonce_count = n;
        gbtc_backend_result_t r = {0};
        if (backend->run_batch(&sub, &r) != 0) {
            fprintf(stderr, "rescan chunk failed at off=%u\n", off);
            backend->shutdown();
            return 1;
        }
        if (r.overflow) {
            fprintf(stderr, "rescan chunk still overflows at off=%u\n", off);
            backend->shutdown();
            return 1;
        }
        for (uint32_t i = 0; i < r.count; i++) recovered[nrec++] = r.nonces[i];
        covered += r.hashes_done;
    }
    int failures = 0;
    if (covered != work.nonce_count) {
        fprintf(stderr, "rescan covered %llu want %u (double count or gap)\n",
                (unsigned long long)covered, work.nonce_count);
        failures++;
    }
    if (nrec != cpu_total) {
        fprintf(stderr, "recovered %u want %u\n", nrec, cpu_total);
        failures++;
    } else {
        qsort(recovered, nrec, sizeof(*recovered), cmp_u32);
        qsort(cpu_all, cpu_total, sizeof(*cpu_all), cmp_u32);
        if (memcmp(recovered, cpu_all, cpu_total * sizeof(*cpu_all)) != 0) {
            fprintf(stderr, "recovered set mismatch\n");
            failures++;
        }
    }
    backend->shutdown();
    if (!failures) printf("overflow recovery passed (target=0x%08x total=%u)\n", target, cpu_total);
    return failures ? 1 : 0;
}
