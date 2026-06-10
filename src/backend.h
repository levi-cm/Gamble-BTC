#ifndef GBTC_BACKEND_H
#define GBTC_BACKEND_H

#include <stddef.h>
#include <stdint.h>

#define GBTC_MAX_FOUND_NONCES 15u

typedef enum {
    GBTC_BACKEND_GLES,
    GBTC_BACKEND_VULKAN,
    GBTC_BACKEND_OPENCL
} gbtc_backend_kind_t;

typedef struct {
    uint32_t midstate[8];
    uint32_t tail3[3];
    uint32_t nonce_base;
    uint32_t nonce_count;
} gbtc_work_batch_t;

typedef struct {
    uint32_t count;
    uint32_t nonces[GBTC_MAX_FOUND_NONCES];
    uint64_t hashes_done;
} gbtc_backend_result_t;

typedef struct gbtc_backend {
    gbtc_backend_kind_t kind;
    const char *name;
    const char *api;
    int (*probe)(char *reason, size_t reason_cap);
    int (*init)(char *reason, size_t reason_cap);
    int (*run_batch)(const gbtc_work_batch_t *work, gbtc_backend_result_t *result);
    void (*shutdown)(void);
} gbtc_backend_t;

const char *gbtc_backend_kind_name(gbtc_backend_kind_t kind);
int gbtc_parse_backend_kind(const char *name, gbtc_backend_kind_t *kind);
int gbtc_backend_is_auto(const char *name);

extern const gbtc_backend_t gbtc_vulkan_backend;
extern const gbtc_backend_t gbtc_gles_backend;
extern const gbtc_backend_t gbtc_opencl_backend;

#endif
