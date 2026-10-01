// gamble-btc CUDA backend for NVIDIA Pascal sm_61.
//
// Reached exclusively through dlopen("libcuda.so.1"), so the binary keeps
// zero link-time CUDA dependencies; the driver shared library is provided by
// the NVIDIA container runtime (--gpus all), same pattern as src/opencl.c.
// The kernel cubin (src/cuda/gbtc_mine.sm61.cubin, built by
// scripts/build-cuda-kernel.sh) ships as a data file and is loaded with the
// Driver API. The host context uses CU_CTX_SCHED_BLOCKING_SYNC so
// cuCtxSynchronize sleeps instead of spin-burning a CPU core.

#define _POSIX_C_SOURCE 200809L
#include "cuda.h"
#include "backend.h"

#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ---------------------------------------------------------------------------
// Minimal CUDA Driver API declarations (no headers needed, dlopen only).
// Exported libcuda.so.1 symbols carry the _v2 suffix.
// ---------------------------------------------------------------------------
typedef int CUresult;
typedef int CUdevice;
typedef struct CUctx_st *CUcontext;
typedef struct CUmod_st *CUmodule;
typedef struct CUfunc_st *CUfunction;
typedef unsigned long long CUdeviceptr;

#define CUDA_SUCCESS 0
#define CU_CTX_SCHED_BLOCKING_SYNC 0x04u
#define CU_FUNC_ATTRIBUTE_NUM_REGS 4

typedef struct {
    void *handle;
    CUresult (*Init)(unsigned);
    CUresult (*DeviceGetCount)(int *);
    CUresult (*DeviceGet)(CUdevice *, int);
    CUresult (*DeviceGetName)(char *, int, CUdevice);
    CUresult (*GetErrorString)(CUresult, const char **);
    CUresult (*CtxCreate)(CUcontext *, unsigned, CUdevice);
    CUresult (*CtxDestroy)(CUcontext);
    CUresult (*CtxSynchronize)(void);
    CUresult (*ModuleLoad)(CUmodule *, const char *);
    CUresult (*ModuleGetFunction)(CUfunction *, CUmodule, const char *);
    CUresult (*ModuleUnload)(CUmodule);
    CUresult (*MemAlloc)(CUdeviceptr *, size_t);
    CUresult (*MemFree)(CUdeviceptr);
    CUresult (*MemcpyHtoD)(CUdeviceptr, const void *, size_t);
    CUresult (*MemcpyDtoH)(void *, CUdeviceptr, size_t);
    CUresult (*LaunchKernel)(CUfunction, unsigned, unsigned, unsigned,
                             unsigned, unsigned, unsigned, unsigned,
                             void *, void **, void **);
    CUresult (*FuncGetAttribute)(int *, int, CUfunction);
} cu_api_t;

static cu_api_t g_cu;
static int g_cu_loaded = 0;

static void *load_sym(void *handle, const char *name)
{
    dlerror();
    void *sym = dlsym(handle, name);
    return dlerror() == NULL ? sym : NULL;
}

static int cu_load(char *reason, size_t reason_cap)
{
    if (g_cu_loaded) return 0;
    memset(&g_cu, 0, sizeof(g_cu));
    void *handle = dlopen("libcuda.so.1", RTLD_NOW | RTLD_LOCAL);
    if (!handle) {
        if (reason && reason_cap) {
            snprintf(reason, reason_cap, "dlopen libcuda.so.1 failed: %s",
                     dlerror() ? dlerror() : "unknown");
        }
        return -1;
    }
    struct { const char *name; size_t off; int required; } syms[] = {
        {"cuInit", 0, 1}, {"cuDeviceGetCount", 0, 1}, {"cuDeviceGet", 0, 1},
        {"cuDeviceGetName", 0, 0}, {"cuGetErrorString", 0, 0},
        {"cuCtxCreate_v2", 0, 1}, {"cuCtxDestroy_v2", 0, 1},
        {"cuCtxSynchronize", 0, 1}, {"cuModuleLoad", 0, 1},
        {"cuModuleGetFunction", 0, 1}, {"cuModuleUnload", 0, 1},
        {"cuMemAlloc_v2", 0, 1}, {"cuMemFree_v2", 0, 1},
        {"cuMemcpyHtoD_v2", 0, 1}, {"cuMemcpyDtoH_v2", 0, 1},
        {"cuLaunchKernel", 0, 1}, {"cuFuncGetAttribute", 0, 0},
    };
    void **slots[] = {
        (void **)&g_cu.Init, (void **)&g_cu.DeviceGetCount,
        (void **)&g_cu.DeviceGet, (void **)&g_cu.DeviceGetName,
        (void **)&g_cu.GetErrorString, (void **)&g_cu.CtxCreate,
        (void **)&g_cu.CtxDestroy, (void **)&g_cu.CtxSynchronize,
        (void **)&g_cu.ModuleLoad, (void **)&g_cu.ModuleGetFunction,
        (void **)&g_cu.ModuleUnload, (void **)&g_cu.MemAlloc,
        (void **)&g_cu.MemFree, (void **)&g_cu.MemcpyHtoD,
        (void **)&g_cu.MemcpyDtoH, (void **)&g_cu.LaunchKernel,
        (void **)&g_cu.FuncGetAttribute,
    };
    for (size_t i = 0; i < sizeof(syms) / sizeof(syms[0]); i++) {
        void *sym = load_sym(handle, syms[i].name);
        if (!sym && syms[i].required) {
            if (reason && reason_cap)
                snprintf(reason, reason_cap, "libcuda.so.1 has no %s", syms[i].name);
            dlclose(handle);
            return -1;
        }
        memcpy(slots[i], &sym, sizeof(sym));
    }
    g_cu.handle = handle;
    g_cu_loaded = 1;
    return 0;
}

// ---------------------------------------------------------------------------
// Config.
// ---------------------------------------------------------------------------
#define GBTC_CUDA_DEFAULT_BLOCK_SIZE 64u
#define GBTC_CUDA_CUBIN_DEFAULT "/usr/local/share/gbtc/gbtc_mine.sm61.cubin"

static gbtc_cuda_config_t g_cfg;

void gbtc_cuda_config_defaults(gbtc_cuda_config_t *cfg)
{
    cfg->block_size = GBTC_CUDA_DEFAULT_BLOCK_SIZE;
}

uint32_t gbtc_cuda_batch_quantum(const gbtc_cuda_config_t *cfg)
{
    return cfg->block_size;
}

uint32_t gbtc_cuda_active_block_size(void)
{
    return g_cfg.block_size;
}

const char *gbtc_cuda_kernel_name(void)
{
    return "cuda-sm61";
}

static int cuda_block_allowed(uint32_t v)
{
    return v == 32 || v == 64 || v == 128 || v == 256 || v == 512;
}

int gbtc_cuda_config_from_env(gbtc_cuda_config_t *cfg, char *reason, size_t reason_cap)
{
    gbtc_cuda_config_defaults(cfg);
    const char *value = getenv("GBTC_CUDA_BLOCK");
    if (value && value[0]) {
        char *end = NULL;
        errno = 0;
        unsigned long parsed = strtoul(value, &end, 10);
        if (errno || !end || *end != '\0' || parsed > UINT32_MAX ||
            !cuda_block_allowed((uint32_t)parsed)) {
            if (reason && reason_cap) {
                snprintf(reason, reason_cap,
                         "GBTC_CUDA_BLOCK must be 32, 64, 128, 256, or 512");
            }
            return -1;
        }
        cfg->block_size = (uint32_t)parsed;
    }
    return 0;
}

// ---------------------------------------------------------------------------
// Backend state.
// ---------------------------------------------------------------------------
static CUdevice g_dev = 0;
static CUcontext g_ctx = NULL;
static CUmodule g_mod = NULL;
static CUfunction g_fn = NULL;
static CUdeviceptr g_buf_in = 0;
static CUdeviceptr g_buf_out = 0;
static char g_dev_name[256] = "";
static char g_dev_info[512] = "";

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

static const char *cu_err(int rc)
{
    const char *s = NULL;
    if (g_cu.GetErrorString && g_cu.GetErrorString(rc, &s) == CUDA_SUCCESS && s) return s;
    return "unknown CUDA error";
}

void gbtc_cuda_device_strings(const char **vendor, const char **name,
                              const char **version)
{
    if (vendor) *vendor = "NVIDIA CUDA";
    if (name) *name = g_dev_name[0] ? g_dev_name : "unknown";
    if (version) *version = "CUDA Driver API";
}

static const char *cuda_cubin_path(void)
{
    const char *env = getenv("GBTC_CUDA_CUBIN");
    return (env && env[0]) ? env : GBTC_CUDA_CUBIN_DEFAULT;
}

static int cuda_probe(char *reason, size_t reason_cap)
{
    if (cu_load(reason, reason_cap) != 0) return -1;
    if (g_cu.Init(0) != CUDA_SUCCESS) {
        set_reason(reason, reason_cap, "cuInit failed");
        return -1;
    }
    int count = 0;
    if (g_cu.DeviceGetCount(&count) != CUDA_SUCCESS || count <= 0) {
        set_reason(reason, reason_cap, "no CUDA devices found");
        return -1;
    }
    CUdevice dev = 0;
    if (g_cu.DeviceGet(&dev, 0) != CUDA_SUCCESS) {
        set_reason(reason, reason_cap, "cuDeviceGet(0) failed");
        return -1;
    }
    char name[256] = "";
    if (g_cu.DeviceGetName) g_cu.DeviceGetName(name, sizeof(name), dev);
    set_reason(reason, reason_cap, "CUDA %s (devices=%d)",
               name[0] ? name : "device 0", count);
    return 0;
}

static void cuda_shutdown(void)
{
    if (g_buf_in) { g_cu.MemFree(g_buf_in); g_buf_in = 0; }
    if (g_buf_out) { g_cu.MemFree(g_buf_out); g_buf_out = 0; }
    if (g_mod) { g_cu.ModuleUnload(g_mod); g_mod = NULL; }
    g_fn = NULL;
    if (g_ctx) { g_cu.CtxDestroy(g_ctx); g_ctx = NULL; }
    g_dev_name[0] = '\0';
}

static int cuda_init(char *reason, size_t reason_cap)
{
    if (cu_load(reason, reason_cap) != 0) return -1;
    if (gbtc_cuda_config_from_env(&g_cfg, reason, reason_cap) != 0) return -1;
    cuda_shutdown();
    CUresult rc = g_cu.Init(0);
    if (rc != CUDA_SUCCESS) {
        set_reason(reason, reason_cap, "cuInit failed: %s", cu_err(rc));
        return -1;
    }
    rc = g_cu.DeviceGet(&g_dev, 0);
    if (rc != CUDA_SUCCESS) {
        set_reason(reason, reason_cap, "cuDeviceGet failed: %s", cu_err(rc));
        return -1;
    }
    if (g_cu.DeviceGetName) g_cu.DeviceGetName(g_dev_name, sizeof(g_dev_name), g_dev);
    // Blocking-sync context: the host sleeps in cuCtxSynchronize instead of
    // spinning a CPU core while the GPU works.
    rc = g_cu.CtxCreate(&g_ctx, CU_CTX_SCHED_BLOCKING_SYNC, g_dev);
    if (rc != CUDA_SUCCESS || !g_ctx) {
        set_reason(reason, reason_cap, "blocking-sync context failed: %s", cu_err(rc));
        cuda_shutdown();
        return -1;
    }
    rc = g_cu.ModuleLoad(&g_mod, cuda_cubin_path());
    if (rc != CUDA_SUCCESS || !g_mod) {
        set_reason(reason, reason_cap, "cubin load failed (%s): %s",
                   cuda_cubin_path(), cu_err(rc));
        cuda_shutdown();
        return -1;
    }
    rc = g_cu.ModuleGetFunction(&g_fn, g_mod, "gbtc_mine");
    if (rc != CUDA_SUCCESS || !g_fn) {
        set_reason(reason, reason_cap, "gbtc_mine not found: %s", cu_err(rc));
        cuda_shutdown();
        return -1;
    }
    int regs = 0;
    if (g_cu.FuncGetAttribute) g_cu.FuncGetAttribute(&regs, CU_FUNC_ATTRIBUTE_NUM_REGS, g_fn);
    rc = g_cu.MemAlloc(&g_buf_in, 16 * sizeof(uint32_t));
    if (rc != CUDA_SUCCESS) {
        set_reason(reason, reason_cap, "input alloc failed: %s", cu_err(rc));
        cuda_shutdown();
        return -1;
    }
    rc = g_cu.MemAlloc(&g_buf_out, 16 * sizeof(uint32_t));
    if (rc != CUDA_SUCCESS) {
        set_reason(reason, reason_cap, "output alloc failed: %s", cu_err(rc));
        cuda_shutdown();
        return -1;
    }
    snprintf(g_dev_info, sizeof(g_dev_info), "CUDA %s (block=%u regs=%d)",
             g_dev_name[0] ? g_dev_name : "device 0", g_cfg.block_size, regs);
    set_reason(reason, reason_cap, "%s", g_dev_info);
    return 0;
}

static int cuda_run_batch(const gbtc_work_batch_t *work, gbtc_backend_result_t *result)
{
    if (!work || !result || !g_fn || work->nonce_count == 0) return -1;
    if ((work->nonce_count % g_cfg.block_size) != 0) return -1;

    uint32_t in[16] = {0};
    memcpy(in + 0, work->midstate, 8 * sizeof(uint32_t));
    memcpy(in + 8, work->tail3, 3 * sizeof(uint32_t));
    in[11] = work->target[0];
    in[12] = work->nonce_base;

    CUresult rc = g_cu.MemcpyHtoD(g_buf_in, in, sizeof(in));
    if (rc != CUDA_SUCCESS) return -1;
    uint32_t zero = 0;
    rc = g_cu.MemcpyHtoD(g_buf_out, &zero, sizeof(zero));
    if (rc != CUDA_SUCCESS) return -1;

    void *args[] = { &g_buf_in, &g_buf_out };
    unsigned grid = work->nonce_count / g_cfg.block_size;
    rc = g_cu.LaunchKernel(g_fn, grid, 1, 1, g_cfg.block_size, 1, 1,
                           0, NULL, args, NULL);
    if (rc != CUDA_SUCCESS) return -1;
    rc = g_cu.CtxSynchronize();
    if (rc != CUDA_SUCCESS) return -1;

    uint32_t out[16] = {0};
    rc = g_cu.MemcpyDtoH(out, g_buf_out, sizeof(out));
    if (rc != CUDA_SUCCESS) return -1;

    result->count = out[0];
    if (result->count > GBTC_MAX_FOUND_NONCES) result->count = GBTC_MAX_FOUND_NONCES;
    for (uint32_t i = 0; i < result->count; i++) result->nonces[i] = out[1 + i];
    result->hashes_done = work->nonce_count;
    return 0;
}

const gbtc_backend_t gbtc_cuda_backend = {
    .kind = GBTC_BACKEND_CUDA,
    .name = "cuda",
    .api = "cuda",
    .probe = cuda_probe,
    .init = cuda_init,
    .run_batch = cuda_run_batch,
    .shutdown = cuda_shutdown,
};
