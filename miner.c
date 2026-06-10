// gamble-btc — DIY iGPU SHA-256d solo miner for HD 4600
// Path: EGL surfaceless + GLES 3.2 compute shader (Mesa crocus driver)
// Pool: stratum+tcp (public-pool.io)
//
// Single-file C, ~700 lines. Educational. NOT optimized for max hashrate.

#define _GNU_SOURCE
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl32.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <signal.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <time.h>
#include <ctype.h>
#include <math.h>
#include <cjson/cJSON.h>

#define LOG(fmt, ...) do { fprintf(stderr, "[%ld] " fmt "\n", time(NULL), ##__VA_ARGS__); } while(0)
#define DIE(fmt, ...) do { LOG("FATAL " fmt, ##__VA_ARGS__); exit(1); } while(0)

// ============================================================================
// SHA-256 reference (CPU side, used for midstate + coinbase + merkle)
// ============================================================================
static const uint32_t K256[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};
static const uint32_t H256_INIT[8] = {
    0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
    0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19
};

static inline uint32_t rotr32(uint32_t x, int n){return (x>>n)|(x<<(32-n));}
static void sha256_compress(uint32_t H[8], const uint8_t block[64]) {
    uint32_t W[64];
    for (int i = 0; i < 16; i++) {
        W[i] = ((uint32_t)block[4*i]<<24) | ((uint32_t)block[4*i+1]<<16) |
               ((uint32_t)block[4*i+2]<<8) | (uint32_t)block[4*i+3];
    }
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = rotr32(W[i-15],7)^rotr32(W[i-15],18)^(W[i-15]>>3);
        uint32_t s1 = rotr32(W[i-2],17)^rotr32(W[i-2],19)^(W[i-2]>>10);
        W[i] = W[i-16] + s0 + W[i-7] + s1;
    }
    uint32_t a=H[0],b=H[1],c=H[2],d=H[3],e=H[4],f=H[5],g=H[6],h=H[7];
    for (int i = 0; i < 64; i++) {
        uint32_t S1 = rotr32(e,6)^rotr32(e,11)^rotr32(e,25);
        uint32_t ch = (e&f)^(~e&g);
        uint32_t t1 = h + S1 + ch + K256[i] + W[i];
        uint32_t S0 = rotr32(a,2)^rotr32(a,13)^rotr32(a,22);
        uint32_t mj = (a&b)^(a&c)^(b&c);
        uint32_t t2 = S0 + mj;
        h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;
    }
    H[0]+=a; H[1]+=b; H[2]+=c; H[3]+=d; H[4]+=e; H[5]+=f; H[6]+=g; H[7]+=h;
}
static void sha256_full(uint32_t H[8], const uint8_t *data, size_t len) {
    memcpy(H, H256_INIT, 32);
    uint8_t buf[64];
    size_t i = 0;
    while (i + 64 <= len) { sha256_compress(H, data + i); i += 64; }
    size_t rem = len - i;
    memcpy(buf, data + i, rem);
    buf[rem] = 0x80;
    if (rem >= 56) {
        memset(buf + rem + 1, 0, 64 - rem - 1);
        sha256_compress(H, buf);
        memset(buf, 0, 56);
    } else {
        memset(buf + rem + 1, 0, 56 - rem - 1);
    }
    uint64_t bits = (uint64_t)len * 8;
    for (int j = 0; j < 8; j++) buf[56+j] = (bits >> (56 - 8*j)) & 0xff;
    sha256_compress(H, buf);
}
static void sha256d(uint8_t out[32], const uint8_t *data, size_t len) {
    uint32_t H[8];
    sha256_full(H, data, len);
    uint8_t mid[32];
    for (int i = 0; i < 8; i++) {
        mid[4*i+0] = H[i]>>24; mid[4*i+1] = H[i]>>16;
        mid[4*i+2] = H[i]>>8;  mid[4*i+3] = H[i];
    }
    sha256_full(H, mid, 32);
    for (int i = 0; i < 8; i++) {
        out[4*i+0] = H[i]>>24; out[4*i+1] = H[i]>>16;
        out[4*i+2] = H[i]>>8;  out[4*i+3] = H[i];
    }
}

// ============================================================================
// Hex helpers
// ============================================================================
static int hexval(char c) {
    if (c>='0'&&c<='9') return c-'0';
    if (c>='a'&&c<='f') return c-'a'+10;
    if (c>='A'&&c<='F') return c-'A'+10;
    return -1;
}
static int hex_to_bin(uint8_t *out, const char *hex, size_t bytes) {
    for (size_t i = 0; i < bytes; i++) {
        int hi = hexval(hex[2*i]), lo = hexval(hex[2*i+1]);
        if (hi < 0 || lo < 0) return -1;
        out[i] = (uint8_t)((hi<<4)|lo);
    }
    return 0;
}
static void bin_to_hex(char *out, const uint8_t *in, size_t bytes) {
    static const char *h = "0123456789abcdef";
    for (size_t i = 0; i < bytes; i++) {
        out[2*i] = h[in[i]>>4]; out[2*i+1] = h[in[i]&15];
    }
    out[2*bytes] = 0;
}

// ============================================================================
// GLSL compute shader for sha256d nonce search
// ============================================================================
// Original sliding-W shader kept for reference. Replaced by build_kernel_src() below
// which generates a fully-unrolled version with all named uint w0..w15 + literal K constants.
static const char *kernel_src_unused =
    "#version 320 es\n"
    "layout(local_size_x = 64) in;\n"
    "layout(std430, binding = 0) readonly buffer In {\n"
    "  uint midstate[8];\n"
    "  uint w0; uint w1; uint w2;\n"
    "  uint nonce_base;\n"
    "  uint pad[3];\n"
    "} I;\n"
    "layout(std430, binding = 1) buffer Out {\n"
    "  uint count;\n"
    "  uint nonces[15];\n"
    "} O;\n"
    "const uint K[64] = uint[64](\n"
    " 0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,\n"
    " 0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,\n"
    " 0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,\n"
    " 0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,\n"
    " 0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,\n"
    " 0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,\n"
    " 0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,\n"
    " 0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u);\n"
    "#define ROR(x,n) (((x)>>(n))|((x)<<(32u-(n))))\n"
    "#define BIGSIG0(x) (ROR(x,2u)^ROR(x,13u)^ROR(x,22u))\n"
    "#define BIGSIG1(x) (ROR(x,6u)^ROR(x,11u)^ROR(x,25u))\n"
    "#define SMALLSIG0(x) (ROR(x,7u)^ROR(x,18u)^((x)>>3u))\n"
    "#define SMALLSIG1(x) (ROR(x,17u)^ROR(x,19u)^((x)>>10u))\n"
    "#define CH(x,y,z) (((x)&(y))^(~(x)&(z)))\n"
    "#define MAJ(x,y,z) (((x)&(y))^((x)&(z))^((y)&(z)))\n"
    "void main() {\n"
    "  uint global_idx = gl_GlobalInvocationID.x + gl_GlobalInvocationID.y * (gl_NumWorkGroups.x * gl_WorkGroupSize.x);\n"
    "  uint nonce = I.nonce_base + global_idx;\n"
    "  uint W[16];\n"
    "  // ---- First SHA: continue from midstate, second 64-byte block ----\n"
    "  W[0]=I.w0; W[1]=I.w1; W[2]=I.w2; W[3]=nonce;\n"
    "  W[4]=0x80000000u;\n"
    "  W[5]=0u; W[6]=0u; W[7]=0u; W[8]=0u; W[9]=0u; W[10]=0u; W[11]=0u; W[12]=0u; W[13]=0u; W[14]=0u;\n"
    "  W[15]=640u;\n"
    "  uint a=I.midstate[0], b=I.midstate[1], c=I.midstate[2], d=I.midstate[3];\n"
    "  uint e=I.midstate[4], f=I.midstate[5], g=I.midstate[6], h=I.midstate[7];\n"
    "  uint t1, t2;\n"
    "  for (int t = 0; t < 16; t++) {\n"
    "    t1 = h + BIGSIG1(e) + CH(e,f,g) + K[t] + W[t];\n"
    "    t2 = BIGSIG0(a) + MAJ(a,b,c);\n"
    "    h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;\n"
    "  }\n"
    "  for (int t = 16; t < 64; t++) {\n"
    "    int i15=(t-15)&15, i2=(t-2)&15, i7=(t-7)&15, i16=(t-16)&15, i0=t&15;\n"
    "    uint w15=W[i15], w2=W[i2];\n"
    "    uint wt = W[i16] + SMALLSIG0(w15) + W[i7] + SMALLSIG1(w2);\n"
    "    W[i0] = wt;\n"
    "    t1 = h + BIGSIG1(e) + CH(e,f,g) + K[t] + wt;\n"
    "    t2 = BIGSIG0(a) + MAJ(a,b,c);\n"
    "    h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;\n"
    "  }\n"
    "  uint h0=I.midstate[0]+a, h1=I.midstate[1]+b, h2=I.midstate[2]+c, h3=I.midstate[3]+d;\n"
    "  uint h4=I.midstate[4]+e, h5=I.midstate[5]+f, h6=I.midstate[6]+g, h7=I.midstate[7]+h;\n"
    "  // ---- Second SHA: hash 32-byte first hash, padded to 64 ----\n"
    "  W[0]=h0; W[1]=h1; W[2]=h2; W[3]=h3; W[4]=h4; W[5]=h5; W[6]=h6; W[7]=h7;\n"
    "  W[8]=0x80000000u; W[9]=0u; W[10]=0u; W[11]=0u; W[12]=0u; W[13]=0u; W[14]=0u;\n"
    "  W[15]=256u;\n"
    "  a=0x6a09e667u; b=0xbb67ae85u; c=0x3c6ef372u; d=0xa54ff53au;\n"
    "  e=0x510e527fu; f=0x9b05688cu; g=0x1f83d9abu; h=0x5be0cd19u;\n"
    "  for (int t = 0; t < 16; t++) {\n"
    "    t1 = h + BIGSIG1(e) + CH(e,f,g) + K[t] + W[t];\n"
    "    t2 = BIGSIG0(a) + MAJ(a,b,c);\n"
    "    h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;\n"
    "  }\n"
    "  for (int t = 16; t < 64; t++) {\n"
    "    int i15=(t-15)&15, i2=(t-2)&15, i7=(t-7)&15, i16=(t-16)&15, i0=t&15;\n"
    "    uint w15=W[i15], w2=W[i2];\n"
    "    uint wt = W[i16] + SMALLSIG0(w15) + W[i7] + SMALLSIG1(w2);\n"
    "    W[i0] = wt;\n"
    "    t1 = h + BIGSIG1(e) + CH(e,f,g) + K[t] + wt;\n"
    "    t2 = BIGSIG0(a) + MAJ(a,b,c);\n"
    "    h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;\n"
    "  }\n"
    "  uint final_h7 = 0x5be0cd19u + h;\n"
    "  if (final_h7 == 0u) {\n"
    "    uint idx = atomicAdd(O.count, 1u);\n"
    "    if (idx < 15u) O.nonces[idx] = nonce;\n"
    "  }\n"
    "}\n";

// ---- Fully-unrolled shader generator ----
// Emits 128 round expressions (64 first-SHA + 64 second-SHA) inline with named
// uint w0..w15 vars and literal K constants. No dynamic indexing → Mesa can register-allocate.
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

static void emit_round(char **p, const char *vars[8], int t, const char *wname) {
    *p += sprintf(*p,
        "  t1 = %s + BIGSIG1(%s) + CH(%s,%s,%s) + 0x%08xu + %s;\n"
        "  t2 = BIGSIG0(%s) + MAJ(%s,%s,%s);\n"
        "  %s = %s + t1; %s = t1 + t2;\n",
        vars[7], vars[4], vars[4], vars[5], vars[6], SHA_K[t], wname,
        vars[0], vars[0], vars[1], vars[2],
        vars[3], vars[3], vars[7]);
    // Rotate vars: a=t1+t2, b=a, c=b, ..., h=g
    // After the round: new a=t1+t2 (stored in vars[7] var), new e=d+t1 (stored in vars[3])
    // Variable rotation: shift via name swap by caller
}

static char* build_kernel_src(void) {
    static char buf[262144];
    char *p = buf;
    p += sprintf(p,
        "#version 320 es\n"
        "layout(local_size_x = 64) in;\n"
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
        "#define SMALLSIG1(x) (ROR(x,17u)^ROR(x,19u)^((x)>>10u))\n"
        "#define CH(x,y,z) (((x)&(y))^(~(x)&(z)))\n"
        "#define MAJ(x,y,z) (((x)&(y))^((x)&(z))^((y)&(z)))\n"
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

    const char *wnames[16] = {"w0","w1","w2","w3","w4","w5","w6","w7","w8","w9","w10","w11","w12","w13","w14","w15"};

    {
        const char *v[8] = {"a","b","c","d","e","f","g","h"};
        for (int t = 0; t < 64; t++) {
            const char *wt;
            if (t < 16) wt = wnames[t];
            else {
                int s = t & 15;
                int s2 = (t-2) & 15;
                int s7 = (t-7) & 15;
                int s15 = (t-15) & 15;
                p += sprintf(p, "  %s = %s + SMALLSIG0(%s) + %s + SMALLSIG1(%s);\n",
                             wnames[s], wnames[s], wnames[s15], wnames[s7], wnames[s2]);
                wt = wnames[s];
            }
            p += sprintf(p,
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
        p += sprintf(p,
            "  uint h1_0 = I.midstate[0] + %s;\n"
            "  uint h1_1 = I.midstate[1] + %s;\n"
            "  uint h1_2 = I.midstate[2] + %s;\n"
            "  uint h1_3 = I.midstate[3] + %s;\n"
            "  uint h1_4 = I.midstate[4] + %s;\n"
            "  uint h1_5 = I.midstate[5] + %s;\n"
            "  uint h1_6 = I.midstate[6] + %s;\n"
            "  uint h1_7 = I.midstate[7] + %s;\n",
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]);
    }

    p += sprintf(p,
        "  w0 = h1_0; w1 = h1_1; w2 = h1_2; w3 = h1_3;\n"
        "  w4 = h1_4; w5 = h1_5; w6 = h1_6; w7 = h1_7;\n"
        "  w8 = 0x80000000u; w9 = 0u; w10 = 0u; w11 = 0u;\n"
        "  w12 = 0u; w13 = 0u; w14 = 0u; w15 = 256u;\n"
        "  a = 0x6a09e667u; b = 0xbb67ae85u; c = 0x3c6ef372u; d = 0xa54ff53au;\n"
        "  e = 0x510e527fu; f = 0x9b05688cu; g = 0x1f83d9abu; h = 0x5be0cd19u;\n");

    {
        const char *v[8] = {"a","b","c","d","e","f","g","h"};
        for (int t = 0; t < 64; t++) {
            const char *wt;
            if (t < 16) wt = wnames[t];
            else {
                int s = t & 15;
                int s2 = (t-2) & 15;
                int s7 = (t-7) & 15;
                int s15 = (t-15) & 15;
                p += sprintf(p, "  %s = %s + SMALLSIG0(%s) + %s + SMALLSIG1(%s);\n",
                             wnames[s], wnames[s], wnames[s15], wnames[s7], wnames[s2]);
                wt = wnames[s];
            }
            p += sprintf(p,
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
        p += sprintf(p,
            "  uint final_h7 = 0x5be0cd19u + %s;\n"
            "  if (final_h7 == 0u) {\n"
            "    uint idx = atomicAdd(O.count, 1u);\n"
            "    if (idx < 15u) O.nonces[idx] = nonce;\n"
            "  }\n"
            "}\n",
            v[7]);
    }

    return buf;
}

// ============================================================================
// Forward decls for live-stats globals (defined further below)
// ============================================================================
extern pthread_mutex_t m_mu;
extern uint64_t g_submitted, g_accepted, g_rejected;
extern double   g_current_diff;
extern char     g_current_job[64];
extern uint32_t g_current_ntime, g_current_nbits;
extern int      g_merkle_branches;

// ============================================================================
// Stratum client
// ============================================================================
typedef struct {
    int fd;
    char rxbuf[65536];
    size_t rxlen;
    char extranonce1[33];
    int extranonce1_len;
    int extranonce2_size;
    int next_id;
    char username[128];
    char password[64];
    double diff;
    // Current job
    bool have_job;
    char job_id[64];
    uint8_t prevhash[32];
    char coinb1_hex[1024];
    char coinb2_hex[1024];
    int merkle_count;
    uint8_t merkle[32][32];
    uint32_t version_be;
    uint32_t nbits_be;
    uint32_t ntime_be;
    bool clean;
} stratum_t;

static int set_nodelay(int fd) {
    int v = 1;
    return setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &v, sizeof(v));
}

static int stratum_connect(stratum_t *s, const char *url) {
    // url like "stratum+tcp://host:port"
    const char *p = strstr(url, "://");
    if (!p) { LOG("bad url"); return -1; }
    p += 3;
    char host[256]; int port = 0;
    const char *colon = strrchr(p, ':');
    if (!colon) { LOG("no port"); return -1; }
    size_t hlen = colon - p;
    if (hlen >= sizeof(host)) return -1;
    memcpy(host, p, hlen); host[hlen] = 0;
    port = atoi(colon + 1);
    LOG("stratum: connecting to %s:%d", host, port);

    struct addrinfo hints = {0}, *ai;
    hints.ai_family = AF_UNSPEC; hints.ai_socktype = SOCK_STREAM;
    char ports[8]; snprintf(ports, sizeof(ports), "%d", port);
    if (getaddrinfo(host, ports, &hints, &ai) != 0) { LOG("getaddrinfo fail"); return -1; }
    int fd = -1;
    for (struct addrinfo *r = ai; r; r = r->ai_next) {
        fd = socket(r->ai_family, r->ai_socktype, r->ai_protocol);
        if (fd < 0) continue;
        if (connect(fd, r->ai_addr, r->ai_addrlen) == 0) break;
        close(fd); fd = -1;
    }
    freeaddrinfo(ai);
    if (fd < 0) { LOG("connect fail"); return -1; }
    set_nodelay(fd);
    s->fd = fd;
    s->rxlen = 0;
    s->next_id = 1;
    s->have_job = false;
    s->diff = 1.0;
    LOG("stratum: connected");
    return 0;
}

static int stratum_send(stratum_t *s, const char *json) {
    size_t len = strlen(json);
    char buf[8192];
    if (len + 2 > sizeof(buf)) return -1;
    memcpy(buf, json, len);
    buf[len] = '\n'; buf[len+1] = 0;
    ssize_t w = send(s->fd, buf, len+1, 0);
    if (w != (ssize_t)(len+1)) { LOG("send fail %zd", w); return -1; }
    return 0;
}

static char *stratum_readline(stratum_t *s, int timeout_ms) {
    // Fill rxbuf, return ptr to one line (NUL-term) or NULL
    while (1) {
        char *nl = memchr(s->rxbuf, '\n', s->rxlen);
        if (nl) {
            *nl = 0;
            static char line[8192];
            size_t llen = nl - s->rxbuf;
            if (llen >= sizeof(line)) llen = sizeof(line) - 1;
            memcpy(line, s->rxbuf, llen); line[llen] = 0;
            size_t consumed = (nl - s->rxbuf) + 1;
            memmove(s->rxbuf, s->rxbuf + consumed, s->rxlen - consumed);
            s->rxlen -= consumed;
            return line;
        }
        struct timeval tv = { timeout_ms/1000, (timeout_ms%1000)*1000 };
        fd_set rfds; FD_ZERO(&rfds); FD_SET(s->fd, &rfds);
        int r = select(s->fd + 1, &rfds, NULL, NULL, &tv);
        if (r <= 0) return NULL;
        ssize_t n = recv(s->fd, s->rxbuf + s->rxlen, sizeof(s->rxbuf) - s->rxlen - 1, 0);
        if (n <= 0) { LOG("stratum closed (n=%zd)", n); return NULL; }
        s->rxlen += n;
    }
}

static void stratum_handle_notify(stratum_t *s, cJSON *params) {
    if (!cJSON_IsArray(params) || cJSON_GetArraySize(params) < 9) return;
    cJSON *jid = cJSON_GetArrayItem(params, 0);
    cJSON *prev = cJSON_GetArrayItem(params, 1);
    cJSON *cb1 = cJSON_GetArrayItem(params, 2);
    cJSON *cb2 = cJSON_GetArrayItem(params, 3);
    cJSON *mb = cJSON_GetArrayItem(params, 4);
    cJSON *ver = cJSON_GetArrayItem(params, 5);
    cJSON *nbits = cJSON_GetArrayItem(params, 6);
    cJSON *ntime = cJSON_GetArrayItem(params, 7);
    cJSON *clean = cJSON_GetArrayItem(params, 8);
    snprintf(s->job_id, sizeof(s->job_id), "%s", jid->valuestring);
    hex_to_bin(s->prevhash, prev->valuestring, 32);
    snprintf(s->coinb1_hex, sizeof(s->coinb1_hex), "%s", cb1->valuestring);
    snprintf(s->coinb2_hex, sizeof(s->coinb2_hex), "%s", cb2->valuestring);
    s->merkle_count = cJSON_GetArraySize(mb);
    if (s->merkle_count > 32) s->merkle_count = 32;
    for (int i = 0; i < s->merkle_count; i++) {
        cJSON *h = cJSON_GetArrayItem(mb, i);
        hex_to_bin(s->merkle[i], h->valuestring, 32);
    }
    uint8_t buf[4];
    hex_to_bin(buf, ver->valuestring, 4);
    s->version_be = ((uint32_t)buf[0]<<24)|((uint32_t)buf[1]<<16)|((uint32_t)buf[2]<<8)|buf[3];
    hex_to_bin(buf, nbits->valuestring, 4);
    s->nbits_be = ((uint32_t)buf[0]<<24)|((uint32_t)buf[1]<<16)|((uint32_t)buf[2]<<8)|buf[3];
    hex_to_bin(buf, ntime->valuestring, 4);
    s->ntime_be = ((uint32_t)buf[0]<<24)|((uint32_t)buf[1]<<16)|((uint32_t)buf[2]<<8)|buf[3];
    s->clean = cJSON_IsTrue(clean);
    s->have_job = true;
    LOG("notify: job=%s ntime=%08x nbits=%08x clean=%d merkle_branches=%d",
        s->job_id, s->ntime_be, s->nbits_be, s->clean, s->merkle_count);
    pthread_mutex_lock(&m_mu);
    snprintf(g_current_job, sizeof(g_current_job), "%s", s->job_id);
    g_current_ntime = s->ntime_be;
    g_current_nbits = s->nbits_be;
    g_merkle_branches = s->merkle_count;
    pthread_mutex_unlock(&m_mu);
}

static int stratum_subscribe_authorize(stratum_t *s) {
    char msg[512];
    snprintf(msg, sizeof(msg),
        "{\"id\":%d,\"method\":\"mining.subscribe\",\"params\":[\"gamble-btc/0.1\"]}",
        s->next_id++);
    if (stratum_send(s, msg) < 0) return -1;
    char *line;
    while ((line = stratum_readline(s, 10000))) {
        cJSON *root = cJSON_Parse(line);
        if (!root) continue;
        cJSON *result = cJSON_GetObjectItem(root, "result");
        cJSON *id = cJSON_GetObjectItem(root, "id");
        if (cJSON_IsArray(result) && cJSON_IsNumber(id) && id->valueint == 1) {
            // result = [[subscriptions...], extranonce1_hex, extranonce2_size]
            cJSON *en1 = cJSON_GetArrayItem(result, 1);
            cJSON *en2sz = cJSON_GetArrayItem(result, 2);
            snprintf(s->extranonce1, sizeof(s->extranonce1), "%s", en1->valuestring);
            s->extranonce1_len = strlen(en1->valuestring) / 2;
            s->extranonce2_size = en2sz->valueint;
            LOG("subscribe: extranonce1=%s en2_size=%d", s->extranonce1, s->extranonce2_size);
            cJSON_Delete(root);
            break;
        }
        cJSON_Delete(root);
    }
    snprintf(msg, sizeof(msg),
        "{\"id\":%d,\"method\":\"mining.authorize\",\"params\":[\"%s\",\"%s\"]}",
        s->next_id++, s->username, s->password);
    if (stratum_send(s, msg) < 0) return -1;
    LOG("authorized as %s", s->username);
    return 0;
}

static void stratum_pump(stratum_t *s, int timeout_ms) {
    char *line = stratum_readline(s, timeout_ms);
    if (!line) return;
    cJSON *root = cJSON_Parse(line);
    if (!root) return;
    cJSON *method = cJSON_GetObjectItem(root, "method");
    cJSON *params = cJSON_GetObjectItem(root, "params");
    if (cJSON_IsString(method)) {
        if (strcmp(method->valuestring, "mining.notify") == 0) {
            stratum_handle_notify(s, params);
        } else if (strcmp(method->valuestring, "mining.set_difficulty") == 0) {
            cJSON *d = cJSON_GetArrayItem(params, 0);
            if (cJSON_IsNumber(d)) {
                s->diff = d->valuedouble;
                LOG("set_difficulty %f", s->diff);
                pthread_mutex_lock(&m_mu);
                g_current_diff = s->diff;
                pthread_mutex_unlock(&m_mu);
            }
        }
    } else {
        cJSON *result = cJSON_GetObjectItem(root, "result");
        cJSON *err = cJSON_GetObjectItem(root, "error");
        if (cJSON_IsBool(result)) {
            LOG("submit response: result=%d err=%s",
                cJSON_IsTrue(result),
                cJSON_IsNull(err) ? "null" : (cJSON_IsArray(err) ? cJSON_PrintUnformatted(err) : "?"));
            pthread_mutex_lock(&m_mu);
            if (cJSON_IsTrue(result)) g_accepted++; else g_rejected++;
            pthread_mutex_unlock(&m_mu);
        }
    }
    cJSON_Delete(root);
}

static int stratum_submit(stratum_t *s, const char *job_id, const char *en2_hex,
                           uint32_t ntime_be, uint32_t nonce_be) {
    char msg[1024];
    snprintf(msg, sizeof(msg),
        "{\"id\":%d,\"method\":\"mining.submit\",\"params\":[\"%s\",\"%s\",\"%s\",\"%08x\",\"%08x\"]}",
        s->next_id++, s->username, job_id, en2_hex, ntime_be, nonce_be);
    LOG("SUBMIT: %s", msg);
    pthread_mutex_lock(&m_mu);
    g_submitted++;
    pthread_mutex_unlock(&m_mu);
    return stratum_send(s, msg);
}

// ============================================================================
// Work building (coinbase + merkle root + header midstate)
// ============================================================================
static void compute_merkle_root(uint8_t mr[32], const stratum_t *s, const uint8_t *en2, int en2_len) {
    uint8_t cb1[1024], cb2[1024];
    int cb1_len = strlen(s->coinb1_hex) / 2;
    int cb2_len = strlen(s->coinb2_hex) / 2;
    hex_to_bin(cb1, s->coinb1_hex, cb1_len);
    hex_to_bin(cb2, s->coinb2_hex, cb2_len);
    uint8_t en1[32];
    hex_to_bin(en1, s->extranonce1, s->extranonce1_len);
    uint8_t coinbase[2048];
    int p = 0;
    memcpy(coinbase + p, cb1, cb1_len); p += cb1_len;
    memcpy(coinbase + p, en1, s->extranonce1_len); p += s->extranonce1_len;
    memcpy(coinbase + p, en2, en2_len); p += en2_len;
    memcpy(coinbase + p, cb2, cb2_len); p += cb2_len;
    uint8_t cb_hash[32];
    sha256d(cb_hash, coinbase, p);
    memcpy(mr, cb_hash, 32);
    for (int i = 0; i < s->merkle_count; i++) {
        uint8_t cat[64];
        memcpy(cat, mr, 32);
        memcpy(cat + 32, s->merkle[i], 32);
        sha256d(mr, cat, 64);
    }
}

// Build header (80 bytes), pre-nonce. Returns midstate of first 64 bytes.
// Layout: version(4 LE) || prevhash(32, word-swapped) || merkle_root(32) || ntime(4 LE) || nbits(4 LE) || nonce(4 LE)
static void build_header_midstate(uint32_t midstate[8], uint32_t tail3[3],
                                  const stratum_t *s, const uint8_t merkle_root[32]) {
    uint8_t hdr[80];
    // version: stratum sends BE hex; serialize header LE
    hdr[0] = s->version_be & 0xff;
    hdr[1] = (s->version_be >> 8) & 0xff;
    hdr[2] = (s->version_be >> 16) & 0xff;
    hdr[3] = (s->version_be >> 24) & 0xff;
    // prevhash: stratum hex is 32 bytes "internal" order. Per stratum spec, swap each 4-byte word.
    for (int i = 0; i < 8; i++) {
        hdr[4 + 4*i + 0] = s->prevhash[4*i + 3];
        hdr[4 + 4*i + 1] = s->prevhash[4*i + 2];
        hdr[4 + 4*i + 2] = s->prevhash[4*i + 1];
        hdr[4 + 4*i + 3] = s->prevhash[4*i + 0];
    }
    // merkle_root: comes from our sha256d output (already big-endian). Per stratum, place as-is into header at offset 36.
    // Header expects little-endian 32-byte hash. sha256d returns big-endian word output; treat bytes as-is for header.
    // Common stratum miners place merkle bytes directly. Match that.
    memcpy(hdr + 36, merkle_root, 32);
    // ntime LE
    hdr[68] = s->ntime_be & 0xff;
    hdr[69] = (s->ntime_be >> 8) & 0xff;
    hdr[70] = (s->ntime_be >> 16) & 0xff;
    hdr[71] = (s->ntime_be >> 24) & 0xff;
    // nbits LE
    hdr[72] = s->nbits_be & 0xff;
    hdr[73] = (s->nbits_be >> 8) & 0xff;
    hdr[74] = (s->nbits_be >> 16) & 0xff;
    hdr[75] = (s->nbits_be >> 24) & 0xff;
    // nonce: filled per-thread in shader; placeholder
    hdr[76]=0; hdr[77]=0; hdr[78]=0; hdr[79]=0;

    // midstate from first 64 bytes
    memcpy(midstate, H256_INIT, 32);
    sha256_compress(midstate, hdr);
    // tail words: bytes 64..75 → 3 uint32 in BE word form for shader
    for (int i = 0; i < 3; i++) {
        tail3[i] = ((uint32_t)hdr[64+4*i]<<24)|((uint32_t)hdr[64+4*i+1]<<16)|
                   ((uint32_t)hdr[64+4*i+2]<<8)|hdr[64+4*i+3];
    }
}

// ============================================================================
// EGL / GL setup
// ============================================================================
static EGLDisplay egl_dpy;
static EGLContext egl_ctx;
static GLuint compute_prog;
static GLuint ssbo_in, ssbo_out;

typedef struct {
    uint32_t midstate[8];
    uint32_t w0, w1, w2;
    uint32_t nonce_base;
    uint32_t pad[3];
} shader_in_t;

typedef struct {
    uint32_t count;
    uint32_t nonces[15];
} shader_out_t;

static void gl_init(void) {
    PFNEGLGETPLATFORMDISPLAYEXTPROC eglGetPlatformDisplayEXT =
        (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    if (!eglGetPlatformDisplayEXT) DIE("no eglGetPlatformDisplayEXT");
    egl_dpy = eglGetPlatformDisplayEXT(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (egl_dpy == EGL_NO_DISPLAY) DIE("eglGetPlatformDisplay");
    EGLint maj, min;
    if (!eglInitialize(egl_dpy, &maj, &min)) DIE("eglInitialize");
    LOG("EGL %d.%d %s", maj, min, eglQueryString(egl_dpy, EGL_VENDOR));
    if (!eglBindAPI(EGL_OPENGL_ES_API)) DIE("eglBindAPI");
    EGLint cattr[] = { EGL_CONTEXT_MAJOR_VERSION, 3, EGL_CONTEXT_MINOR_VERSION, 2, EGL_NONE };
    egl_ctx = eglCreateContext(egl_dpy, EGL_NO_CONFIG_KHR, EGL_NO_CONTEXT, cattr);
    if (egl_ctx == EGL_NO_CONTEXT) DIE("eglCreateContext 0x%x", eglGetError());
    if (!eglMakeCurrent(egl_dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, egl_ctx)) DIE("eglMakeCurrent");
    LOG("GL: %s | %s | %s", glGetString(GL_VENDOR), glGetString(GL_RENDERER), glGetString(GL_VERSION));

    char *generated = build_kernel_src();
    LOG("shader src: %zu bytes", strlen(generated));
    GLuint sh = glCreateShader(GL_COMPUTE_SHADER);
    const char *src_ptr = generated;
    glShaderSource(sh, 1, &src_ptr, NULL);
    glCompileShader(sh);
    GLint ok;
    glGetShaderiv(sh, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[8192]; GLsizei n;
        glGetShaderInfoLog(sh, sizeof(log), &n, log);
        DIE("shader compile:\n%.*s", n, log);
    }
    compute_prog = glCreateProgram();
    glAttachShader(compute_prog, sh);
    glLinkProgram(compute_prog);
    glGetProgramiv(compute_prog, GL_LINK_STATUS, &ok);
    if (!ok) {
        char log[8192]; GLsizei n;
        glGetProgramInfoLog(compute_prog, sizeof(log), &n, log);
        DIE("program link:\n%.*s", n, log);
    }

    glGenBuffers(1, &ssbo_in);
    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_in);
    glBufferData(GL_SHADER_STORAGE_BUFFER, sizeof(shader_in_t), NULL, GL_DYNAMIC_DRAW);
    glBindBufferBase(GL_SHADER_STORAGE_BUFFER, 0, ssbo_in);

    glGenBuffers(1, &ssbo_out);
    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_out);
    glBufferData(GL_SHADER_STORAGE_BUFFER, sizeof(shader_out_t), NULL, GL_DYNAMIC_DRAW);
    glBindBufferBase(GL_SHADER_STORAGE_BUFFER, 1, ssbo_out);

    LOG("GL pipeline ready");
}

// ============================================================================
// Main loop
// ============================================================================
static volatile int g_stop = 0;
static void on_sigint(int s){ (void)s; g_stop = 1; }

// ============================================================================
// HTTP server (live hashrate UI). Single pthread accept loop, per-client
// detached thread for SSE streaming. Idle when no clients connected.
// ============================================================================

#define METRICS_RING 300   // 1 sample/sec → 5 min of history
#define METRICS_INTERVAL_MS 1000
static double m_samples[METRICS_RING];
static int m_head, m_count;
static uint64_t m_seq;  // bumped on each push so SSE clients can detect
pthread_mutex_t m_mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t  m_cv = PTHREAD_COND_INITIALIZER;

// Live stats (all under m_mu)
static double   g_min_mh, g_max_mh, g_avg_mh, g_last_mh;
static double   g_inst_mh;        // per-batch instantaneous MH/s, written by main loop
static time_t   g_start_time;
uint64_t g_submitted, g_accepted, g_rejected;
static uint64_t g_lifetime_hashes;
double   g_current_diff = 1.0;
char     g_current_job[64];
uint32_t g_current_ntime, g_current_nbits;
int      g_merkle_branches;
static char     g_pool_url[256], g_worker[64], g_btc_addr[128];

static void metrics_push(double mh) {
    pthread_mutex_lock(&m_mu);
    m_samples[m_head] = mh;
    m_head = (m_head + 1) % METRICS_RING;
    if (m_count < METRICS_RING) m_count++;
    g_last_mh = mh;
    // recompute min/max/avg over ring
    double mn = m_samples[0], mx = m_samples[0], sum = 0;
    int start = (m_head - m_count + METRICS_RING) % METRICS_RING;
    for (int i = 0; i < m_count; i++) {
        double v = m_samples[(start + i) % METRICS_RING];
        if (v < mn) mn = v;
        if (v > mx) mx = v;
        sum += v;
    }
    g_min_mh = mn; g_max_mh = mx; g_avg_mh = m_count ? sum / m_count : 0;
    m_seq++;
    pthread_cond_broadcast(&m_cv);
    pthread_mutex_unlock(&m_mu);
}

// Timer thread: snapshots g_inst_mh into ring every METRICS_INTERVAL_MS.
// Decoupled from GPU loop so chart cadence is steady even when batches span >1s.
static void *metrics_timer_thread(void *unused) {
    (void)unused;
    while (!g_stop) {
        struct timespec ts = { METRICS_INTERVAL_MS / 1000,
                               (long)(METRICS_INTERVAL_MS % 1000) * 1000000L };
        nanosleep(&ts, NULL);
        pthread_mutex_lock(&m_mu);
        double mh = g_inst_mh;
        pthread_mutex_unlock(&m_mu);
        metrics_push(mh);
    }
    return NULL;
}

// Bitcoin network difficulty from compact nbits encoding.
// target = mantissa * 2^(8*(exponent-3)); diff_1_target = 0xffff * 2^208
// network_diff = diff_1_target / target
static double nbits_to_difficulty(uint32_t nbits) {
    uint32_t mant = nbits & 0x00ffffff;
    int exp = (nbits >> 24) & 0xff;
    if (mant == 0 || exp == 0) return 0;
    return (double)0xffff / (double)mant * pow(2.0, 8.0 * (0x1d - exp));
}

// Build JSON snapshot. include_history adds chart history array (used on initial connect).
// Returns bytes written.
static int build_stats_json(char *buf, size_t cap, bool include_history) {
    pthread_mutex_lock(&m_mu);
    time_t now = time(NULL);
    long uptime = g_start_time ? (long)(now - g_start_time) : 0;
    // Network/probability math
    double net_diff = nbits_to_difficulty(g_current_nbits);
    double net_hashrate = net_diff > 0 ? net_diff * 4294967296.0 / 600.0 : 0;  // H/s
    double your_h = g_last_mh * 1e6;  // H/s
    double eta_block_s = (your_h > 0 && net_diff > 0) ? net_diff * 4294967296.0 / your_h : 0;
    double prob_per_day = (net_hashrate > 0 && your_h > 0) ? your_h / net_hashrate * 144.0 : 0;
    // Probability of >=1 block within next 24h (Poisson): 1 - exp(-lambda)
    double prob_24h = 1.0 - exp(-prob_per_day);
    int n = snprintf(buf, cap,
        "{\"mh\":%.3f,\"min\":%.3f,\"max\":%.3f,\"avg\":%.3f,\"samples\":%d,"
        "\"uptime\":%ld,\"submitted\":%llu,\"accepted\":%llu,\"rejected\":%llu,"
        "\"lifetime_hashes\":%llu,\"diff\":%.6f,\"job\":\"%s\",\"ntime\":%u,"
        "\"nbits\":%u,\"merkle_branches\":%d,\"pool\":\"%s\",\"worker\":\"%s\",\"addr\":\"%s\","
        "\"net_diff\":%.6e,\"net_hashrate\":%.6e,\"eta_block_s\":%.6e,"
        "\"prob_block_per_day\":%.6e,\"prob_block_24h\":%.6e",
        g_last_mh, g_min_mh, g_max_mh, g_avg_mh, m_count,
        uptime, (unsigned long long)g_submitted, (unsigned long long)g_accepted,
        (unsigned long long)g_rejected, (unsigned long long)g_lifetime_hashes,
        g_current_diff, g_current_job, g_current_ntime, g_current_nbits,
        g_merkle_branches, g_pool_url, g_worker, g_btc_addr,
        net_diff, net_hashrate, eta_block_s, prob_per_day, prob_24h);
    if (include_history && n > 0 && (size_t)n < cap) {
        n += snprintf(buf + n, cap - n, ",\"history\":[");
        int start = (m_head - m_count + METRICS_RING) % METRICS_RING;
        for (int i = 0; i < m_count && (size_t)n < cap; i++) {
            double v = m_samples[(start + i) % METRICS_RING];
            n += snprintf(buf + n, cap - n, "%s%.3f", i ? "," : "", v);
        }
        if ((size_t)n < cap) n += snprintf(buf + n, cap - n, "]");
    }
    if ((size_t)n < cap) n += snprintf(buf + n, cap - n, "}");
    pthread_mutex_unlock(&m_mu);
    return n;
}

static const char *index_html =
    "<!doctype html><meta charset=utf-8><title>gamble-btc</title>"
    "<style>"
    "html,body{margin:0;min-height:100%;background:#0b0b0b;color:#0f0;font:13px ui-monospace,Menlo,monospace}"
    "header{padding:10px 14px;border-bottom:1px solid #1a1a1a;display:flex;justify-content:space-between;align-items:baseline}"
    "header h1{margin:0;font-size:14px;color:#0f0;font-weight:normal;letter-spacing:1px}"
    "header span{color:#0a0;font-size:12px}"
    "#stat{font-size:32px;color:#0f0;text-shadow:0 0 8px #0f04;padding:4px 14px}"
    "#sub{padding:0 14px 8px;color:#070;font-size:11px}"
    "canvas{display:block;width:100%;height:280px}"
    "#grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:8px;padding:10px 14px}"
    ".cell{border:1px solid #1a1a1a;padding:8px 10px;background:#0d0d0d}"
    ".cell .k{color:#070;font-size:10px;text-transform:uppercase;letter-spacing:1px}"
    ".cell .v{color:#0f0;font-size:15px;margin-top:3px;word-break:break-all}"
    ".cell .v.sm{font-size:11px;color:#0a0}"
    "</style>"
    "<header><h1>gamble-btc :: HD 4600 iGPU</h1><span id=conn>connecting...</span></header>"
    "<div id=stat>--.- MH/s</div>"
    "<div id=sub>last 300 samples (5 min, 1s tick) &middot; SSE stream, server idle when tab closed</div>"
    "<canvas id=c></canvas>"
    "<div id=zoomwrap style='padding:4px 14px 8px;color:#070;font-size:11px;display:flex;align-items:center;gap:10px;flex-wrap:wrap'>"
    "<span>zoom (\xc2\xb1MH/s):</span>"
    "<input type=range id=zoom min=0 max=100 step=0.05 value=64 style='flex:1;max-width:400px;accent-color:#0f0'>"
    "<span id=zoomv>0.500</span>"
    "<span style='margin-left:16px'>x-axis:</span>"
    "<select id=xscale style='background:#0d0d0d;color:#0f0;border:1px solid #1a1a1a;font:inherit;padding:2px 4px'>"
    "<option value=30>30s</option>"
    "<option value=60>1m</option>"
    "<option value=120>2m</option>"
    "<option value=300 selected>5m</option>"
    "</select>"
    "</div>"
    "<div id=grid>"
    "<div class=cell><div class=k>min</div><div class=v id=f_min>-</div></div>"
    "<div class=cell><div class=k>max</div><div class=v id=f_max>-</div></div>"
    "<div class=cell><div class=k>avg</div><div class=v id=f_avg>-</div></div>"
    "<div class=cell><div class=k>uptime</div><div class=v id=f_up>-</div></div>"
    "<div class=cell><div class=k>lifetime hashes</div><div class=v id=f_lh>-</div></div>"
    "<div class=cell><div class=k>shares submitted</div><div class=v id=f_sub>-</div></div>"
    "<div class=cell><div class=k>shares accepted</div><div class=v id=f_acc>-</div></div>"
    "<div class=cell><div class=k>shares rejected</div><div class=v id=f_rej>-</div></div>"
    "<div class=cell><div class=k>pool difficulty</div><div class=v id=f_diff>-</div></div>"
    "<div class=cell><div class=k>current job</div><div class=v sm id=f_job>-</div></div>"
    "<div class=cell><div class=k>ntime</div><div class=v sm id=f_nt>-</div></div>"
    "<div class=cell><div class=k>nbits</div><div class=v sm id=f_nb>-</div></div>"
    "<div class=cell><div class=k>merkle branches</div><div class=v id=f_mb>-</div></div>"
    "<div class=cell><div class=k>pool</div><div class=v sm id=f_pool>-</div></div>"
    "<div class=cell><div class=k>worker</div><div class=v sm id=f_w>-</div></div>"
    "<div class=cell><div class=k>btc address</div><div class=v sm id=f_a>-</div></div>"
    "<div class=cell><div class=k>network difficulty</div><div class=v id=f_nd>-</div></div>"
    "<div class=cell><div class=k>network hashrate</div><div class=v id=f_nh>-</div></div>"
    "<div class=cell><div class=k>your share of net</div><div class=v id=f_share>-</div></div>"
    "<div class=cell><div class=k>expected time to block</div><div class=v id=f_eta>-</div></div>"
    "<div class=cell><div class=k>P(block in 24h)</div><div class=v id=f_p24>-</div></div>"
    "<div class=cell><div class=k>P(block in 1 year)</div><div class=v id=f_py>-</div></div>"
    "<div class=cell><div class=k>expected blocks/year</div><div class=v id=f_bpy>-</div></div>"
    "</div>"
    "<script>"
    "const cv=document.getElementById('c'),ctx=cv.getContext('2d');"
    "const st=document.getElementById('stat'),conn=document.getElementById('conn');"
    "const $=id=>document.getElementById(id);"
    "let data=[];"
    "function fmt(n){if(n>=1e12)return(n/1e12).toFixed(2)+'T';if(n>=1e9)return(n/1e9).toFixed(2)+'G';if(n>=1e6)return(n/1e6).toFixed(2)+'M';if(n>=1e3)return(n/1e3).toFixed(2)+'k';return n.toFixed(0);}"
    "function fmtUp(s){s=Math.floor(s);const d=Math.floor(s/86400),h=Math.floor(s%86400/3600),m=Math.floor(s%3600/60);return(d?d+'d ':'')+String(h).padStart(2,'0')+':'+String(m).padStart(2,'0')+':'+String(s%60).padStart(2,'0');}"
    "function fmtDur(s){if(!isFinite(s)||s<=0)return'-';const yr=s/31557600;if(yr>=1e9)return(yr/1e9).toExponential(2)+' Gyr';if(yr>=1e6)return(yr/1e6).toFixed(2)+' Myr';if(yr>=1e3)return(yr/1e3).toFixed(2)+' kyr';if(yr>=1)return yr.toFixed(2)+' yr';const d=s/86400;if(d>=1)return d.toFixed(2)+' d';const h=s/3600;if(h>=1)return h.toFixed(2)+' h';const m=s/60;if(m>=1)return m.toFixed(2)+' min';return s.toFixed(0)+' s';}"
    "function fmtPct(p){if(p<=0)return'0%';if(p<1e-9)return p.toExponential(2)+' (~'+(p*100).toExponential(2)+'%)';if(p<0.01)return(p*100).toExponential(2)+'%';return(p*100).toFixed(4)+'%';}"
    "function draw(){"
    "const dpr=devicePixelRatio,w=cv.width=cv.clientWidth*dpr,h=cv.height=cv.clientHeight*dpr;"
    "ctx.fillStyle='#0b0b0b';ctx.fillRect(0,0,w,h);"
    "const ml=58*dpr,mr=8*dpr,mt=8*dpr,mb=22*dpr;"
    "const pw=w-ml-mr,ph=h-mt-mb;"
    "const zv=parseFloat($('zoom').value);"
    "const PAD=0.00001*Math.pow(2e7,zv/100);"
    "const xn=parseInt($('xscale').value)||300;"
    "const view=data.slice(-xn);"
    "const nz=view.filter(v=>v>0);"
    "const sorted=nz.slice().sort((a,b)=>a-b);"
    "const pct=(p)=>sorted.length?sorted[Math.min(sorted.length-1,Math.max(0,Math.floor(p*(sorted.length-1))))]:0;"
    "const dmn=sorted.length?pct(0.05):0,dmx=sorted.length?pct(0.95):1;"
    "const mid=(dmn+dmx)/2,half=(dmx-dmn)/2+PAD;"
    "const mn=mid-half,mx=mid+half;"
    "const rng=mx-mn||1;"
    "ctx.font=(10*dpr)+'px ui-monospace,monospace';ctx.fillStyle='#0a0';"
    "ctx.strokeStyle='#0a0';ctx.lineWidth=dpr;ctx.globalAlpha=0.18;ctx.beginPath();"
    "for(let i=0;i<=5;i++){const y=mt+ph*i/5;ctx.moveTo(ml,y);ctx.lineTo(ml+pw,y);}"
    "for(let i=0;i<=6;i++){const x=ml+pw*i/6;ctx.moveTo(x,mt);ctx.lineTo(x,mt+ph);}"
    "ctx.stroke();ctx.globalAlpha=1;"
    "ctx.textAlign='right';ctx.textBaseline='middle';"
    "const ydp=rng<0.00005?7:rng<0.0005?6:rng<0.005?5:rng<0.05?4:rng<0.5?3:2;"
    "for(let i=0;i<=5;i++){const v=mn+rng*(1-i/5),y=mt+ph*i/5;ctx.fillText(v.toFixed(ydp),ml-4*dpr,y);}"
    "ctx.textAlign='center';ctx.textBaseline='top';"
    "const span=Math.max(0,view.length-1);"
    "for(let i=0;i<=6;i++){const sec=Math.round(span*(1-i/6)),x=ml+pw*i/6,lbl=sec===0?'now':'-'+(sec>=60?Math.floor(sec/60)+'m'+(sec%60?(sec%60)+'s':''):sec+'s');ctx.fillText(lbl,x,mt+ph+4*dpr);}"
    "ctx.textAlign='left';ctx.textBaseline='alphabetic';"
    "ctx.fillStyle='#070';ctx.fillText('MH/s',4*dpr,mt+10*dpr);"
    "if(view.length<2)return;"
    "ctx.strokeStyle='#0f0';ctx.lineWidth=2*dpr;ctx.beginPath();"
    "view.forEach((v,i)=>{const x=ml+(i/(view.length-1))*pw,y=mt+ph-((v-mn)/rng)*ph;i?ctx.lineTo(x,y):ctx.moveTo(x,y);});"
    "ctx.stroke();"
    "ctx.strokeStyle='#0f04';ctx.lineWidth=dpr;ctx.beginPath();ctx.moveTo(ml,mt+ph);ctx.lineTo(ml+pw,mt+ph);ctx.stroke();"
    "}"
    "function apply(o){"
    "if(typeof o.mh==='number'){st.textContent=o.mh.toFixed(3)+' MH/s';if(o.history){data=o.history.slice();}else{data.push(o.mh);while(data.length>300)data.shift();}}"
    "if(o.min!=null)$('f_min').textContent=o.min.toFixed(3)+' MH/s';"
    "if(o.max!=null)$('f_max').textContent=o.max.toFixed(3)+' MH/s';"
    "if(o.avg!=null)$('f_avg').textContent=o.avg.toFixed(3)+' MH/s';"
    "if(o.uptime!=null)$('f_up').textContent=fmtUp(o.uptime);"
    "if(o.lifetime_hashes!=null)$('f_lh').textContent=fmt(o.lifetime_hashes)+'H';"
    "if(o.submitted!=null)$('f_sub').textContent=o.submitted;"
    "if(o.accepted!=null)$('f_acc').textContent=o.accepted;"
    "if(o.rejected!=null)$('f_rej').textContent=o.rejected;"
    "if(o.diff!=null)$('f_diff').textContent=o.diff;"
    "if(o.job)$('f_job').textContent=o.job;"
    "if(o.ntime!=null)$('f_nt').textContent='0x'+o.ntime.toString(16);"
    "if(o.nbits!=null)$('f_nb').textContent='0x'+o.nbits.toString(16);"
    "if(o.merkle_branches!=null)$('f_mb').textContent=o.merkle_branches;"
    "if(o.pool)$('f_pool').textContent=o.pool;"
    "if(o.worker)$('f_w').textContent=o.worker;"
    "if(o.addr)$('f_a').textContent=o.addr;"
    "if(o.net_diff!=null)$('f_nd').textContent=o.net_diff>0?fmt(o.net_diff)+' ('+o.net_diff.toExponential(3)+')':'-';"
    "if(o.net_hashrate!=null)$('f_nh').textContent=o.net_hashrate>0?fmt(o.net_hashrate)+'H/s':'-';"
    "if(o.net_hashrate>0&&o.mh>0){const sh=o.mh*1e6/o.net_hashrate;$('f_share').textContent=fmtPct(sh);}"
    "if(o.eta_block_s!=null)$('f_eta').textContent=fmtDur(o.eta_block_s);"
    "if(o.prob_block_24h!=null)$('f_p24').textContent=fmtPct(o.prob_block_24h);"
    "if(o.prob_block_per_day!=null){const py=1-Math.exp(-o.prob_block_per_day*365.25);$('f_py').textContent=fmtPct(py);$('f_bpy').textContent=(o.prob_block_per_day*365.25).toExponential(3);}"
    "draw();"
    "}"
    "const es=new EventSource('/events');"
    "es.onopen=()=>conn.textContent='live';"
    "es.onerror=()=>conn.textContent='reconnecting...';"
    "es.onmessage=e=>{try{apply(JSON.parse(e.data));}catch(x){}};"
    "addEventListener('resize',draw);"
    "$('zoom').addEventListener('input',()=>{const z=parseFloat($('zoom').value),p=0.00001*Math.pow(2e7,z/100);$('zoomv').textContent=p<0.0001?p.toFixed(6):p<0.001?p.toFixed(5):p<0.01?p.toFixed(4):p<1?p.toFixed(3):p.toFixed(2);draw();});"
    "$('xscale').addEventListener('change',draw);"
    "draw();"
    "</script>";

static int write_all(int fd, const char *buf, size_t len) {
    while (len) {
        ssize_t w = send(fd, buf, len, MSG_NOSIGNAL);
        if (w <= 0) return -1;
        buf += w; len -= w;
    }
    return 0;
}

static void *sse_client_thread(void *arg) {
    int fd = (int)(intptr_t)arg;
    const char *hdr =
        "HTTP/1.1 200 OK\r\n"
        "Content-Type: text/event-stream\r\n"
        "Cache-Control: no-cache, no-store\r\n"
        "Connection: close\r\n"
        "X-Accel-Buffering: no\r\n"
        "Access-Control-Allow-Origin: *\r\n"
        "\r\n"
        "retry: 3000\n\n";
    if (write_all(fd, hdr, strlen(hdr)) < 0) goto out;

    // Initial snapshot with full history
    char buf[8192];
    int n = snprintf(buf, sizeof(buf), "data: ");
    n += build_stats_json(buf + n, sizeof(buf) - n, true);
    n += snprintf(buf + n, sizeof(buf) - n, "\n\n");
    if (write_all(fd, buf, n) < 0) goto out;

    pthread_mutex_lock(&m_mu);
    uint64_t seen = m_seq;
    pthread_mutex_unlock(&m_mu);

    // Stream stats snapshots on each metrics push
    while (1) {
        pthread_mutex_lock(&m_mu);
        while (m_seq == seen) pthread_cond_wait(&m_cv, &m_mu);
        seen = m_seq;
        pthread_mutex_unlock(&m_mu);
        char line[2048];
        int len = snprintf(line, sizeof(line), "data: ");
        len += build_stats_json(line + len, sizeof(line) - len, false);
        len += snprintf(line + len, sizeof(line) - len, "\n\n");
        if (write_all(fd, line, len) < 0) break;
    }
out:
    close(fd);
    return NULL;
}

static void *http_thread(void *unused) {
    (void)unused;
    int srv = socket(AF_INET, SOCK_STREAM, 0);
    if (srv < 0) { LOG("http: socket fail"); return NULL; }
    int yes = 1;
    setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes));
    struct sockaddr_in a = {0};
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_ANY);
    a.sin_port = htons(41174);
    if (bind(srv, (struct sockaddr*)&a, sizeof(a)) < 0) {
        LOG("http: bind 41174 fail: %s", strerror(errno));
        close(srv); return NULL;
    }
    if (listen(srv, 16) < 0) { LOG("http: listen fail"); close(srv); return NULL; }
    LOG("http: listening on 0.0.0.0:41174");
    while (!g_stop) {
        int c = accept(srv, NULL, NULL);
        if (c < 0) continue;
        // Read request line + headers (small, single recv)
        char req[2048];
        ssize_t n = recv(c, req, sizeof(req) - 1, 0);
        if (n <= 0) { close(c); continue; }
        req[n] = 0;
        if (strncmp(req, "GET /events", 11) == 0 && (req[11] == ' ' || req[11] == '?')) {
            pthread_t t;
            if (pthread_create(&t, NULL, sse_client_thread, (void*)(intptr_t)c) == 0) {
                pthread_detach(t);
            } else {
                close(c);
            }
        } else if (strncmp(req, "GET / ", 6) == 0 || strncmp(req, "GET /index", 10) == 0) {
            char hdr[256];
            int hlen = snprintf(hdr, sizeof(hdr),
                "HTTP/1.1 200 OK\r\n"
                "Content-Type: text/html; charset=utf-8\r\n"
                "Content-Length: %zu\r\n"
                "Cache-Control: no-store\r\n"
                "Connection: close\r\n"
                "\r\n", strlen(index_html));
            write_all(c, hdr, hlen);
            write_all(c, index_html, strlen(index_html));
            close(c);
        } else {
            const char *r = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            write_all(c, r, strlen(r));
            close(c);
        }
    }
    close(srv);
    return NULL;
}

int main(void) {
    signal(SIGINT, on_sigint);
    signal(SIGTERM, on_sigint);
    const char *btc = getenv("BTC_ADDRESS");
    const char *url = getenv("POOL_URL");
    const char *worker = getenv("WORKER_NAME");
    if (!btc) DIE("BTC_ADDRESS env not set");
    if (!url) url = "stratum+tcp://public-pool.io:21496";
    if (!worker) worker = "x";
    LOG("addr=%s pool=%s worker=%s", btc, url, worker);

    g_start_time = time(NULL);
    snprintf(g_pool_url, sizeof(g_pool_url), "%s", url);
    snprintf(g_worker, sizeof(g_worker), "%s", worker);
    snprintf(g_btc_addr, sizeof(g_btc_addr), "%s", btc);

    gl_init();

    pthread_t ht;
    if (pthread_create(&ht, NULL, http_thread, NULL) == 0) pthread_detach(ht);
    else LOG("http: pthread_create failed");

    pthread_t mt;
    if (pthread_create(&mt, NULL, metrics_timer_thread, NULL) == 0) pthread_detach(mt);
    else LOG("metrics: pthread_create failed");

    stratum_t S = {0};
    snprintf(S.username, sizeof(S.username), "%s", btc);
    snprintf(S.password, sizeof(S.password), "%s", worker);
    if (stratum_connect(&S, url) < 0) DIE("stratum connect");
    if (stratum_subscribe_authorize(&S) < 0) DIE("stratum subscribe");

    // wait for first job
    while (!S.have_job && !g_stop) stratum_pump(&S, 5000);

    uint64_t total_hashes = 0;
    time_t t_last = time(NULL);
    struct timespec t_batch_prev;
    clock_gettime(CLOCK_MONOTONIC, &t_batch_prev);
    uint32_t en2_counter = 0;
    const uint32_t BATCH_NONCES = 1u << 24;  // 16M per dispatch — amortize CPU↔GPU overhead
    const uint32_t WG_SIZE = 64;

    while (!g_stop) {
        // Drain incoming stratum messages (non-blocking-ish)
        stratum_pump(&S, 0);
        if (!S.have_job) { stratum_pump(&S, 1000); continue; }

        // Build current work: pick fresh extranonce2
        uint8_t en2[8] = {0};
        int en2_len = S.extranonce2_size;
        for (int i = 0; i < en2_len; i++) en2[i] = (en2_counter >> (8*i)) & 0xff;
        en2_counter++;

        uint8_t merkle_root[32];
        compute_merkle_root(merkle_root, &S, en2, en2_len);

        shader_in_t in = {0};
        build_header_midstate(in.midstate, &in.w0, &S, merkle_root);

        char en2_hex[32];
        bin_to_hex(en2_hex, en2, en2_len);
        char job_id_snapshot[64];
        snprintf(job_id_snapshot, sizeof(job_id_snapshot), "%s", S.job_id);
        uint32_t ntime_snapshot = S.ntime_be;

        // Sweep full 2^32 nonce range until new job arrives.
        // (1<<32) / BATCH_NONCES = 2^32 / 2^24 = 256 batches per ntime cycle
        uint32_t base = 0;
        for (int batch = 0; batch < (int)((1ull<<32) / BATCH_NONCES); batch++, base += BATCH_NONCES) {
            if (g_stop) break;
            // Reset output, set input
            shader_out_t out_init = {0};
            glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_out);
            glBufferSubData(GL_SHADER_STORAGE_BUFFER, 0, sizeof(out_init), &out_init);

            in.nonce_base = base;
            glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_in);
            glBufferSubData(GL_SHADER_STORAGE_BUFFER, 0, sizeof(in), &in);

            glUseProgram(compute_prog);
            const uint32_t WGX = 256;
            const uint32_t WGY = (BATCH_NONCES / WG_SIZE) / WGX;
            glDispatchCompute(WGX, WGY, 1);
            GLenum derr = glGetError();
            if (derr != GL_NO_ERROR) LOG("dispatch err 0x%x", derr);
            glMemoryBarrier(GL_SHADER_STORAGE_BARRIER_BIT);
            glFinish();

            glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_out);
            shader_out_t *res = glMapBufferRange(GL_SHADER_STORAGE_BUFFER, 0, sizeof(shader_out_t), GL_MAP_READ_BIT);
            if (!res) { LOG("glMapBufferRange err 0x%x", glGetError()); break; }
            uint32_t nfound = res->count;
            uint32_t nonces[15];
            if (nfound > 15) nfound = 15;
            for (uint32_t i = 0; i < nfound; i++) nonces[i] = res->nonces[i];
            glUnmapBuffer(GL_SHADER_STORAGE_BUFFER);

            for (uint32_t i = 0; i < nfound; i++) {
                stratum_submit(&S, job_id_snapshot, en2_hex, ntime_snapshot, nonces[i]);
            }

            total_hashes += BATCH_NONCES;
            // Per-batch instantaneous MH/s — fed to metrics timer thread (pushes 1Hz)
            struct timespec t_batch_now;
            clock_gettime(CLOCK_MONOTONIC, &t_batch_now);
            double batch_secs = (t_batch_now.tv_sec - t_batch_prev.tv_sec) +
                                (t_batch_now.tv_nsec - t_batch_prev.tv_nsec) / 1e9;
            t_batch_prev = t_batch_now;
            double inst = batch_secs > 0 ? (double)BATCH_NONCES / 1e6 / batch_secs : 0;
            pthread_mutex_lock(&m_mu);
            g_lifetime_hashes += BATCH_NONCES;
            g_inst_mh = inst;
            pthread_mutex_unlock(&m_mu);
            time_t now = time(NULL);
            if (now - t_last >= 10) {
                double secs = (double)(now - t_last);
                double mh = (double)total_hashes / 1e6 / secs;
                LOG("hashrate ~%.2f MH/s (batch=%u, found=%u)", mh, BATCH_NONCES, nfound);
                total_hashes = 0;
                t_last = now;
            }

            // Check for new job mid-sweep
            stratum_pump(&S, 0);
            if (strcmp(S.job_id, job_id_snapshot) != 0 && S.clean) {
                LOG("new job, abandoning sweep");
                break;
            }
        }
    }

    LOG("shutting down");
    eglDestroyContext(egl_dpy, egl_ctx);
    eglTerminate(egl_dpy);
    return 0;
}
