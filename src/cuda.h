#ifndef GBTC_CUDA_H
#define GBTC_CUDA_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    // Threads per block; also the batch dispatch quantum.
    uint32_t block_size;
} gbtc_cuda_config_t;

void gbtc_cuda_config_defaults(gbtc_cuda_config_t *cfg);
int gbtc_cuda_config_from_env(gbtc_cuda_config_t *cfg, char *reason, size_t reason_cap);
uint32_t gbtc_cuda_batch_quantum(const gbtc_cuda_config_t *cfg);
uint32_t gbtc_cuda_active_block_size(void);
const char *gbtc_cuda_kernel_name(void);
void gbtc_cuda_device_strings(const char **vendor, const char **name,
                              const char **version);

#endif
