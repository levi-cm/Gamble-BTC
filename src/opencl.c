// gamble-btc OpenCL iGPU backend.
//
// Reached exclusively through dlopen("libOpenCL.so.1"), so the binary keeps
// zero link-time OpenCL dependencies and runs on any ICD present in the
// container (Mesa Rusticl, Intel NEO, ...). The compute kernel is the same
// unrolled SHA-256d nonce scan as the GLES path: work-item i tests
// nonce_base + i and reports matches through a shared counter.

#define _POSIX_C_SOURCE 200809L
#include "opencl.h"
#include "backend.h"

#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// ---------------------------------------------------------------------------
// Minimal OpenCL 1.2 declarations (no headers needed, dlopen only).
// ---------------------------------------------------------------------------
typedef int32_t cl_int;
typedef uint32_t cl_uint;
typedef uint64_t cl_ulong;
typedef uint32_t cl_bool;
typedef uint64_t cl_bitfield;
typedef intptr_t cl_context_properties;
typedef struct _cl_event *cl_event;
typedef struct _cl_platform_id *cl_platform_id;
typedef struct _cl_device_id *cl_device_id;
typedef struct _cl_context *cl_context;
typedef struct _cl_command_queue *cl_command_queue;
typedef struct _cl_mem *cl_mem;
typedef struct _cl_program *cl_program;
typedef struct _cl_kernel *cl_kernel;
typedef size_t cl_device_type;

#define CL_SUCCESS 0
#define CL_TRUE 1
#define CL_FALSE 0
#define CL_DEVICE_TYPE_GPU ((cl_device_type)(1 << 2))
#define CL_MEM_READ_WRITE ((cl_bitfield)(1 << 0))
#define CL_MEM_READ_ONLY ((cl_bitfield)(1 << 2))
#define CL_PLATFORM_NAME 0x0902u
#define CL_DEVICE_NAME 0x102Bu
#define CL_DEVICE_VENDOR 0x102Cu
#define CL_DEVICE_VERSION 0x102Fu
#define CL_PROGRAM_BUILD_LOG 0x1183u
#define CL_KERNEL_WORK_GROUP_SIZE 0x11B0u
#define CL_EVENT_COMMAND_EXECUTION_STATUS 0x11D3u
#define CL_COMPLETE 0
#define CL_DEVICE_EXTENSIONS 0x1030u
#define CL_QUEUE_PROPERTIES 0x1093u
#define CL_QUEUE_THROTTLE_LOW_KHR ((cl_bitfield)(1 << 2))

typedef struct {
    void *handle;
    cl_int (*GetPlatformIDs)(cl_uint, cl_platform_id *, cl_uint *);
    cl_int (*GetPlatformInfo)(cl_platform_id, cl_uint, size_t, void *, size_t *);
    cl_int (*GetDeviceIDs)(cl_platform_id, cl_device_type, cl_uint, cl_device_id *, cl_uint *);
    cl_int (*GetDeviceInfo)(cl_device_id, cl_uint, size_t, void *, size_t *);
    cl_context (*CreateContext)(const cl_context_properties *, cl_uint, const cl_device_id *,
                                void (*)(const char *, const void *, size_t, void *),
                                void *, cl_int *);
    cl_command_queue (*CreateCommandQueue)(cl_context, cl_device_id, cl_bitfield, cl_int *);
    cl_command_queue (*CreateCommandQueueWithProperties)(cl_context, cl_device_id,
                                                         const intptr_t *, cl_int *);
    cl_mem (*CreateBuffer)(cl_context, cl_bitfield, size_t, void *, cl_int *);
    cl_program (*CreateProgramWithSource)(cl_context, cl_uint, const char **, const size_t *, cl_int *);
    cl_int (*BuildProgram)(cl_program, cl_uint, const cl_device_id *, const char *,
                           void (*)(cl_program, void *), void *);
    cl_int (*GetProgramBuildInfo)(cl_program, cl_device_id, cl_uint, size_t, void *, size_t *);
    cl_kernel (*CreateKernel)(cl_program, const char *, cl_int *);
    cl_int (*SetKernelArg)(cl_kernel, cl_uint, size_t, const void *);
    cl_int (*EnqueueWriteBuffer)(cl_command_queue, cl_mem, cl_bool, size_t, size_t,
                                 const void *, cl_uint, const void *, void *);
    cl_int (*EnqueueNDRangeKernel)(cl_command_queue, cl_kernel, cl_uint, const size_t *,
                                   const size_t *, const size_t *, cl_uint, const void *, void *);
    cl_int (*EnqueueReadBuffer)(cl_command_queue, cl_mem, cl_bool, size_t, size_t,
                                void *, cl_uint, const void *, void *);
    cl_int (*Finish)(cl_command_queue);
    cl_int (*Flush)(cl_command_queue);
    cl_int (*GetEventInfo)(cl_event, cl_uint, size_t, void *, size_t *);
    cl_int (*ReleaseEvent)(cl_event);
    cl_int (*GetKernelWorkGroupInfo)(cl_kernel, cl_device_id, cl_uint, size_t, void *, size_t *);
    cl_int (*ReleaseKernel)(cl_kernel);
    cl_int (*ReleaseProgram)(cl_program);
    cl_int (*ReleaseMemObject)(cl_mem);
    cl_int (*ReleaseCommandQueue)(cl_command_queue);
    cl_int (*ReleaseContext)(cl_context);
} cl_api_t;

static cl_api_t g_cl;
static int g_cl_loaded = 0;

static void *load_sym(void *handle, const char *name)
{
    dlerror();
    void *sym = dlsym(handle, name);
    return dlerror() == NULL ? sym : NULL;
}

static int cl_load(char *reason, size_t reason_cap)
{
    if (g_cl_loaded) return 0;
    memset(&g_cl, 0, sizeof(g_cl));
    void *handle = dlopen("libOpenCL.so.1", RTLD_NOW | RTLD_LOCAL);
    if (!handle) {
        if (reason && reason_cap) {
            snprintf(reason, reason_cap, "dlopen libOpenCL.so.1 failed: %s",
                     dlerror() ? dlerror() : "unknown");
        }
        return -1;
    }
#define LOAD(fn)                                                        \
    do {                                                                \
        void *sym_ = load_sym(handle, "cl" #fn);                        \
        if (!sym_) {                                                    \
            if (reason && reason_cap)                                   \
                snprintf(reason, reason_cap, "libOpenCL.so.1 has no cl" #fn); \
            dlclose(handle);                                            \
            return -1;                                                  \
        }                                                               \
        memcpy(&g_cl.fn, &sym_, sizeof(sym_));                          \
    } while (0)
    LOAD(GetPlatformIDs);
    LOAD(GetPlatformInfo);
    LOAD(GetDeviceIDs);
    LOAD(GetDeviceInfo);
    LOAD(CreateContext);
    LOAD(CreateCommandQueue);
    /* Optional: absent on old ICDs, guarded at use. */
    {
        void *sym_ = load_sym(handle, "clCreateCommandQueueWithProperties");
        memcpy(&g_cl.CreateCommandQueueWithProperties, &sym_, sizeof(sym_));
    }
    LOAD(CreateBuffer);
    LOAD(CreateProgramWithSource);
    LOAD(BuildProgram);
    LOAD(GetProgramBuildInfo);
    LOAD(CreateKernel);
    LOAD(SetKernelArg);
    LOAD(EnqueueWriteBuffer);
    LOAD(EnqueueNDRangeKernel);
    LOAD(EnqueueReadBuffer);
    LOAD(Finish);
    /* Optional event-poll path (kills driver spin-wait); guarded at use. */
    {
        void *sym_ = load_sym(handle, "clFlush");
        memcpy(&g_cl.Flush, &sym_, sizeof(sym_));
        sym_ = load_sym(handle, "clGetEventInfo");
        memcpy(&g_cl.GetEventInfo, &sym_, sizeof(sym_));
        sym_ = load_sym(handle, "clReleaseEvent");
        memcpy(&g_cl.ReleaseEvent, &sym_, sizeof(sym_));
    }
    LOAD(GetKernelWorkGroupInfo);
    LOAD(ReleaseKernel);
    LOAD(ReleaseProgram);
    LOAD(ReleaseMemObject);
    LOAD(ReleaseCommandQueue);
    LOAD(ReleaseContext);
#undef LOAD
    g_cl.handle = handle;
    g_cl_loaded = 1;
    return 0;
}

// ---------------------------------------------------------------------------
// Config.
// ---------------------------------------------------------------------------
#define GBTC_OPENCL_DEFAULT_LOCAL_SIZE 64u
#define GBTC_OPENCL_DEFAULT_POLL_US 1000u
#define GBTC_OPENCL_SOURCE_CAP (256u * 1024u)

static gbtc_opencl_config_t g_cfg;

void gbtc_opencl_config_defaults(gbtc_opencl_config_t *cfg)
{
    cfg->local_size = GBTC_OPENCL_DEFAULT_LOCAL_SIZE;
    cfg->local_size_auto = 0;
    cfg->poll_us = GBTC_OPENCL_DEFAULT_POLL_US;
    cfg->simd = 0;
    cfg->kernel = GBTC_OCL_KERNEL_UNROLLED;
}

const char *gbtc_opencl_kernel_name(void)
{
    switch (g_cfg.kernel) {
    case GBTC_OCL_KERNEL_LOOPED: return "ocl-looped";
    case GBTC_OCL_KERNEL_DUAL: return "ocl-dual";
    case GBTC_OCL_KERNEL_PRE3: return "ocl-pre3";
    case GBTC_OCL_KERNEL_UNROLLED:
    default: return "ocl-unrolled";
    }
}

uint32_t gbtc_opencl_batch_quantum(const gbtc_opencl_config_t *cfg)
{
    uint32_t q = cfg->local_size;
    if (cfg->kernel == GBTC_OCL_KERNEL_DUAL) q *= 2u;
    return q;
}

static int opencl_local_size_allowed(uint32_t v)
{
    return v == 8 || v == 16 || v == 32 || v == 64 ||
           v == 128 || v == 256 || v == 512;
}

static int opencl_simd_allowed(uint32_t v)
{
    return v == 0 || v == 8 || v == 16 || v == 32;
}

static int opencl_parse_kernel(const char *value, gbtc_ocl_kernel_t *out)
{
    if (strcmp(value, "unrolled") == 0) { *out = GBTC_OCL_KERNEL_UNROLLED; return 0; }
    if (strcmp(value, "looped") == 0) { *out = GBTC_OCL_KERNEL_LOOPED; return 0; }
    if (strcmp(value, "dual") == 0) { *out = GBTC_OCL_KERNEL_DUAL; return 0; }
    if (strcmp(value, "pre3") == 0) { *out = GBTC_OCL_KERNEL_PRE3; return 0; }
    return -1;
}

int gbtc_opencl_config_from_env(gbtc_opencl_config_t *cfg, char *reason, size_t reason_cap)
{
    gbtc_opencl_config_defaults(cfg);
    const char *value = getenv("GBTC_OPENCL_LOCAL_SIZE");
    if (value && value[0]) {
        if (strcmp(value, "auto") == 0) {
            cfg->local_size_auto = 1;
        } else {
            char *end = NULL;
            errno = 0;
            unsigned long parsed = strtoul(value, &end, 10);
            if (errno || !end || *end != '\0' || parsed > UINT32_MAX ||
                !opencl_local_size_allowed((uint32_t)parsed)) {
                if (reason && reason_cap) {
                    snprintf(reason, reason_cap,
                             "GBTC_OPENCL_LOCAL_SIZE must be auto, 8, 16, 32, 64, 128, 256, or 512");
                }
                return -1;
            }
            cfg->local_size = (uint32_t)parsed;
        }
    }
    // Completion poll interval: the feeder thread sleeps this long between
    // event-status checks instead of spin-waiting in the driver.
    value = getenv("GBTC_OPENCL_POLL_US");
    if (value && value[0]) {
        char *end = NULL;
        errno = 0;
        unsigned long parsed = strtoul(value, &end, 10);
        if (errno || !end || *end != '\0' || parsed < 100 || parsed > 50000) {
            if (reason && reason_cap) {
                snprintf(reason, reason_cap,
                         "GBTC_OPENCL_POLL_US must be an integer from 100 to 50000 microseconds");
            }
            return -1;
        }
        cfg->poll_us = (uint32_t)parsed;
    }
    // SIMD/subgroup width request; emitted as intel_reqd_sub_group_size.
    value = getenv("GBTC_OPENCL_SIMD");
    if (value && value[0]) {
        if (strcmp(value, "auto") == 0) {
            cfg->simd = 0;
        } else {
            char *end = NULL;
            errno = 0;
            unsigned long parsed = strtoul(value, &end, 10);
            if (errno || !end || *end != '\0' || parsed > UINT32_MAX ||
                !opencl_simd_allowed((uint32_t)parsed)) {
                if (reason && reason_cap) {
                    snprintf(reason, reason_cap,
                             "GBTC_OPENCL_SIMD must be auto, 8, 16, or 32");
                }
                return -1;
            }
            cfg->simd = (uint32_t)parsed;
        }
    }
    value = getenv("GBTC_OPENCL_KERNEL");
    if (value && value[0]) {
        if (opencl_parse_kernel(value, &cfg->kernel) != 0) {
            if (reason && reason_cap) {
                snprintf(reason, reason_cap,
                         "GBTC_OPENCL_KERNEL must be unrolled, looped, dual, or pre3");
            }
            return -1;
        }
    }
    return 0;
}

// ---------------------------------------------------------------------------
// Kernel source generation (unrolled SHA-256d, one nonce per work-item).
//
// Input:  I[0..7] midstate, I[8..10] header words 16..18, I[11] target word 0,
//         I[12] nonce base, I[13..15] pad.
// Output: O[0] match count, O[1..15] matching nonces.
// ---------------------------------------------------------------------------
typedef struct {
    char *buf;
    size_t cap;
    size_t len;
    int failed;
} sb_t;

static void sb_appendf(sb_t *sb, const char *fmt, ...)
{
    if (sb->failed || sb->len >= sb->cap) {
        sb->failed = 1;
        return;
    }
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(sb->buf + sb->len, sb->cap - sb->len, fmt, ap);
    va_end(ap);
    if (n < 0 || (size_t)n >= sb->cap - sb->len) {
        sb->failed = 1;
        return;
    }
    sb->len += (size_t)n;
}

static const uint32_t SHA_K[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

static uint32_t pre_rotr(uint32_t x, unsigned n)
{
    return (x >> n) | (x << (32u - n));
}

// Host-side job-level precompute: the first 3 compression rounds of the
// header-final block. Rounds 0..2 consume only W0..W2 (tail3, job-fixed), so
// the result S3 is nonce-independent. The ORIGINAL midstate is still needed
// separately for the feed-forward step; S3 is not a replacement for it.
// Recomputed per batch for simplicity (3 rounds vs an ~88 ms GPU batch);
// it is invariant across nonce_base within a job sweep.
static void sha_prefix3(uint32_t out[8], const uint32_t mid[8],
                        const uint32_t tail3[3])
{
    uint32_t a = mid[0], b = mid[1], c = mid[2], d = mid[3];
    uint32_t e = mid[4], f = mid[5], g = mid[6], h = mid[7];
    for (int t = 0; t < 3; t++) {
        uint32_t s1 = pre_rotr(e, 6) ^ pre_rotr(e, 11) ^ pre_rotr(e, 25);
        uint32_t ch = (e & f) ^ (~e & g);
        uint32_t t1 = h + s1 + ch + SHA_K[t] + tail3[t];
        uint32_t s0 = pre_rotr(a, 2) ^ pre_rotr(a, 13) ^ pre_rotr(a, 22);
        uint32_t mj = (a & b) ^ (a & c) ^ (b & c);
        uint32_t t2 = s0 + mj;
        h = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }
    out[0] = a; out[1] = b; out[2] = c; out[3] = d;
    out[4] = e; out[5] = f; out[6] = g; out[7] = h;
}

// Emit compression rounds [t_first, t_first + t_count) for one block using
// rotating variable names. Each round right-rotates the name ring by one; on
// return slot_names[i] names the variable holding the i-th state word
// (a..h) after the last emitted round. Callers needing only e pass a
// non-NULL e_name_out instead.
static void emit_unrolled_rounds(sb_t *sb, const char *const vnames[8],
                                 const char *const wnames[16],
                                 int t_first, int t_count,
                                 const char **slot_names_out,
                                 const char **e_name_out)
{
    const char *v[8];
    for (int i = 0; i < 8; i++) v[i] = vnames[i];

    for (int t = t_first; t < t_first + t_count; t++) {
        const char *wt = wnames[t & 15];
        if (t >= 16) {
            int s = t & 15, s2 = (t - 2) & 15, s7 = (t - 7) & 15, s15 = (t - 15) & 15;
            sb_appendf(sb, "  %s = %s + SMALLSIG0(%s) + %s + SMALLSIG1(%s);\n",
                       wnames[s], wnames[s], wnames[s15], wnames[s7], wnames[s2]);
            wt = wnames[s];
        }
        sb_appendf(sb,
            "  t1 = %s + BIGSIG1(%s) + CH(%s,%s,%s) + 0x%08xu + %s;\n"
            "  t2 = BIGSIG0(%s) + MAJ(%s,%s,%s);\n"
            "  %s = %s + t1; %s = t1 + t2;\n",
            v[7], v[4], v[4], v[5], v[6], SHA_K[t], wt,
            v[0], v[0], v[1], v[2],
            v[3], v[3], v[7]);
        const char *na = v[7], *ne = v[3];
        v[7] = v[6]; v[6] = v[5]; v[5] = v[4]; v[4] = ne;
        v[3] = v[2]; v[2] = v[1]; v[1] = v[0]; v[0] = na;
    }
    if (slot_names_out) {
        for (int i = 0; i < 8; i++) slot_names_out[i] = v[i];
    }
    if (e_name_out) *e_name_out = v[4];
}

// Emit the 64-round compression for one block using rotating variable names.
// After 64 rounds (a multiple of 8) the working variables are back in their
// original name order and hold the resulting digest words.
static void emit_unrolled_block(sb_t *sb, const char *const vnames[8],
                                const char *const wnames[16])
{
    emit_unrolled_rounds(sb, vnames, wnames, 0, 64, NULL, NULL);
}

// Emit one complete SHA-256d nonce test (two compression rounds) with names
// suffixed so several nonces can share a kernel body. Caller declares
// `t1`/`t2` scratch and passes an expression evaluating to the nonce, which is
// also what gets recorded on a match.
static void emit_nonce_unrolled(sb_t *sb, const char *nonce_expr, const char *sfx)
{
    char vn[8][12], wn[16][12];
    const char *vnames[8], *wnames[16];
    for (int i = 0; i < 8; i++) {
        snprintf(vn[i], sizeof(vn[i]), "%c%s", "abcdefgh"[i], sfx);
        vnames[i] = vn[i];
    }
    for (int i = 0; i < 16; i++) {
        snprintf(wn[i], sizeof(wn[i]), "w%d%s", i, sfx);
        wnames[i] = wn[i];
    }

    sb_appendf(sb, "  uint %s = I[8], %s = I[9], %s = I[10], %s = %s;\n",
               wnames[0], wnames[1], wnames[2], wnames[3], nonce_expr);
    sb_appendf(sb, "  uint %s = 0x80000000u", wnames[4]);
    for (int i = 5; i < 15; i++) sb_appendf(sb, ", %s = 0u", wnames[i]);
    sb_appendf(sb, ", %s = 640u;\n", wnames[15]);
    sb_appendf(sb, "  uint %s = I[0], %s = I[1], %s = I[2], %s = I[3];\n",
               vnames[0], vnames[1], vnames[2], vnames[3]);
    sb_appendf(sb, "  uint %s = I[4], %s = I[5], %s = I[6], %s = I[7];\n",
               vnames[4], vnames[5], vnames[6], vnames[7]);

    emit_unrolled_block(sb, vnames, wnames);

    // Fold the midstate into the digest, then run the second block.
    for (int i = 0; i < 8; i++) {
        sb_appendf(sb, "  %s = I[%d] + %s;\n", wnames[i], i, vnames[i]);
    }
    sb_appendf(sb, "  %s = 0x80000000u", wnames[8]);
    for (int i = 9; i < 15; i++) sb_appendf(sb, ", %s = 0u", wnames[i]);
    sb_appendf(sb, ", %s = 256u;\n", wnames[15]);
    sb_appendf(sb, "  %s = 0x6a09e667u; %s = 0xbb67ae85u;"
                   " %s = 0x3c6ef372u; %s = 0xa54ff53au;\n",
               vnames[0], vnames[1], vnames[2], vnames[3]);
    sb_appendf(sb, "  %s = 0x510e527fu; %s = 0x9b05688cu;"
                   " %s = 0x1f83d9abu; %s = 0x5be0cd19u;\n",
               vnames[4], vnames[5], vnames[6], vnames[7]);

    emit_unrolled_block(sb, vnames, wnames);

    sb_appendf(sb,
        "  if (BSWAP32(0x5be0cd19u + %s) <= I[11]) {\n"
        "    uint idx%s = atomic_inc(O);\n"
        "    if (idx%s < 15u) O[idx%s + 1u] = %s;\n"
        "  }\n",
        vnames[7], sfx, sfx, sfx, nonce_expr);
}

// Prefix-3 variant: the first block starts at round 3 from the host-computed
// job-level state S3 (I[0..7]); W0..W2 still come from tail3 (I[16..18])
// because the message expansion needs them. Feed-forward uses the ORIGINAL
// midstate (I[8..15]), loaded after the rounds so it is not live during
// them. Layout: S[0..7] mid[8..15] tail[16..18] target[19] noncebase[20].
static void emit_nonce_pre3(sb_t *sb, const char *nonce_expr)
{
    char vn[8][12], wn[16][12];
    const char *vnames[8], *wnames[16];
    for (int i = 0; i < 8; i++) {
        snprintf(vn[i], sizeof(vn[i]), "%c", "abcdefgh"[i]);
        vnames[i] = vn[i];
    }
    for (int i = 0; i < 16; i++) {
        snprintf(wn[i], sizeof(wn[i]), "w%d", i);
        wnames[i] = wn[i];
    }

    sb_appendf(sb, "  uint w0 = I[16], w1 = I[17], w2 = I[18], w3 = %s;\n",
               nonce_expr);
    sb_appendf(sb, "  uint w4 = 0x80000000u");
    for (int i = 5; i < 15; i++) sb_appendf(sb, ", w%d = 0u", i);
    sb_appendf(sb, ", w15 = 640u;\n");
    sb_appendf(sb, "  uint a = I[0], b = I[1], c = I[2], d = I[3];\n");
    sb_appendf(sb, "  uint e = I[4], f = I[5], g = I[6], h = I[7];\n");

    const char *slots[8] = {0};
    emit_unrolled_rounds(sb, vnames, wnames, 3, 61, slots, NULL);

    // Fold the ORIGINAL midstate into the digest, then run the second block.
    for (int i = 0; i < 8; i++) {
        sb_appendf(sb, "  w%d = I[%d] + %s;\n", i, 8 + i, slots[i]);
    }
    sb_appendf(sb, "  w8 = 0x80000000u");
    for (int i = 9; i < 15; i++) sb_appendf(sb, ", w%d = 0u", i);
    sb_appendf(sb, ", w15 = 256u;\n");
    sb_appendf(sb, "  a = 0x6a09e667u; b = 0xbb67ae85u;"
                   " c = 0x3c6ef372u; d = 0xa54ff53au;\n");
    sb_appendf(sb, "  e = 0x510e527fu; f = 0x9b05688cu;"
                   " g = 0x1f83d9abu; h = 0x5be0cd19u;\n");

    emit_unrolled_block(sb, vnames, wnames);

    sb_appendf(sb,
        "  if (BSWAP32(0x5be0cd19u + h) <= I[19]) {\n"
        "    uint idx = atomic_inc(O);\n"
        "    if (idx < 15u) O[idx + 1u] = %s;\n"
        "  }\n",
        nonce_expr);
}

// Compact round-loop variant: far less generated code and fewer live values
// are visible to the compiler than the fully unrolled form.
static void emit_nonce_looped(sb_t *sb, const char *nonce_expr)
{
    sb_appendf(sb,
        "  uint w[16];\n"
        "  uint a, b, c, d, e, f, g, h;\n"
        "  const uint K[64] = {");
    for (int i = 0; i < 64; i++) {
        sb_appendf(sb, "%s0x%08xu", i ? ", " : "", SHA_K[i]);
    }
    sb_appendf(sb,
        "};\n"
        "  w[0] = I[8]; w[1] = I[9]; w[2] = I[10]; w[3] = %s;\n"
        "  w[4] = 0x80000000u; w[5] = 0u; w[6] = 0u; w[7] = 0u;\n"
        "  w[8] = 0u; w[9] = 0u; w[10] = 0u; w[11] = 0u;\n"
        "  w[12] = 0u; w[13] = 0u; w[14] = 0u; w[15] = 640u;\n"
        "  a = I[0]; b = I[1]; c = I[2]; d = I[3];\n"
        "  e = I[4]; f = I[5]; g = I[6]; h = I[7];\n",
        nonce_expr);
    for (int block = 0; block < 2; block++) {
        if (block == 1) {
            sb_appendf(sb,
                "  w[0] = I[0] + a; w[1] = I[1] + b; w[2] = I[2] + c; w[3] = I[3] + d;\n"
                "  w[4] = I[4] + e; w[5] = I[5] + f; w[6] = I[6] + g; w[7] = I[7] + h;\n"
                "  w[8] = 0x80000000u; w[9] = 0u; w[10] = 0u; w[11] = 0u;\n"
                "  w[12] = 0u; w[13] = 0u; w[14] = 0u; w[15] = 256u;\n"
                "  a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;\n"
                "  e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;\n");
        }
        sb_appendf(sb,
            "  for (int i = 0; i < 64; i++) {\n"
            "    uint wt = w[i & 15];\n"
            "    if (i >= 16) {\n"
            "      w[i & 15] = wt + SMALLSIG0(w[(i - 15) & 15]) + w[(i - 7) & 15]\n"
            "                + SMALLSIG1(w[(i - 2) & 15]);\n"
            "      wt = w[i & 15];\n"
            "    }\n"
            "    uint lt1 = h + BIGSIG1(e) + CH(e, f, g) + K[i] + wt;\n"
            "    uint lt2 = BIGSIG0(a) + MAJ(a, b, c);\n"
            "    h = g; g = f; f = e; e = d + lt1;\n"
            "    d = c; c = b; b = a; a = lt1 + lt2;\n"
            "  }\n");
    }
    sb_appendf(sb,
        "  if (BSWAP32(0x5be0cd19u + h) <= I[11]) {\n"
        "    uint idx = atomic_inc(O);\n"
        "    if (idx < 15u) O[idx + 1u] = %s;\n"
        "  }\n",
        nonce_expr);
}

char *gbtc_opencl_build_kernel_src(char *reason, size_t reason_cap)
{
    char *buf = calloc(1, GBTC_OPENCL_SOURCE_CAP);
    if (!buf) {
        if (reason && reason_cap) snprintf(reason, reason_cap, "out of memory");
        return NULL;
    }
    sb_t sb = { .buf = buf, .cap = GBTC_OPENCL_SOURCE_CAP, .len = 0, .failed = 0 };

    sb_appendf(&sb,
        "#define ROR(x,n) (((x)>>(n))|((x)<<(32u-(n))))\n"
        "#define BIGSIG0(x) (ROR(x,2u)^ROR(x,13u)^ROR(x,22u))\n"
        "#define BIGSIG1(x) (ROR(x,6u)^ROR(x,11u)^ROR(x,25u))\n"
        "#define SMALLSIG0(x) (ROR(x,7u)^ROR(x,18u)^((x)>>3u))\n"
        "#define SMALLSIG1(x) (ROR(x,17u)^ROR(x,19u)^((x)>>10u))\n"
        "#define CH(x,y,z) (((x)&(y))^(~(x)&(z)))\n"
        "#define MAJ(x,y,z) (((x)&(y))^((x)&(z))^((y)&(z)))\n"
        "#define BSWAP32(x) (((x)>>24u)|(((x)>>8u)&0x0000ff00u)|(((x)<<8u)&0x00ff0000u)|((x)<<24u))\n");

    if (g_cfg.simd != 0) {
        sb_appendf(&sb, "#define GBTC_SIMD_ATTR "
                         "__attribute__((intel_reqd_sub_group_size(%u)))\n", g_cfg.simd);
    } else {
        sb_appendf(&sb, "#define GBTC_SIMD_ATTR\n");
    }
    // Telling IGC the exact work-group size lets it specialize dispatch and
    // register allocation. Only possible when the size is explicit, not auto
    // (auto is resolved after the program is built).
    if (!g_cfg.local_size_auto) {
        sb_appendf(&sb, "#define GBTC_WG_ATTR "
                         "__attribute__((reqd_work_group_size(%u, 1, 1)))\n",
                   g_cfg.local_size);
    } else {
        sb_appendf(&sb, "#define GBTC_WG_ATTR\n");
    }

    sb_appendf(&sb,
        "GBTC_SIMD_ATTR\n"
        "GBTC_WG_ATTR\n"
        "__kernel void gbtc_mine(__global const uint *I, __global uint *O) {\n"
        "  uint gid = get_global_id(0);\n"
        "  uint t1, t2;\n"
        "  (void)t1; (void)t2;\n");

    if (g_cfg.kernel == GBTC_OCL_KERNEL_DUAL) {
        // Two adjacent nonces per work-item: halves the global size and
        // amortizes per-item setup, at the cost of register pressure.
        emit_nonce_unrolled(&sb, "I[12] + (gid << 1u)", "0");
        emit_nonce_unrolled(&sb, "I[12] + (gid << 1u) + 1u", "1");
    } else if (g_cfg.kernel == GBTC_OCL_KERNEL_LOOPED) {
        emit_nonce_looped(&sb, "I[12] + gid");
    } else if (g_cfg.kernel == GBTC_OCL_KERNEL_PRE3) {
        emit_nonce_pre3(&sb, "I[20] + gid");
    } else {
        emit_nonce_unrolled(&sb, "I[12] + gid", "");
    }
    sb_appendf(&sb, "}\n");

    if (sb.failed) {
        free(buf);
        if (reason && reason_cap) snprintf(reason, reason_cap, "generated OpenCL source overflowed");
        return NULL;
    }
    return buf;
}

// ---------------------------------------------------------------------------
// Backend state.
// ---------------------------------------------------------------------------
static cl_platform_id g_platform = NULL;
static cl_device_id g_device = NULL;
static cl_context g_ctx = NULL;
static cl_command_queue g_queue = NULL;
static cl_program g_program = NULL;
static cl_kernel g_kernel = NULL;
static cl_mem g_buf_in = NULL;
static cl_mem g_buf_out = NULL;
static char g_dev_info[512] = "";
static const char *g_queue_mode = "default";
static int g_ocl_debug = -1;

static int ocl_debug_enabled(void)
{
    if (g_ocl_debug < 0) {
        const char *v = getenv("GBTC_OPENCL_DEBUG");
        g_ocl_debug = (v && v[0] != '0') ? 1 : 0;
    }
    return g_ocl_debug;
}

#define OCL_DBG(step, err)                                               \
    do {                                                                 \
        if (ocl_debug_enabled())                                         \
            fprintf(stderr, "[opencl-debug] %s: err=%d\n", step, (int)(err)); \
    } while (0)
static char g_plat_name[256] = "";
static char g_dev_name[256] = "";
static char g_dev_ver[256] = "";

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

uint32_t gbtc_opencl_active_local_size(void)
{
    return g_cfg.local_size;
}

void gbtc_opencl_device_strings(const char **vendor, const char **name,
                                const char **version)
{
    // ICD loader exposes no separate vendor string here; reuse platform name.
    if (vendor) *vendor = g_plat_name[0] ? g_plat_name : "OpenCL";
    if (name) *name = g_dev_name[0] ? g_dev_name : "unknown";
    if (version) *version = g_dev_ver[0] ? g_dev_ver : "unknown";
}

static int platform_is_fallback(const char *plat)
{
    // Mesa Rusticl / Clover stay available as fallbacks; a vendor driver
    // (Intel NEO, ...) wins when present.
    return plat && (strstr(plat, "rusticl") || strstr(plat, "Rusticl") ||
                    strstr(plat, "Clover") || strstr(plat, "clover"));
}

static int try_pick_pass(cl_platform_id *platforms, cl_uint nplatforms, int skip_fallback)
{
    const char *want = getenv("GBTC_OPENCL_PLATFORM");
    if (want && !want[0]) want = NULL;
    for (cl_uint p = 0; p < nplatforms; p++) {
        char plat[256] = "";
        size_t n = 0;
        g_cl.GetPlatformInfo(platforms[p], CL_PLATFORM_NAME, sizeof(plat), plat, &n);
        if (want && !strstr(plat, want)) continue;
        if (!want && skip_fallback && platform_is_fallback(plat)) continue;
        cl_uint ndevs = 0;
        if (g_cl.GetDeviceIDs(platforms[p], CL_DEVICE_TYPE_GPU, 0, NULL, &ndevs) != CL_SUCCESS ||
            ndevs == 0) {
            continue;
        }
        cl_device_id *devs = calloc(ndevs, sizeof(*devs));
        if (!devs) return -2;
        if (g_cl.GetDeviceIDs(platforms[p], CL_DEVICE_TYPE_GPU, ndevs, devs, NULL) != CL_SUCCESS) {
            free(devs);
            continue;
        }
        char dev[256] = "", ver[256] = "";
        g_cl.GetDeviceInfo(devs[0], CL_DEVICE_NAME, sizeof(dev), dev, &n);
        g_cl.GetDeviceInfo(devs[0], CL_DEVICE_VERSION, sizeof(ver), ver, &n);
        snprintf(g_plat_name, sizeof(g_plat_name), "%s", plat[0] ? plat : "?");
        snprintf(g_dev_name, sizeof(g_dev_name), "%s", dev[0] ? dev : "?");
        snprintf(g_dev_ver, sizeof(g_dev_ver), "%s", ver[0] ? ver : "?");
        snprintf(g_dev_info, sizeof(g_dev_info), "OpenCL %.160s / %.160s / %.160s",
                 g_plat_name, g_dev_name, g_dev_ver);
        g_platform = platforms[p];
        g_device = devs[0];
        free(devs);
        return 0;
    }
    return -1;
}

static int pick_gpu_device(char *reason, size_t reason_cap)
{
    cl_uint nplatforms = 0;
    if (g_cl.GetPlatformIDs(0, NULL, &nplatforms) != CL_SUCCESS || nplatforms == 0) {
        set_reason(reason, reason_cap, "no OpenCL platforms found");
        return -1;
    }
    cl_platform_id *platforms = calloc(nplatforms, sizeof(*platforms));
    if (!platforms) {
        set_reason(reason, reason_cap, "out of memory");
        return -1;
    }
    int rc = -1;
    if (g_cl.GetPlatformIDs(nplatforms, platforms, NULL) != CL_SUCCESS) {
        set_reason(reason, reason_cap, "clGetPlatformIDs failed");
        goto out;
    }
    // Prefer a vendor driver (Intel NEO) over Mesa fallback ICDs.
    rc = try_pick_pass(platforms, nplatforms, 1);
    if (rc == -2) {
        set_reason(reason, reason_cap, "out of memory");
        goto out;
    }
    if (rc != 0) rc = try_pick_pass(platforms, nplatforms, 0);
    if (rc == -2) {
        set_reason(reason, reason_cap, "out of memory");
        rc = -1;
        goto out;
    }
    if (rc != 0) set_reason(reason, reason_cap, "no OpenCL GPU device found");
out:
    free(platforms);
    return rc;
}

static size_t opencl_input_words(void)
{
    return g_cfg.kernel == GBTC_OCL_KERNEL_PRE3 ? 24u : 16u;
}

static int opencl_probe(char *reason, size_t reason_cap)
{    if (cl_load(reason, reason_cap) != 0) return -1;
    char saved[sizeof(g_dev_info)];
    snprintf(saved, sizeof(saved), "%s", g_dev_info);
    cl_platform_id saved_plat = g_platform;
    cl_device_id saved_dev = g_device;
    if (pick_gpu_device(reason, reason_cap) != 0) return -1;
    set_reason(reason, reason_cap, "%s", g_dev_info);
    // Restore any previous pick; init() re-picks deterministically anyway.
    snprintf(g_dev_info, sizeof(g_dev_info), "%s", saved);
    g_platform = saved_plat;
    g_device = saved_dev;
    return 0;
}

static void opencl_shutdown(void)
{
    if (g_kernel) { g_cl.ReleaseKernel(g_kernel); g_kernel = NULL; }
    if (g_program) { g_cl.ReleaseProgram(g_program); g_program = NULL; }
    if (g_buf_in) { g_cl.ReleaseMemObject(g_buf_in); g_buf_in = NULL; }
    if (g_buf_out) { g_cl.ReleaseMemObject(g_buf_out); g_buf_out = NULL; }
    if (g_queue) { g_cl.ReleaseCommandQueue(g_queue); g_queue = NULL; }
    g_queue_mode = "default";
    if (g_ctx) { g_cl.ReleaseContext(g_ctx); g_ctx = NULL; }
    g_platform = NULL;
    g_device = NULL;
}

static int opencl_init(char *reason, size_t reason_cap)
{
    if (cl_load(reason, reason_cap) != 0) return -1;
    if (gbtc_opencl_config_from_env(&g_cfg, reason, reason_cap) != 0) return -1;
    opencl_shutdown();
    if (pick_gpu_device(reason, reason_cap) != 0) return -1;

    cl_int err = CL_SUCCESS;
    g_ctx = g_cl.CreateContext(NULL, 1, &g_device, NULL, NULL, &err);
    if (!g_ctx || err != CL_SUCCESS) {
        set_reason(reason, reason_cap, "clCreateContext failed (%d)", err);
        opencl_shutdown();
        return -1;
    }
    g_queue = NULL;
    err = CL_SUCCESS;
    // Power-effective waits: CL_QUEUE_THROTTLE_LOW_KHR makes the driver's
    // completion wait sleep instead of spin-burning a CPU core. Without
    // cl_khr_throttle_hints (or without the 2.0 entry point) fall back to a
    // plain queue; the feeder thread then burns CPU in NEO's poll loop.
    g_queue_mode = "default-no-entrypoint";
    if (g_cl.CreateCommandQueueWithProperties) {
        char exts[8192] = "";
        size_t n = 0;
        cl_int qerr = g_cl.GetDeviceInfo(g_device, CL_DEVICE_EXTENSIONS,
                                         sizeof(exts) - 1, exts, &n);
        if (qerr != CL_SUCCESS || n == 0) {
            g_queue_mode = "default-extquery-failed";
        } else if (!strstr(exts, "cl_khr_throttle_hints")) {
            g_queue_mode = "default-no-throttle-ext";
        } else {
            const intptr_t props[] = {
                (intptr_t)CL_QUEUE_PROPERTIES,
                (intptr_t)CL_QUEUE_THROTTLE_LOW_KHR,
                0
            };
            g_queue = g_cl.CreateCommandQueueWithProperties(g_ctx, g_device, props, &err);
            if (!g_queue || err != CL_SUCCESS) {
                g_queue_mode = "default-throttle-create-failed";
                g_queue = NULL;
            } else {
                g_queue_mode = "throttle-low";
            }
        }
    }
    if (!g_queue) {
        g_queue = g_cl.CreateCommandQueue(g_ctx, g_device, 0, &err);
    }
    if (!g_queue || err != CL_SUCCESS) {
        set_reason(reason, reason_cap, "command queue creation failed (%d)", err);
        opencl_shutdown();
        return -1;
    }

    char *src = gbtc_opencl_build_kernel_src(reason, reason_cap);
    if (!src) {
        opencl_shutdown();
        return -1;
    }
    const char *srcs[1] = { src };
    g_program = g_cl.CreateProgramWithSource(g_ctx, 1, srcs, NULL, &err);
    if (!g_program || err != CL_SUCCESS) {
        set_reason(reason, reason_cap, "clCreateProgramWithSource failed (%d)", err);
        free(src);
        opencl_shutdown();
        return -1;
    }
    err = g_cl.BuildProgram(g_program, 1, &g_device, "-cl-std=CL1.2", NULL, NULL);
    if (err != CL_SUCCESS) {
        char log[4096] = "";
        size_t n = 0;
        g_cl.GetProgramBuildInfo(g_program, g_device, CL_PROGRAM_BUILD_LOG,
                                 sizeof(log) - 1, log, &n);
        set_reason(reason, reason_cap, "OpenCL build failed (%d): %.*s",
                   err, (int)sizeof(log) - 1, log);
        free(src);
        opencl_shutdown();
        return -1;
    }
    free(src);

    g_kernel = g_cl.CreateKernel(g_program, "gbtc_mine", &err);
    if (!g_kernel || err != CL_SUCCESS) {
        set_reason(reason, reason_cap, "clCreateKernel failed (%d)", err);
        opencl_shutdown();
        return -1;
    }
    size_t max_wg = 0;
    if (g_cl.GetKernelWorkGroupInfo(g_kernel, g_device, CL_KERNEL_WORK_GROUP_SIZE,
                                    sizeof(max_wg), &max_wg, NULL) != CL_SUCCESS || max_wg == 0) {
        set_reason(reason, reason_cap, "clGetKernelWorkGroupInfo failed");
        opencl_shutdown();
        return -1;
    }
    if (g_cfg.local_size_auto) {
        uint32_t v = 64;
        while (v > max_wg && v > 8) v /= 2;
        if (v > max_wg) {
            set_reason(reason, reason_cap, "device max work-group size %zu too small", max_wg);
            opencl_shutdown();
            return -1;
        }
        g_cfg.local_size = v;
    } else if (g_cfg.local_size > max_wg) {
        set_reason(reason, reason_cap, "local_size %u exceeds device max %zu",
                   g_cfg.local_size, max_wg);
        opencl_shutdown();
        return -1;
    }

    g_buf_in = g_cl.CreateBuffer(g_ctx, CL_MEM_READ_ONLY,
                                   opencl_input_words() * sizeof(cl_uint), NULL, &err);
    if (!g_buf_in || err != CL_SUCCESS) {
        set_reason(reason, reason_cap, "input buffer failed (%d)", err);
        opencl_shutdown();
        return -1;
    }
    g_buf_out = g_cl.CreateBuffer(g_ctx, CL_MEM_READ_WRITE, 16 * sizeof(cl_uint), NULL, &err);
    if (!g_buf_out || err != CL_SUCCESS) {
        set_reason(reason, reason_cap, "output buffer failed (%d)", err);
        opencl_shutdown();
        return -1;
    }
    set_reason(reason, reason_cap, "OpenCL pipeline ready on %s (local_size=%u max_wg=%zu queue=%s)",
               g_dev_info, g_cfg.local_size, max_wg, g_queue_mode);
    return 0;
}

static size_t opencl_global_size(const gbtc_work_batch_t *work)
{
    return g_cfg.kernel == GBTC_OCL_KERNEL_DUAL
               ? (size_t)(work->nonce_count / 2u)
               : (size_t)work->nonce_count;
}

static int opencl_run_batch_blocking(const gbtc_work_batch_t *work,
                                       const cl_uint *in, cl_uint *out)
{
    // Fallback for ICDs without the event API: correct, but NEO's
    // completion wait spin-burns a CPU core.
    cl_uint zero = 0;
    cl_int err = g_cl.EnqueueWriteBuffer(g_queue, g_buf_in, CL_TRUE, 0,
                                         opencl_input_words() * sizeof(cl_uint),
                                         in, 0, NULL, NULL);
    if (err != CL_SUCCESS) return -1;
    err = g_cl.EnqueueWriteBuffer(g_queue, g_buf_out, CL_TRUE, 0, sizeof(zero),
                                  &zero, 0, NULL, NULL);
    if (err != CL_SUCCESS) return -1;
    size_t global = opencl_global_size(work);
    size_t local = g_cfg.local_size;
    err = g_cl.EnqueueNDRangeKernel(g_queue, g_kernel, 1, NULL, &global, &local,
                                    0, NULL, NULL);
    if (err != CL_SUCCESS) return -1;
    if (g_cl.Finish(g_queue) != CL_SUCCESS) return -1;
    err = g_cl.EnqueueReadBuffer(g_queue, g_buf_out, CL_TRUE, 0, 16 * sizeof(cl_uint),
                                 out, 0, NULL, NULL);
    return err == CL_SUCCESS ? 0 : -1;
}

static int event_is_complete(cl_event event)
{
    cl_int status = 0;
    cl_int err = g_cl.GetEventInfo(event, CL_EVENT_COMMAND_EXECUTION_STATUS,
                                   sizeof(status), &status, NULL);
    OCL_DBG("get-event-info", err);
    if (err != CL_SUCCESS) return -1;
    if (status < 0) return -1;
    return status == CL_COMPLETE;
}

static void sleep_us(uint32_t microseconds)
{
    struct timespec req;
    req.tv_sec = (time_t)(microseconds / 1000000u);
    req.tv_nsec = (long)((microseconds % 1000000u) * 1000u);
    nanosleep(&req, NULL);
}

static int opencl_run_batch_polled(const gbtc_work_batch_t *work,
                                   const cl_uint *in, cl_uint *out)
{
    // Preferred path: chain the whole batch behind events, flush once, then
    // sleep-poll the kernel event. The feeder thread burns ~0 CPU while the
    // iGPU works; the final blocking read returns immediately.
    cl_event ev_w0 = NULL, ev_w1 = NULL, ev_k = NULL;
    cl_int err = g_cl.EnqueueWriteBuffer(g_queue, g_buf_in, CL_FALSE, 0,
                                         opencl_input_words() * sizeof(cl_uint),
                                         in, 0, NULL, &ev_w0);
    OCL_DBG("write-in", err);
    if (err != CL_SUCCESS || !ev_w0) goto fail;
    cl_uint zero = 0;
    err = g_cl.EnqueueWriteBuffer(g_queue, g_buf_out, CL_FALSE, 0, sizeof(zero),
                                  &zero, 1, &ev_w0, &ev_w1);
    OCL_DBG("write-zero", err);
    if (err != CL_SUCCESS || !ev_w1) goto fail;
    size_t global = opencl_global_size(work);
    size_t local = g_cfg.local_size;
    err = g_cl.EnqueueNDRangeKernel(g_queue, g_kernel, 1, NULL, &global, &local,
                                    1, &ev_w1, &ev_k);
    OCL_DBG("ndrange", err);
    if (err != CL_SUCCESS || !ev_k) goto fail;
    err = g_cl.Flush(g_queue);
    OCL_DBG("flush", err);
    if (err != CL_SUCCESS) goto fail;

    for (;;) {
        int done = event_is_complete(ev_k);
        if (done < 0) { OCL_DBG("event-info", -999); goto fail; }
        if (done) break;
        sleep_us(g_cfg.poll_us);
    }

    err = g_cl.EnqueueReadBuffer(g_queue, g_buf_out, CL_TRUE, 0, 16 * sizeof(cl_uint),
                                 out, 0, NULL, NULL);
    OCL_DBG("read", err);
    g_cl.ReleaseEvent(ev_w0);
    g_cl.ReleaseEvent(ev_w1);
    g_cl.ReleaseEvent(ev_k);
    return err == CL_SUCCESS ? 0 : -1;

fail:
    if (ev_w0) g_cl.ReleaseEvent(ev_w0);
    if (ev_w1) g_cl.ReleaseEvent(ev_w1);
    if (ev_k) g_cl.ReleaseEvent(ev_k);
    return -1;
}

static int opencl_run_batch(const gbtc_work_batch_t *work, gbtc_backend_result_t *result)
{
    if (!work || !result || !g_kernel || work->nonce_count == 0) return -1;
    if ((work->nonce_count % gbtc_opencl_batch_quantum(&g_cfg)) != 0) return -1;

    cl_uint in[24] = {0};
    if (g_cfg.kernel == GBTC_OCL_KERNEL_PRE3) {
        uint32_t s3[8];
        sha_prefix3(s3, work->midstate, work->tail3);
        memcpy(in + 0, s3, 8 * sizeof(cl_uint));
        memcpy(in + 8, work->midstate, 8 * sizeof(cl_uint));
        memcpy(in + 16, work->tail3, 3 * sizeof(cl_uint));
        in[19] = work->target[0];
        in[20] = work->nonce_base;
    } else {
        memcpy(in + 0, work->midstate, 8 * sizeof(cl_uint));
        memcpy(in + 8, work->tail3, 3 * sizeof(cl_uint));
        in[11] = work->target[0];
        in[12] = work->nonce_base;
    }

    cl_int err = g_cl.SetKernelArg(g_kernel, 0, sizeof(g_buf_in), &g_buf_in);
    if (err != CL_SUCCESS) return -1;
    err = g_cl.SetKernelArg(g_kernel, 1, sizeof(g_buf_out), &g_buf_out);
    if (err != CL_SUCCESS) return -1;

    int have_events = g_cl.Flush && g_cl.GetEventInfo && g_cl.ReleaseEvent;
    cl_uint out[16] = {0};
    int rc = have_events ? opencl_run_batch_polled(work, in, out)
                         : opencl_run_batch_blocking(work, in, out);
    if (rc != 0) return -1;

    result->count = out[0];
    if (result->count > GBTC_MAX_FOUND_NONCES) result->count = GBTC_MAX_FOUND_NONCES;
    for (uint32_t i = 0; i < result->count; i++) result->nonces[i] = out[1 + i];
    result->hashes_done = work->nonce_count;
    return 0;
}

const gbtc_backend_t gbtc_opencl_backend = {
    .kind = GBTC_BACKEND_OPENCL,
    .name = "opencl",
    .api = "opencl",
    .probe = opencl_probe,
    .init = opencl_init,
    .run_batch = opencl_run_batch,
    .shutdown = opencl_shutdown,
};
