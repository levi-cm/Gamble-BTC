#ifndef GBTC_GLES_TUNING_H
#define GBTC_GLES_TUNING_H

#include <stddef.h>
#include <stdint.h>

typedef enum {
    GBTC_GLES_KERNEL_UNROLLED,
    GBTC_GLES_KERNEL_PARTIAL,
    GBTC_GLES_KERNEL_LOOPED,
    GBTC_GLES_KERNEL_ALTBOOL,
    GBTC_GLES_KERNEL_DUALNONCE
} gbtc_gles_kernel_t;

typedef struct {
    gbtc_gles_kernel_t kernel;
    uint32_t local_size;
    int local_size_auto;
} gbtc_gles_config_t;

void gbtc_gles_config_defaults(gbtc_gles_config_t *cfg);
const char *gbtc_gles_kernel_name(gbtc_gles_kernel_t kernel);
uint32_t gbtc_gles_kernel_nonces_per_invocation(gbtc_gles_kernel_t kernel);
uint32_t gbtc_gles_invocation_nonce(gbtc_gles_kernel_t kernel, uint32_t nonce_base,
                                    uint32_t invocation_index, uint32_t lane);
uint32_t gbtc_gles_batch_quantum(const gbtc_gles_config_t *cfg);
int gbtc_gles_parse_kernel(const char *name, gbtc_gles_kernel_t *kernel);
int gbtc_gles_parse_local_size(const char *value, uint32_t *local_size,
                               int *is_auto, char *reason, size_t reason_cap);
int gbtc_gles_config_from_env(gbtc_gles_config_t *cfg, char *reason, size_t reason_cap);
char *gbtc_gles_build_kernel_src(const gbtc_gles_config_t *cfg,
                                 char *reason, size_t reason_cap);

#endif
