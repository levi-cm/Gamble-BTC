#ifndef GBTC_OPENCL_H
#define GBTC_OPENCL_H

#include <stddef.h>
#include <stdint.h>

typedef enum {
    GBTC_OCL_KERNEL_UNROLLED = 0,
    GBTC_OCL_KERNEL_LOOPED,
    GBTC_OCL_KERNEL_DUAL,
    // First block starts at round 3 from a host-precomputed job-level state
    // (rounds 0..2 are nonce-independent); feed-forward still uses the
    // original midstate. Baseline `unrolled` is unchanged.
    GBTC_OCL_KERNEL_PRE3,
} gbtc_ocl_kernel_t;

typedef struct {
    uint32_t local_size;
    int local_size_auto;
    uint32_t poll_us;
    // Requested SIMD/subgroup width; 0 means let IGC choose.
    uint32_t simd;
    gbtc_ocl_kernel_t kernel;
} gbtc_opencl_config_t;

void gbtc_opencl_config_defaults(gbtc_opencl_config_t *cfg);
int gbtc_opencl_config_from_env(gbtc_opencl_config_t *cfg, char *reason, size_t reason_cap);
uint32_t gbtc_opencl_batch_quantum(const gbtc_opencl_config_t *cfg);
uint32_t gbtc_opencl_active_local_size(void);
const char *gbtc_opencl_kernel_name(void);
char *gbtc_opencl_build_kernel_src(char *reason, size_t reason_cap);
void gbtc_opencl_device_strings(const char **vendor, const char **name,
                                const char **version);

#endif
