#include "gles_tuning.h"

#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define GBTC_GLES_DEFAULT_LOCAL_SIZE 64u
#define GBTC_GLES_SOURCE_CAP (512u * 1024u)

typedef struct {
    char *buf;
    size_t cap;
    size_t len;
    int failed;
} sb_t;

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

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

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

void gbtc_gles_config_defaults(gbtc_gles_config_t *cfg)
{
    cfg->kernel = GBTC_GLES_KERNEL_UNROLLED;
    cfg->local_size = GBTC_GLES_DEFAULT_LOCAL_SIZE;
    cfg->local_size_auto = 0;
}

const char *gbtc_gles_kernel_name(gbtc_gles_kernel_t kernel)
{
    switch (kernel) {
    case GBTC_GLES_KERNEL_UNROLLED: return "unrolled";
    case GBTC_GLES_KERNEL_PARTIAL: return "partial";
    case GBTC_GLES_KERNEL_LOOPED: return "looped";
    case GBTC_GLES_KERNEL_ALTBOOL: return "altbool";
    case GBTC_GLES_KERNEL_DUALNONCE: return "dualnonce";
    }
    return "unknown";
}

uint32_t gbtc_gles_kernel_nonces_per_invocation(gbtc_gles_kernel_t kernel)
{
    return kernel == GBTC_GLES_KERNEL_DUALNONCE ? 2u : 1u;
}

uint32_t gbtc_gles_invocation_nonce(gbtc_gles_kernel_t kernel, uint32_t nonce_base,
                                    uint32_t invocation_index, uint32_t lane)
{
    if (kernel == GBTC_GLES_KERNEL_DUALNONCE) {
        return nonce_base + invocation_index * 2u + (lane ? 1u : 0u);
    }
    (void)lane;
    return nonce_base + invocation_index;
}

uint32_t gbtc_gles_batch_quantum(const gbtc_gles_config_t *cfg)
{
    return cfg->local_size * gbtc_gles_kernel_nonces_per_invocation(cfg->kernel);
}

int gbtc_gles_parse_kernel(const char *name, gbtc_gles_kernel_t *kernel)
{
    if (!kernel) return -1;
    if (!name || !name[0]) {
        *kernel = GBTC_GLES_KERNEL_UNROLLED;
        return 0;
    }
    if (strcmp(name, "unrolled") == 0) {
        *kernel = GBTC_GLES_KERNEL_UNROLLED;
        return 0;
    }
    if (strcmp(name, "partial") == 0) {
        *kernel = GBTC_GLES_KERNEL_PARTIAL;
        return 0;
    }
    if (strcmp(name, "looped") == 0) {
        *kernel = GBTC_GLES_KERNEL_LOOPED;
        return 0;
    }
    if (strcmp(name, "altbool") == 0) {
        *kernel = GBTC_GLES_KERNEL_ALTBOOL;
        return 0;
    }
    if (strcmp(name, "dualnonce") == 0) {
        *kernel = GBTC_GLES_KERNEL_DUALNONCE;
        return 0;
    }
    return -1;
}

static int local_size_allowed(uint32_t local_size)
{
    switch (local_size) {
    case 8:
    case 16:
    case 32:
    case 64:
    case 128:
    case 256:
        return 1;
    default:
        return 0;
    }
}

int gbtc_gles_parse_local_size(const char *value, uint32_t *local_size,
                               int *is_auto, char *reason, size_t reason_cap)
{
    if (!local_size || !is_auto) {
        set_reason(reason, reason_cap, "invalid local-size parser arguments");
        return -1;
    }
    *local_size = GBTC_GLES_DEFAULT_LOCAL_SIZE;
    *is_auto = 0;

    if (!value || !value[0]) return 0;
    if (strcmp(value, "auto") == 0) {
        *is_auto = 1;
        return 0;
    }

    char *end = NULL;
    errno = 0;
    unsigned long parsed = strtoul(value, &end, 10);
    if (errno || !end || *end != '\0' || parsed > UINT32_MAX ||
        !local_size_allowed((uint32_t)parsed)) {
        set_reason(reason, reason_cap,
                   "GBTC_GLES_LOCAL_SIZE must be auto, 8, 16, 32, 64, 128, or 256");
        return -1;
    }
    *local_size = (uint32_t)parsed;
    return 0;
}

int gbtc_gles_config_from_env(gbtc_gles_config_t *cfg, char *reason, size_t reason_cap)
{
    gbtc_gles_config_defaults(cfg);
    if (gbtc_gles_parse_kernel(getenv("GBTC_GLES_KERNEL"), &cfg->kernel) != 0) {
        set_reason(reason, reason_cap,
                   "GBTC_GLES_KERNEL must be unrolled, partial, looped, altbool, or dualnonce");
        return -1;
    }
    return gbtc_gles_parse_local_size(getenv("GBTC_GLES_LOCAL_SIZE"),
                                      &cfg->local_size, &cfg->local_size_auto,
                                      reason, reason_cap);
}

static void append_prefix(sb_t *sb, const gbtc_gles_config_t *cfg)
{
    sb_appendf(sb,
        "#version 320 es\n"
        "layout(local_size_x = %u) in;\n"
        "layout(std430, binding = 0) readonly buffer In {\n"
        "  uint midstate[8];\n"
        "  uint w0_in; uint w1_in; uint w2_in;\n"
        "  uint nonce_base;\n"
        "  uint pad[3];\n"
        "} I;\n"
        "layout(std430, binding = 1) buffer Out {\n"
        "  uint count;\n"
        "  uint nonces[15];\n"
        "} O;\n"
        "#define ROR(x,n) (((x)>>(n))|((x)<<(32u-(n))))\n"
        "#define BIGSIG0(x) (ROR(x,2u)^ROR(x,13u)^ROR(x,22u))\n"
        "#define BIGSIG1(x) (ROR(x,6u)^ROR(x,11u)^ROR(x,25u))\n"
        "#define SMALLSIG0(x) (ROR(x,7u)^ROR(x,18u)^((x)>>3u))\n"
        "#define SMALLSIG1(x) (ROR(x,17u)^ROR(x,19u)^((x)>>10u))\n",
        cfg->local_size);
    if (cfg->kernel == GBTC_GLES_KERNEL_ALTBOOL) {
        sb_appendf(sb,
            "#define CH(x,y,z) ((z)^((x)&((y)^(z))))\n"
            "#define MAJ(x,y,z) (((x)&(y))|((z)&((x)|(y))))\n");
    } else {
        sb_appendf(sb,
            "#define CH(x,y,z) (((x)&(y))^(~(x)&(z)))\n"
            "#define MAJ(x,y,z) (((x)&(y))^((x)&(z))^((y)&(z)))\n");
    }
}

static void append_unrolled(sb_t *sb, const gbtc_gles_config_t *cfg)
{
    const char *wnames[16] = {
        "w0","w1","w2","w3","w4","w5","w6","w7",
        "w8","w9","w10","w11","w12","w13","w14","w15"
    };
    append_prefix(sb, cfg);
    sb_appendf(sb,
        "void main() {\n"
        "  uint global_idx = gl_GlobalInvocationID.x + gl_GlobalInvocationID.y * (gl_NumWorkGroups.x * gl_WorkGroupSize.x);\n"
        "  uint nonce = I.nonce_base + global_idx;\n"
        "  uint t1, t2;\n"
        "  uint w0 = I.w0_in;\n"
        "  uint w1 = I.w1_in;\n"
        "  uint w2 = I.w2_in;\n"
        "  uint w3 = nonce;\n"
        "  uint w4 = 0x80000000u;\n"
        "  uint w5 = 0u; uint w6 = 0u; uint w7 = 0u; uint w8 = 0u;\n"
        "  uint w9 = 0u; uint w10 = 0u; uint w11 = 0u; uint w12 = 0u;\n"
        "  uint w13 = 0u; uint w14 = 0u; uint w15 = 640u;\n"
        "  uint a = I.midstate[0]; uint b = I.midstate[1]; uint c = I.midstate[2]; uint d = I.midstate[3];\n"
        "  uint e = I.midstate[4]; uint f = I.midstate[5]; uint g = I.midstate[6]; uint h = I.midstate[7];\n");

    const char *v1[8] = {"a","b","c","d","e","f","g","h"};
    for (int t = 0; t < 64; t++) {
        const char *wt;
        if (t < 16) {
            wt = wnames[t];
        } else {
            int s = t & 15;
            int s2 = (t - 2) & 15;
            int s7 = (t - 7) & 15;
            int s15 = (t - 15) & 15;
            sb_appendf(sb, "  %s = %s + SMALLSIG0(%s) + %s + SMALLSIG1(%s);\n",
                       wnames[s], wnames[s], wnames[s15], wnames[s7], wnames[s2]);
            wt = wnames[s];
        }
        sb_appendf(sb,
            "  t1 = %s + BIGSIG1(%s) + CH(%s,%s,%s) + 0x%08xu + %s;\n"
            "  t2 = BIGSIG0(%s) + MAJ(%s,%s,%s);\n"
            "  %s = %s + t1; %s = t1 + t2;\n",
            v1[7], v1[4], v1[4], v1[5], v1[6], SHA_K[t], wt,
            v1[0], v1[0], v1[1], v1[2],
            v1[3], v1[3], v1[7]);
        const char *na = v1[7], *ne = v1[3];
        v1[7] = v1[6]; v1[6] = v1[5]; v1[5] = v1[4]; v1[4] = ne;
        v1[3] = v1[2]; v1[2] = v1[1]; v1[1] = v1[0]; v1[0] = na;
    }
    sb_appendf(sb,
        "  uint h1_0 = I.midstate[0] + %s;\n"
        "  uint h1_1 = I.midstate[1] + %s;\n"
        "  uint h1_2 = I.midstate[2] + %s;\n"
        "  uint h1_3 = I.midstate[3] + %s;\n"
        "  uint h1_4 = I.midstate[4] + %s;\n"
        "  uint h1_5 = I.midstate[5] + %s;\n"
        "  uint h1_6 = I.midstate[6] + %s;\n"
        "  uint h1_7 = I.midstate[7] + %s;\n"
        "  w0 = h1_0; w1 = h1_1; w2 = h1_2; w3 = h1_3;\n"
        "  w4 = h1_4; w5 = h1_5; w6 = h1_6; w7 = h1_7;\n"
        "  w8 = 0x80000000u; w9 = 0u; w10 = 0u; w11 = 0u;\n"
        "  w12 = 0u; w13 = 0u; w14 = 0u; w15 = 256u;\n"
        "  a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;\n"
        "  e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;\n",
        v1[0], v1[1], v1[2], v1[3], v1[4], v1[5], v1[6], v1[7]);

    const char *v2[8] = {"a","b","c","d","e","f","g","h"};
    for (int t = 0; t < 64; t++) {
        const char *wt;
        if (t < 16) {
            wt = wnames[t];
        } else {
            int s = t & 15;
            int s2 = (t - 2) & 15;
            int s7 = (t - 7) & 15;
            int s15 = (t - 15) & 15;
            sb_appendf(sb, "  %s = %s + SMALLSIG0(%s) + %s + SMALLSIG1(%s);\n",
                       wnames[s], wnames[s], wnames[s15], wnames[s7], wnames[s2]);
            wt = wnames[s];
        }
        sb_appendf(sb,
            "  t1 = %s + BIGSIG1(%s) + CH(%s,%s,%s) + 0x%08xu + %s;\n"
            "  t2 = BIGSIG0(%s) + MAJ(%s,%s,%s);\n"
            "  %s = %s + t1; %s = t1 + t2;\n",
            v2[7], v2[4], v2[4], v2[5], v2[6], SHA_K[t], wt,
            v2[0], v2[0], v2[1], v2[2],
            v2[3], v2[3], v2[7]);
        const char *na = v2[7], *ne = v2[3];
        v2[7] = v2[6]; v2[6] = v2[5]; v2[5] = v2[4]; v2[4] = ne;
        v2[3] = v2[2]; v2[2] = v2[1]; v2[1] = v2[0]; v2[0] = na;
    }
    sb_appendf(sb,
        "  uint final_h7 = 0x5be0cd19u + %s;\n"
        "  if (final_h7 == 0u) {\n"
        "    uint idx = atomicAdd(O.count, 1u);\n"
        "    if (idx < 15u) O.nonces[idx] = nonce;\n"
        "  }\n"
        "}\n",
        v2[7]);
}

static void append_k_array(sb_t *sb)
{
    sb_appendf(sb, "const uint K[64] = uint[64](\n");
    for (int i = 0; i < 64; i++) {
        sb_appendf(sb, "  0x%08xu%s\n", SHA_K[i], i == 63 ? "" : ",");
    }
    sb_appendf(sb, ");\n");
}

static void append_round_function(sb_t *sb, int ring_schedule)
{
    append_k_array(sb);
    sb_appendf(sb,
        "uint final_word7(uint nonce) {\n");
    if (ring_schedule) {
        sb_appendf(sb,
            "  uint W[16];\n"
            "  W[0]=I.w0_in; W[1]=I.w1_in; W[2]=I.w2_in; W[3]=nonce;\n"
            "  W[4]=0x80000000u; W[5]=0u; W[6]=0u; W[7]=0u;\n"
            "  W[8]=0u; W[9]=0u; W[10]=0u; W[11]=0u; W[12]=0u; W[13]=0u; W[14]=0u; W[15]=640u;\n");
    } else {
        sb_appendf(sb,
            "  uint W[64];\n"
            "  W[0]=I.w0_in; W[1]=I.w1_in; W[2]=I.w2_in; W[3]=nonce;\n"
            "  W[4]=0x80000000u; W[5]=0u; W[6]=0u; W[7]=0u;\n"
            "  W[8]=0u; W[9]=0u; W[10]=0u; W[11]=0u; W[12]=0u; W[13]=0u; W[14]=0u; W[15]=640u;\n"
            "  for (int i = 16; i < 64; i++) W[i] = W[i-16] + SMALLSIG0(W[i-15]) + W[i-7] + SMALLSIG1(W[i-2]);\n");
    }
    sb_appendf(sb,
        "  uint a=I.midstate[0], b=I.midstate[1], c=I.midstate[2], d=I.midstate[3];\n"
        "  uint e=I.midstate[4], f=I.midstate[5], g=I.midstate[6], h=I.midstate[7];\n"
        "  for (int i = 0; i < 64; i++) {\n");
    if (ring_schedule) {
        sb_appendf(sb,
            "    if (i >= 16) {\n"
            "      int s = i & 15;\n"
            "      W[s] = W[s] + SMALLSIG0(W[(i - 15) & 15]) + W[(i - 7) & 15] + SMALLSIG1(W[(i - 2) & 15]);\n"
            "    }\n"
            "    uint wt = W[i & 15];\n");
    } else {
        sb_appendf(sb, "    uint wt = W[i];\n");
    }
    sb_appendf(sb,
        "    uint t1 = h + BIGSIG1(e) + CH(e,f,g) + K[i] + wt;\n"
        "    uint t2 = BIGSIG0(a) + MAJ(a,b,c);\n"
        "    h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;\n"
        "  }\n"
        "  uint h1_0=I.midstate[0]+a, h1_1=I.midstate[1]+b, h1_2=I.midstate[2]+c, h1_3=I.midstate[3]+d;\n"
        "  uint h1_4=I.midstate[4]+e, h1_5=I.midstate[5]+f, h1_6=I.midstate[6]+g, h1_7=I.midstate[7]+h;\n");

    if (ring_schedule) {
        sb_appendf(sb,
            "  W[0]=h1_0; W[1]=h1_1; W[2]=h1_2; W[3]=h1_3; W[4]=h1_4; W[5]=h1_5; W[6]=h1_6; W[7]=h1_7;\n"
            "  W[8]=0x80000000u; W[9]=0u; W[10]=0u; W[11]=0u; W[12]=0u; W[13]=0u; W[14]=0u; W[15]=256u;\n");
    } else {
        sb_appendf(sb,
            "  W[0]=h1_0; W[1]=h1_1; W[2]=h1_2; W[3]=h1_3; W[4]=h1_4; W[5]=h1_5; W[6]=h1_6; W[7]=h1_7;\n"
            "  W[8]=0x80000000u; W[9]=0u; W[10]=0u; W[11]=0u; W[12]=0u; W[13]=0u; W[14]=0u; W[15]=256u;\n"
            "  for (int i = 16; i < 64; i++) W[i] = W[i-16] + SMALLSIG0(W[i-15]) + W[i-7] + SMALLSIG1(W[i-2]);\n");
    }
    sb_appendf(sb,
        "  a=0x6a09e667u; b=0xbb67ae85u; c=0x3c6ef372u; d=0xa54ff53au;\n"
        "  e=0x510e527fu; f=0x9b05688cu; g=0x1f83d9abu; h=0x5be0cd19u;\n"
        "  for (int i = 0; i < 64; i++) {\n");
    if (ring_schedule) {
        sb_appendf(sb,
            "    if (i >= 16) {\n"
            "      int s = i & 15;\n"
            "      W[s] = W[s] + SMALLSIG0(W[(i - 15) & 15]) + W[(i - 7) & 15] + SMALLSIG1(W[(i - 2) & 15]);\n"
            "    }\n"
            "    uint wt = W[i & 15];\n");
    } else {
        sb_appendf(sb, "    uint wt = W[i];\n");
    }
    sb_appendf(sb,
        "    uint t1 = h + BIGSIG1(e) + CH(e,f,g) + K[i] + wt;\n"
        "    uint t2 = BIGSIG0(a) + MAJ(a,b,c);\n"
        "    h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;\n"
        "  }\n"
        "  return 0x5be0cd19u + h;\n"
        "}\n"
        "bool nonce_matches(uint nonce) { return final_word7(nonce) == 0u; }\n");
}

static void append_looped_main(sb_t *sb, const gbtc_gles_config_t *cfg)
{
    int ring_schedule = cfg->kernel == GBTC_GLES_KERNEL_PARTIAL;
    append_prefix(sb, cfg);
    append_round_function(sb, ring_schedule);
    sb_appendf(sb,
        "void main() {\n"
        "  uint global_idx = gl_GlobalInvocationID.x + gl_GlobalInvocationID.y * (gl_NumWorkGroups.x * gl_WorkGroupSize.x);\n");
    if (cfg->kernel == GBTC_GLES_KERNEL_DUALNONCE) {
        sb_appendf(sb,
            "  uint nonce0 = I.nonce_base + global_idx * 2u;\n"
            "  uint nonce1 = nonce0 + 1u;\n"
            "  if (nonce_matches(nonce0)) {\n"
            "    uint idx = atomicAdd(O.count, 1u);\n"
            "    if (idx < 15u) O.nonces[idx] = nonce0;\n"
            "  }\n"
            "  if (nonce_matches(nonce1)) {\n"
            "    uint idx = atomicAdd(O.count, 1u);\n"
            "    if (idx < 15u) O.nonces[idx] = nonce1;\n"
            "  }\n");
    } else {
        sb_appendf(sb,
            "  uint nonce = I.nonce_base + global_idx;\n"
            "  if (nonce_matches(nonce)) {\n"
            "    uint idx = atomicAdd(O.count, 1u);\n"
            "    if (idx < 15u) O.nonces[idx] = nonce;\n"
            "  }\n");
    }
    sb_appendf(sb, "}\n");
}

char *gbtc_gles_build_kernel_src(const gbtc_gles_config_t *cfg,
                                 char *reason, size_t reason_cap)
{
    if (!cfg || !local_size_allowed(cfg->local_size)) {
        set_reason(reason, reason_cap, "invalid GLES kernel configuration");
        return NULL;
    }

    char *buf = calloc(1, GBTC_GLES_SOURCE_CAP);
    if (!buf) {
        set_reason(reason, reason_cap, "out of memory allocating shader source");
        return NULL;
    }
    sb_t sb = {
        .buf = buf,
        .cap = GBTC_GLES_SOURCE_CAP,
        .len = 0,
        .failed = 0,
    };

    if (cfg->kernel == GBTC_GLES_KERNEL_UNROLLED ||
        cfg->kernel == GBTC_GLES_KERNEL_ALTBOOL) {
        append_unrolled(&sb, cfg);
    } else {
        append_looped_main(&sb, cfg);
    }

    if (sb.failed) {
        free(buf);
        set_reason(reason, reason_cap, "generated GLES shader exceeded %u bytes",
                   GBTC_GLES_SOURCE_CAP);
        return NULL;
    }
    return buf;
}
