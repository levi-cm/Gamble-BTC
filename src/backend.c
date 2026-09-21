#include "backend.h"

#include <stdio.h>
#include <string.h>

const char *gbtc_backend_kind_name(gbtc_backend_kind_t kind)
{
    switch (kind) {
    case GBTC_BACKEND_GLES: return "gles";
    case GBTC_BACKEND_VULKAN: return "vulkan";
    case GBTC_BACKEND_OPENCL: return "opencl";
    }
    return "unknown";
}

int gbtc_backend_is_auto(const char *name)
{
    return name == NULL || name[0] == '\0' || strcmp(name, "auto") == 0;
}

int gbtc_parse_backend_kind(const char *name, gbtc_backend_kind_t *kind)
{
    if (!name || !kind) return -1;
    if (strcmp(name, "gles") == 0) {
        *kind = GBTC_BACKEND_GLES;
        return 0;
    }
    if (strcmp(name, "vulkan") == 0) {
        *kind = GBTC_BACKEND_VULKAN;
        return 0;
    }
    if (strcmp(name, "opencl") == 0) {
        *kind = GBTC_BACKEND_OPENCL;
        return 0;
    }
    return -1;
}

static int unavailable_probe(char *reason, size_t reason_cap)
{
    snprintf(reason, reason_cap, "backend is not implemented in this build yet");
    return -1;
}

static int unavailable_init(char *reason, size_t reason_cap)
{
    return unavailable_probe(reason, reason_cap);
}

static int unavailable_run(const gbtc_work_batch_t *work, gbtc_backend_result_t *result)
{
    (void)work;
    (void)result;
    return -1;
}

static void unavailable_shutdown(void)
{
}

const gbtc_backend_t gbtc_vulkan_backend = {
    .kind = GBTC_BACKEND_VULKAN,
    .name = "vulkan",
    .api = "vulkan",
    .probe = unavailable_probe,
    .init = unavailable_init,
    .run_batch = unavailable_run,
    .shutdown = unavailable_shutdown,
};

// The OpenCL backend is implemented in src/opencl.c (gbtc_opencl_backend).
