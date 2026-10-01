// gamble-btc - DIY iGPU SHA-256d solo miner
// Path: EGL surfaceless + GLES 3.2 compute shader
// Pool: stratum+tcp (public-pool.io)
//
// Educational. NOT optimized for max hashrate.

#define _GNU_SOURCE
#include "bench.h"
#include "backend.h"
#include "gles_tuning.h"
#include "opencl.h"
#include "http_config.h"
#include "stratum_protocol.h"

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
#include <inttypes.h>
#include <limits.h>
#include <signal.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <string.h>
#include <stdbool.h>
#include <time.h>
#include <ctype.h>
#include <math.h>
#include <cjson/cJSON.h>

#define LOG(fmt, ...) do { fprintf(stderr, "[%ld] " fmt "\n", time(NULL), ##__VA_ARGS__); } while(0)
#define DIE(fmt, ...) do { LOG("FATAL " fmt, ##__VA_ARGS__); exit(1); } while(0)

// ============================================================================
// Host-side SHA-256 helpers used only for work construction and tests.
// Mining work is intentionally dispatched to iGPU backends, not a CPU backend.
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

// GLES shader generation lives in src/gles_tuning.c so parser and source
// variants can be tested without EGL/GLES development headers.

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
    size_t extranonce1_len;
    size_t extranonce2_size;
    int next_id;
    char username[128];
    char password[64];
    double diff;
    uint32_t target[8];
    uint64_t target_generation;
    int pending_submit_ids[64];
    size_t pending_submit_count;
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
    char target_reason[128] = "";
    if (gbtc_target_from_difficulty(s->diff, s->target,
                                    target_reason, sizeof(target_reason)) != 0) {
        LOG("stratum: default target failed: %s", target_reason);
        close(fd);
        return -1;
    }
    LOG("stratum: connected");
    return 0;
}

static int stratum_send(stratum_t *s, const char *json) {
    size_t len = strlen(json);
    char buf[8192];
    if (len + 2 > sizeof(buf)) return -1;
    memcpy(buf, json, len);
    buf[len] = '\n'; buf[len+1] = 0;
    size_t sent = 0;
    while (sent < len + 1u) {
        ssize_t w = send(s->fd, buf + sent, len + 1u - sent, MSG_NOSIGNAL);
        if (w <= 0) { LOG("send fail %zd", w); return -1; }
        sent += (size_t)w;
    }
    return 0;
}

static int stratum_send_method(stratum_t *s, int id, const char *method, cJSON *params)
{
    cJSON *root = cJSON_CreateObject();
    if (!root || !params || !cJSON_AddNumberToObject(root, "id", id) ||
        !cJSON_AddStringToObject(root, "method", method)) {
        cJSON_Delete(params);
        cJSON_Delete(root);
        return -1;
    }
    cJSON_AddItemToObject(root, "params", params);
    char *json = cJSON_PrintUnformatted(root);
    int result = json ? stratum_send(s, json) : -1;
    cJSON_free(json);
    cJSON_Delete(root);
    return result;
}

static char *stratum_readline(stratum_t *s, int timeout_ms) {
    // Fill rxbuf, return ptr to one line (NUL-term) or NULL
    while (1) {
        char *nl = memchr(s->rxbuf, '\n', s->rxlen);
        if (nl) {
            static char line[8192];
            size_t llen = nl - s->rxbuf;
            size_t consumed = llen + 1u;
            if (llen >= sizeof(line)) {
                memmove(s->rxbuf, s->rxbuf + consumed, s->rxlen - consumed);
                s->rxlen -= consumed;
                LOG("stratum: rejected line longer than %zu bytes", sizeof(line) - 1u);
                return NULL;
            }
            memcpy(line, s->rxbuf, llen); line[llen] = 0;
            memmove(s->rxbuf, s->rxbuf + consumed, s->rxlen - consumed);
            s->rxlen -= consumed;
            return line;
        }
        if (s->rxlen >= sizeof(s->rxbuf) - 1u) {
            s->rxlen = 0;
            LOG("stratum: rejected unterminated oversized message");
            return NULL;
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

static int stratum_handle_notify(stratum_t *s, const char *line) {
    gbtc_stratum_job_t job;
    char reason[256] = "";
    if (gbtc_parse_notify(line, &job, reason, sizeof(reason)) != 0) {
        LOG("stratum: rejected mining.notify: %s", reason);
        return -1;
    }
    snprintf(s->job_id, sizeof(s->job_id), "%s", job.job_id);
    memcpy(s->prevhash, job.prevhash, sizeof(s->prevhash));
    snprintf(s->coinb1_hex, sizeof(s->coinb1_hex), "%s", job.coinb1_hex);
    snprintf(s->coinb2_hex, sizeof(s->coinb2_hex), "%s", job.coinb2_hex);
    s->merkle_count = (int)job.merkle_count;
    memcpy(s->merkle, job.merkle, job.merkle_count * sizeof(job.merkle[0]));
    s->version_be = job.version_be;
    s->nbits_be = job.nbits_be;
    s->ntime_be = job.ntime_be;
    s->clean = job.clean;
    s->have_job = true;
    LOG("notify: job=%s ntime=%08x nbits=%08x clean=%d merkle_branches=%d",
        s->job_id, s->ntime_be, s->nbits_be, s->clean, s->merkle_count);
    pthread_mutex_lock(&m_mu);
    snprintf(g_current_job, sizeof(g_current_job), "%s", s->job_id);
    g_current_ntime = s->ntime_be;
    g_current_nbits = s->nbits_be;
    g_merkle_branches = s->merkle_count;
    pthread_mutex_unlock(&m_mu);
    return 0;
}

static void stratum_handle_notification(stratum_t *s, const char *line,
                                        const char *method) {
    if (strcmp(method, "mining.notify") == 0) {
        stratum_handle_notify(s, line);
    } else if (strcmp(method, "mining.set_difficulty") == 0) {
        double difficulty = 0;
        uint32_t target[8];
        char reason[256] = "";
        if (gbtc_parse_set_difficulty(line, &difficulty, target, reason,
                                      sizeof(reason)) == 0) {
            s->diff = difficulty;
            memcpy(s->target, target, sizeof(s->target));
            s->target_generation++;
            LOG("set_difficulty %f", s->diff);
            pthread_mutex_lock(&m_mu);
            g_current_diff = s->diff;
            pthread_mutex_unlock(&m_mu);
        } else {
            LOG("stratum: rejected mining.set_difficulty: %s", reason);
        }
    } else {
        LOG("stratum: ignored unsupported method %s", method);
    }
}

static const char *stratum_read_handshake_line(void *context) {
    return stratum_readline(context, 10000);
}

static void stratum_handle_handshake_notification(void *context,
                                                  const char *line,
                                                  const char *method) {
    stratum_handle_notification(context, line, method);
}

static int stratum_subscribe_authorize(stratum_t *s) {
    int subscribe_id = s->next_id++;
    cJSON *subscribe_params = cJSON_CreateArray();
    if (!subscribe_params || !cJSON_AddItemToArray(subscribe_params, cJSON_CreateString("gamble-btc/0.2")) ||
        stratum_send_method(s, subscribe_id, "mining.subscribe", subscribe_params) < 0) return -1;
    gbtc_stratum_subscription_t subscription;
    char reason[256] = "";
    if (gbtc_stratum_wait_subscribe_response(
            subscribe_id, stratum_read_handshake_line,
            stratum_handle_handshake_notification, s, &subscription,
            reason, sizeof(reason)) != 0) {
        LOG("stratum: subscribe rejected: %s",
            reason[0] ? reason : "invalid subscription response");
        return -1;
    }
    snprintf(s->extranonce1, sizeof(s->extranonce1), "%s", subscription.extranonce1);
    s->extranonce1_len = subscription.extranonce1_len;
    s->extranonce2_size = subscription.extranonce2_size;
    LOG("subscribe: extranonce1=%s en2_size=%zu", s->extranonce1, s->extranonce2_size);

    int authorize_id = s->next_id++;
    cJSON *authorize_params = cJSON_CreateArray();
    if (!authorize_params ||
        !cJSON_AddItemToArray(authorize_params, cJSON_CreateString(s->username)) ||
        !cJSON_AddItemToArray(authorize_params, cJSON_CreateString(s->password)) ||
        stratum_send_method(s, authorize_id, "mining.authorize", authorize_params) < 0) return -1;
    bool authorized = false;
    reason[0] = '\0';
    if (gbtc_stratum_wait_boolean_response(
            authorize_id, stratum_read_handshake_line,
            stratum_handle_handshake_notification, s, &authorized,
            reason, sizeof(reason)) != 0 || !authorized) {
        LOG("stratum: authorization denied or malformed: %s",
            reason[0] ? reason : "pool returned false");
        return -1;
    }
    LOG("authorized as %s", s->username);
    return 0;
}

static int pending_submit_take(stratum_t *s, int id)
{
    for (size_t i = 0; i < s->pending_submit_count; i++) {
        if (s->pending_submit_ids[i] == id) {
            s->pending_submit_ids[i] = s->pending_submit_ids[s->pending_submit_count - 1u];
            s->pending_submit_count--;
            return 0;
        }
    }
    return -1;
}

static void stratum_pump(stratum_t *s, int timeout_ms) {
    char *line = stratum_readline(s, timeout_ms);
    if (!line) return;
    const char *end = NULL;
    cJSON *root = cJSON_ParseWithLengthOpts(line, strlen(line) + 1u, &end, 1);
    if (!root || !cJSON_IsObject(root)) {
        LOG("stratum: rejected malformed JSON message");
        cJSON_Delete(root);
        return;
    }
    cJSON *method = cJSON_GetObjectItemCaseSensitive(root, "method");
    if (cJSON_IsString(method)) {
        stratum_handle_notification(s, line, method->valuestring);
    } else {
        cJSON *id_item = cJSON_GetObjectItemCaseSensitive(root, "id");
        if (cJSON_IsNumber(id_item) && isfinite(id_item->valuedouble) &&
            floor(id_item->valuedouble) == id_item->valuedouble &&
            id_item->valuedouble >= 0 && id_item->valuedouble <= INT_MAX) {
            int id = (int)id_item->valuedouble;
            bool accepted = false;
            char reason[256] = "";
            if (pending_submit_take(s, id) == 0 &&
                gbtc_parse_boolean_response(line, id, &accepted, reason, sizeof(reason)) == 0) {
                LOG("submit response: id=%d result=%d", id, accepted);
                pthread_mutex_lock(&m_mu);
                if (accepted) g_accepted++; else g_rejected++;
                pthread_mutex_unlock(&m_mu);
            } else {
                LOG("stratum: rejected unexpected or malformed response id=%d", id);
            }
        } else {
            LOG("stratum: rejected response with invalid id");
        }
    }
    cJSON_Delete(root);
}

static int stratum_submit(stratum_t *s, const char *job_id, const char *en2_hex,
                           uint32_t ntime_be, uint32_t nonce_be) {
    if (s->pending_submit_count >= sizeof(s->pending_submit_ids) / sizeof(s->pending_submit_ids[0])) {
        LOG("stratum: too many pending share responses");
        return -1;
    }
    int id = s->next_id++;
    char ntime[9], nonce[9];
    snprintf(ntime, sizeof(ntime), "%08x", ntime_be);
    snprintf(nonce, sizeof(nonce), "%08x", nonce_be);
    cJSON *params = cJSON_CreateArray();
    if (!params || !cJSON_AddItemToArray(params, cJSON_CreateString(s->username)) ||
        !cJSON_AddItemToArray(params, cJSON_CreateString(job_id)) ||
        !cJSON_AddItemToArray(params, cJSON_CreateString(en2_hex)) ||
        !cJSON_AddItemToArray(params, cJSON_CreateString(ntime)) ||
        !cJSON_AddItemToArray(params, cJSON_CreateString(nonce)) ||
        stratum_send_method(s, id, "mining.submit", params) < 0) return -1;
    s->pending_submit_ids[s->pending_submit_count++] = id;
    LOG("SUBMIT: id=%d job=%s ntime=%s nonce=%s", id, job_id, ntime, nonce);
    pthread_mutex_lock(&m_mu);
    g_submitted++;
    pthread_mutex_unlock(&m_mu);
    return 0;
}

// ============================================================================
// Work building (coinbase + merkle root + header midstate)
// ============================================================================
static void compute_merkle_root(uint8_t mr[32], const stratum_t *s,
                                const uint8_t *en2, size_t en2_len) {
    uint8_t cb1[1024], cb2[1024];
    int cb1_len = strlen(s->coinb1_hex) / 2;
    int cb2_len = strlen(s->coinb2_hex) / 2;
    hex_to_bin(cb1, s->coinb1_hex, cb1_len);
    hex_to_bin(cb2, s->coinb2_hex, cb2_len);
    uint8_t en1[32];
    hex_to_bin(en1, s->extranonce1, s->extranonce1_len);
    uint8_t coinbase[2048];
    size_t p = 0;
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
static GLint gles_max_groups[3];
static gbtc_gles_config_t gles_config;

static char g_backend[32] = "unknown";
static char g_backend_api[32] = "none";
static char g_device_path[128] = "";
static char g_device_vendor[128] = "";
static char g_device_name[256] = "";
static char g_driver_name[256] = "";
static char g_fallback_reason[256] = "";

typedef struct {
    uint32_t midstate[8];
    uint32_t tail3[3];
    uint32_t target_word0;
    uint32_t nonce_base;
    uint32_t pad[2];
} shader_in_t;

typedef struct {
    uint32_t count;
    uint32_t nonces[15];
} shader_out_t;

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

static void copy_gl_string(char *dst, size_t cap, GLenum name)
{
    const GLubyte *s = glGetString(name);
    snprintf(dst, cap, "%s", s ? (const char *)s : "unknown");
}

static void gles_destroy(void)
{
    if (ssbo_in) {
        glDeleteBuffers(1, &ssbo_in);
        ssbo_in = 0;
    }
    if (ssbo_out) {
        glDeleteBuffers(1, &ssbo_out);
        ssbo_out = 0;
    }
    if (compute_prog) {
        glDeleteProgram(compute_prog);
        compute_prog = 0;
    }
    if (egl_dpy && egl_dpy != EGL_NO_DISPLAY) {
        if (egl_ctx && egl_ctx != EGL_NO_CONTEXT) {
            eglDestroyContext(egl_dpy, egl_ctx);
            egl_ctx = EGL_NO_CONTEXT;
        }
        eglTerminate(egl_dpy);
        egl_dpy = EGL_NO_DISPLAY;
    }
}

static int gles_create_context(char *reason, size_t reason_cap)
{
    const char *device = getenv("GBTC_DEVICE");
    if (!device || !device[0]) device = "/dev/dri/renderD128";
    snprintf(g_device_path, sizeof(g_device_path), "%s", device);
    if (access(device, R_OK | W_OK) != 0) {
        set_reason(reason, reason_cap, "%s is not accessible: %s", device, strerror(errno));
        return -1;
    }

    PFNEGLGETPLATFORMDISPLAYEXTPROC eglGetPlatformDisplayEXT =
        (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    if (!eglGetPlatformDisplayEXT) {
        set_reason(reason, reason_cap, "no eglGetPlatformDisplayEXT");
        return -1;
    }
    egl_dpy = eglGetPlatformDisplayEXT(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (egl_dpy == EGL_NO_DISPLAY) {
        set_reason(reason, reason_cap, "eglGetPlatformDisplay failed");
        return -1;
    }
    EGLint maj, min;
    if (!eglInitialize(egl_dpy, &maj, &min)) {
        set_reason(reason, reason_cap, "eglInitialize failed: 0x%x", eglGetError());
        gles_destroy();
        return -1;
    }
    LOG("EGL %d.%d %s", maj, min, eglQueryString(egl_dpy, EGL_VENDOR));
    if (!eglBindAPI(EGL_OPENGL_ES_API)) {
        set_reason(reason, reason_cap, "eglBindAPI(EGL_OPENGL_ES_API) failed: 0x%x", eglGetError());
        gles_destroy();
        return -1;
    }
    EGLint cattr[] = { EGL_CONTEXT_MAJOR_VERSION, 3, EGL_CONTEXT_MINOR_VERSION, 2, EGL_NONE };
    egl_ctx = eglCreateContext(egl_dpy, EGL_NO_CONFIG_KHR, EGL_NO_CONTEXT, cattr);
    if (egl_ctx == EGL_NO_CONTEXT) {
        set_reason(reason, reason_cap, "eglCreateContext GLES 3.2 failed: 0x%x", eglGetError());
        gles_destroy();
        return -1;
    }
    if (!eglMakeCurrent(egl_dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, egl_ctx)) {
        set_reason(reason, reason_cap, "eglMakeCurrent failed: 0x%x", eglGetError());
        gles_destroy();
        return -1;
    }
    copy_gl_string(g_device_vendor, sizeof(g_device_vendor), GL_VENDOR);
    copy_gl_string(g_device_name, sizeof(g_device_name), GL_RENDERER);
    copy_gl_string(g_driver_name, sizeof(g_driver_name), GL_VERSION);
    for (int i = 0; i < 3; i++) glGetIntegeri_v(GL_MAX_COMPUTE_WORK_GROUP_COUNT, i, &gles_max_groups[i]);
    LOG("GL: %s | %s | %s", glGetString(GL_VENDOR), glGetString(GL_RENDERER), glGetString(GL_VERSION));
    return 0;
}

static int gles_compile_compute(const char *src, GLuint *program, char *reason, size_t reason_cap)
{
    GLuint sh = glCreateShader(GL_COMPUTE_SHADER);
    if (!sh) {
        set_reason(reason, reason_cap, "glCreateShader(GL_COMPUTE_SHADER) failed");
        return -1;
    }
    glShaderSource(sh, 1, &src, NULL);
    glCompileShader(sh);
    GLint ok;
    glGetShaderiv(sh, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[8192]; GLsizei n = 0;
        glGetShaderInfoLog(sh, sizeof(log), &n, log);
        set_reason(reason, reason_cap, "compute shader compile failed: %.*s", n, log);
        glDeleteShader(sh);
        return -1;
    }
    GLuint prog = glCreateProgram();
    glAttachShader(prog, sh);
    glLinkProgram(prog);
    glDeleteShader(sh);
    glGetProgramiv(prog, GL_LINK_STATUS, &ok);
    if (!ok) {
        char log[8192]; GLsizei n = 0;
        glGetProgramInfoLog(prog, sizeof(log), &n, log);
        set_reason(reason, reason_cap, "compute program link failed: %.*s", n, log);
        glDeleteProgram(prog);
        return -1;
    }
    *program = prog;
    return 0;
}

static int gles_probe(char *reason, size_t reason_cap)
{
    static const char *probe_src =
        "#version 320 es\n"
        "layout(local_size_x = 1) in;\n"
        "void main(){}\n";
    GLuint probe_prog = 0;
    if (gles_create_context(reason, reason_cap) != 0) return -1;
    int rc = gles_compile_compute(probe_src, &probe_prog, reason, reason_cap);
    if (probe_prog) glDeleteProgram(probe_prog);
    if (rc == 0) {
        set_reason(reason, reason_cap, "%s / %s / %s",
                   g_device_vendor, g_device_name, g_driver_name);
    }
    gles_destroy();
    return rc;
}

static int gles_init(char *reason, size_t reason_cap)
{
    if (gbtc_gles_config_from_env(&gles_config, reason, reason_cap) != 0) {
        return -1;
    }
    if (gles_create_context(reason, reason_cap) != 0) return -1;

    char *generated = gbtc_gles_build_kernel_src(&gles_config, reason, reason_cap);
    if (!generated) {
        gles_destroy();
        return -1;
    }
    size_t generated_len = strlen(generated);
    LOG("shader compile: kernel=%s local_size=%u src_bytes=%zu renderer=\"%s\" gl=\"%s\"",
        gbtc_gles_kernel_name(gles_config.kernel), gles_config.local_size,
        generated_len, g_device_name, g_driver_name);
    if (gles_compile_compute(generated, &compute_prog, reason, reason_cap) != 0) {
        LOG("shader compile failed: kernel=%s local_size=%u reason=%s",
            gbtc_gles_kernel_name(gles_config.kernel), gles_config.local_size, reason);
        free(generated);
        gles_destroy();
        return -1;
    }
    LOG("shader compile ok: kernel=%s local_size=%u src_bytes=%zu",
        gbtc_gles_kernel_name(gles_config.kernel), gles_config.local_size, generated_len);
    free(generated);

    glGenBuffers(1, &ssbo_in);
    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_in);
    glBufferData(GL_SHADER_STORAGE_BUFFER, sizeof(shader_in_t), NULL, GL_DYNAMIC_DRAW);
    glBindBufferBase(GL_SHADER_STORAGE_BUFFER, 0, ssbo_in);

    glGenBuffers(1, &ssbo_out);
    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_out);
    glBufferData(GL_SHADER_STORAGE_BUFFER, sizeof(shader_out_t), NULL, GL_DYNAMIC_DRAW);
    glBindBufferBase(GL_SHADER_STORAGE_BUFFER, 1, ssbo_out);

    set_reason(reason, reason_cap, "GLES pipeline ready on %s", g_device_name);
    LOG("GL pipeline ready kernel=%s local_size=%u",
        gbtc_gles_kernel_name(gles_config.kernel), gles_config.local_size);
    return 0;
}

static int gles_run_batch(const gbtc_work_batch_t *work, gbtc_backend_result_t *result)
{
    if (!work || !result || work->nonce_count == 0) return -1;
    uint32_t nonces_per_invocation = gbtc_gles_kernel_nonces_per_invocation(gles_config.kernel);
    uint32_t dispatch_quantum = gles_config.local_size * nonces_per_invocation;
    if ((work->nonce_count % dispatch_quantum) != 0) {
        LOG("gles: nonce_count=%u is not divisible by dispatch quantum %u (local_size=%u nonces_per_invocation=%u)",
            work->nonce_count, dispatch_quantum, gles_config.local_size, nonces_per_invocation);
        return -1;
    }

    uint32_t invocations = work->nonce_count / nonces_per_invocation;
    uint32_t groups = invocations / gles_config.local_size;
    uint32_t wgx = groups < 256u ? groups : 256u;
    while (wgx > 1 && (groups % wgx) != 0) wgx--;
    uint32_t wgy = groups / wgx;
    if (gles_max_groups[0] > 0 && (GLint)wgx > gles_max_groups[0]) {
        LOG("gles: dispatch x=%u exceeds device limit %d", wgx, gles_max_groups[0]);
        return -1;
    }
    if (gles_max_groups[1] > 0 && (GLint)wgy > gles_max_groups[1]) {
        LOG("gles: dispatch y=%u exceeds device limit %d", wgy, gles_max_groups[1]);
        return -1;
    }

    shader_out_t out_init = {0};
    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_out);
    glBufferSubData(GL_SHADER_STORAGE_BUFFER, 0, sizeof(out_init), &out_init);

    shader_in_t in = {0};
    memcpy(in.midstate, work->midstate, sizeof(in.midstate));
    memcpy(in.tail3, work->tail3, sizeof(in.tail3));
    in.target_word0 = work->target[0];
    in.nonce_base = work->nonce_base;
    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_in);
    glBufferSubData(GL_SHADER_STORAGE_BUFFER, 0, sizeof(in), &in);

    glUseProgram(compute_prog);
    glDispatchCompute(wgx, wgy, 1);
    GLenum derr = glGetError();
    if (derr != GL_NO_ERROR) {
        LOG("dispatch err 0x%x", derr);
        return -1;
    }
    glMemoryBarrier(GL_SHADER_STORAGE_BARRIER_BIT);
    glFinish();

    glBindBuffer(GL_SHADER_STORAGE_BUFFER, ssbo_out);
    shader_out_t *res = glMapBufferRange(GL_SHADER_STORAGE_BUFFER, 0, sizeof(shader_out_t), GL_MAP_READ_BIT);
    if (!res) {
        LOG("glMapBufferRange err 0x%x", glGetError());
        return -1;
    }
    result->count = res->count;
    if (result->count > GBTC_MAX_FOUND_NONCES) result->count = GBTC_MAX_FOUND_NONCES;
    for (uint32_t i = 0; i < result->count; i++) result->nonces[i] = res->nonces[i];
    result->hashes_done = work->nonce_count;
    glUnmapBuffer(GL_SHADER_STORAGE_BUFFER);
    return 0;
}

const gbtc_backend_t gbtc_gles_backend = {
    .kind = GBTC_BACKEND_GLES,
    .name = "gles",
    .api = "gles",
    .probe = gles_probe,
    .init = gles_init,
    .run_batch = gles_run_batch,
    .shutdown = gles_destroy,
};

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

#define STATUS_JSON_CAP 32768u

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
    char job[385], pool[1537], worker[385], addr[769];
    char backend[193], backend_api[193], device_path[769], device_vendor[769];
    char device_name[1537], driver_name[1537], fallback_reason[1537];
    char escape_reason[128] = "";
    if (gbtc_json_escape(g_current_job, job, sizeof(job), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_pool_url, pool, sizeof(pool), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_worker, worker, sizeof(worker), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_btc_addr, addr, sizeof(addr), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_backend, backend, sizeof(backend), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_backend_api, backend_api, sizeof(backend_api), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_device_path, device_path, sizeof(device_path), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_device_vendor, device_vendor, sizeof(device_vendor), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_device_name, device_name, sizeof(device_name), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_driver_name, driver_name, sizeof(driver_name), escape_reason, sizeof(escape_reason)) != 0 ||
        gbtc_json_escape(g_fallback_reason, fallback_reason, sizeof(fallback_reason),
                         escape_reason, sizeof(escape_reason)) != 0) {
        int failed = snprintf(buf, cap, "{\"error\":\"status serialization failed\"}");
        pthread_mutex_unlock(&m_mu);
        return failed > 0 && (size_t)failed < cap ? failed : 0;
    }
    int n = snprintf(buf, cap,
        "{\"mh\":%.3f,\"min\":%.3f,\"max\":%.3f,\"avg\":%.3f,\"samples\":%d,"
        "\"uptime\":%ld,\"submitted\":%llu,\"accepted\":%llu,\"rejected\":%llu,"
        "\"lifetime_hashes\":%llu,\"diff\":%.6f,\"job\":\"%s\",\"ntime\":%u,"
        "\"nbits\":%u,\"merkle_branches\":%d,\"pool\":\"%s\",\"worker\":\"%s\",\"addr\":\"%s\","
        "\"backend\":\"%s\",\"backend_api\":\"%s\",\"device_path\":\"%s\","
        "\"device_vendor\":\"%s\",\"device_name\":\"%s\",\"driver_name\":\"%s\","
        "\"fallback_reason\":\"%s\","
        "\"net_diff\":%.6e,\"net_hashrate\":%.6e,\"eta_block_s\":%.6e,"
        "\"prob_block_per_day\":%.6e,\"prob_block_24h\":%.6e",
        g_last_mh, g_min_mh, g_max_mh, g_avg_mh, m_count,
        uptime, (unsigned long long)g_submitted, (unsigned long long)g_accepted,
        (unsigned long long)g_rejected, (unsigned long long)g_lifetime_hashes,
        g_current_diff, job, g_current_ntime, g_current_nbits,
        g_merkle_branches, pool, worker, addr,
        backend, backend_api, device_path, device_vendor, device_name,
        driver_name, fallback_reason,
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
    if (n < 0) n = 0;
    if (cap > 0 && (size_t)n >= cap) n = (int)cap - 1;
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
    "<header><h1 id=rig_title>gamble-btc :: iGPU</h1><span id=conn>connecting...</span></header>"
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
    "<div class=cell><div class=k>backend</div><div class=v id=f_backend>-</div></div>"
    "<div class=cell><div class=k>backend api</div><div class=v id=f_backend_api>-</div></div>"
    "<div class=cell><div class=k>device path</div><div class=v sm id=f_device_path>-</div></div>"
    "<div class=cell><div class=k>device vendor</div><div class=v sm id=f_device_vendor>-</div></div>"
    "<div class=cell><div class=k>device name</div><div class=v sm id=f_device_name>-</div></div>"
    "<div class=cell><div class=k>driver</div><div class=v sm id=f_driver_name>-</div></div>"
    "<div class=cell><div class=k>fallback reason</div><div class=v sm id=f_fallback_reason>-</div></div>"
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
    "const rig=$('rig_title');"
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
    "if(o.job!=null)$('f_job').textContent=o.job||'waiting for stratum job';"
    "if(o.ntime!=null)$('f_nt').textContent='0x'+o.ntime.toString(16);"
    "if(o.nbits!=null)$('f_nb').textContent='0x'+o.nbits.toString(16);"
    "if(o.merkle_branches!=null)$('f_mb').textContent=o.merkle_branches;"
    "if(o.pool)$('f_pool').textContent=o.pool;"
    "if(o.worker)$('f_w').textContent=o.worker;"
    "if(o.addr)$('f_a').textContent=o.addr;"
    "if(o.backend)$('f_backend').textContent=o.backend;"
    "if(o.backend_api)$('f_backend_api').textContent=o.backend_api;"
    "if(o.device_path)$('f_device_path').textContent=o.device_path;"
    "if(o.device_vendor)$('f_device_vendor').textContent=o.device_vendor;"
    "if(o.device_name){$('f_device_name').textContent=o.device_name;if(rig)rig.textContent='gamble-btc :: '+o.device_name;}"
    "if(o.driver_name)$('f_driver_name').textContent=o.driver_name;"
    "if(o.fallback_reason)$('f_fallback_reason').textContent=o.fallback_reason;"
    "if(o.net_diff!=null)$('f_nd').textContent=o.net_diff>0?fmt(o.net_diff)+' ('+o.net_diff.toExponential(3)+')':'-';"
    "if(o.net_hashrate!=null)$('f_nh').textContent=o.net_hashrate>0?fmt(o.net_hashrate)+'H/s':'-';"
    "if(o.net_hashrate>0&&o.mh>0){const sh=o.mh*1e6/o.net_hashrate;$('f_share').textContent=fmtPct(sh);}"
    "if(o.eta_block_s!=null)$('f_eta').textContent=fmtDur(o.eta_block_s);"
    "if(o.prob_block_24h!=null)$('f_p24').textContent=fmtPct(o.prob_block_24h);"
    "if(o.prob_block_per_day!=null){const py=1-Math.exp(-o.prob_block_per_day*365.25);$('f_py').textContent=fmtPct(py);$('f_bpy').textContent=(o.prob_block_per_day*365.25).toExponential(3);}"
    "if(o.job!=null&&o.mh!=null)conn.textContent=(!o.job&&o.mh===0)?'waiting for stratum job':'live';"
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
    char buf[STATUS_JSON_CAP];
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
        char line[STATUS_JSON_CAP];
        int len = snprintf(line, sizeof(line), "data: ");
        len += build_stats_json(line + len, sizeof(line) - len, false);
        len += snprintf(line + len, sizeof(line) - len, "\n\n");
        if (write_all(fd, line, len) < 0) break;
    }
out:
    close(fd);
    return NULL;
}

static bool http_path_is(const char *path, const char *want)
{
    size_t n = strlen(want);
    return strncmp(path, want, n) == 0 && (path[n] == '\0' || path[n] == '?');
}

static bool http_path_is_index(const char *path)
{
    return http_path_is(path, "/") ||
           http_path_is(path, "/index") ||
           http_path_is(path, "/index.html");
}

static void http_send_body(int fd, const char *status, const char *content_type,
                           const char *body, size_t body_len, bool send_body)
{
    char hdr[512];
    int hlen = snprintf(hdr, sizeof(hdr),
        "HTTP/1.1 %s\r\n"
        "Content-Type: %s\r\n"
        "Content-Length: %zu\r\n"
        "Cache-Control: no-store\r\n"
        "Connection: close\r\n"
        "\r\n",
        status, content_type, body_len);
    write_all(fd, hdr, (size_t)hlen);
    if (send_body && body_len > 0) write_all(fd, body, body_len);
}

static void http_send_empty(int fd, const char *status)
{
    char hdr[256];
    int hlen = snprintf(hdr, sizeof(hdr),
        "HTTP/1.1 %s\r\n"
        "Content-Length: 0\r\n"
        "Connection: close\r\n"
        "\r\n", status);
    write_all(fd, hdr, (size_t)hlen);
}

static void *http_thread(void *unused) {
    (void)unused;
    int srv = socket(AF_INET, SOCK_STREAM, 0);
    if (srv < 0) { LOG("http: socket fail"); return NULL; }
    int yes = 1;
    setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes));
    struct sockaddr_in a = {0};
    char bind_address[INET_ADDRSTRLEN];
    char bind_reason[128] = "";
    if (gbtc_parse_http_bind(getenv("GBTC_HTTP_BIND"), &a.sin_addr,
                             bind_address, sizeof(bind_address),
                             bind_reason, sizeof(bind_reason)) != 0) {
        LOG("http: %s", bind_reason);
        close(srv);
        return NULL;
    }
    a.sin_family = AF_INET;
    a.sin_port = htons(41174);
    if (bind(srv, (struct sockaddr*)&a, sizeof(a)) < 0) {
        LOG("http: bind 41174 fail: %s", strerror(errno));
        close(srv); return NULL;
    }
    if (listen(srv, 16) < 0) { LOG("http: listen fail"); close(srv); return NULL; }
    LOG("http: listening on %s:41174", bind_address);
    while (!g_stop) {
        struct sockaddr_in peer = {0};
        socklen_t peer_len = sizeof(peer);
        int c = accept(srv, (struct sockaddr*)&peer, &peer_len);
        if (c < 0) continue;
        // Read request line + headers (small, single recv)
        char req[2048];
        ssize_t n = recv(c, req, sizeof(req) - 1, 0);
        if (n <= 0) { close(c); continue; }
        req[n] = 0;

        char peer_ip[INET_ADDRSTRLEN] = "unknown";
        inet_ntop(AF_INET, &peer.sin_addr, peer_ip, sizeof(peer_ip));

        char method[8] = {0};
        char path[256] = {0};
        if (sscanf(req, "%7s %255s", method, path) != 2) {
            LOG("http: malformed request from %s", peer_ip);
            http_send_empty(c, "400 Bad Request");
            close(c);
            continue;
        }

        bool is_get = strcmp(method, "GET") == 0;
        bool is_head = strcmp(method, "HEAD") == 0;
        if (strcmp(method, "OPTIONS") == 0) {
            const char *r =
                "HTTP/1.1 204 No Content\r\n"
                "Allow: GET, HEAD, OPTIONS\r\n"
                "Access-Control-Allow-Origin: *\r\n"
                "Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n"
                "Content-Length: 0\r\n"
                "Connection: close\r\n"
                "\r\n";
            write_all(c, r, strlen(r));
            close(c);
        } else if (is_get && http_path_is(path, "/events")) {
            pthread_t t;
            if (pthread_create(&t, NULL, sse_client_thread, (void*)(intptr_t)c) == 0) {
                pthread_detach(t);
            } else {
                close(c);
            }
        } else if ((is_get || is_head) && http_path_is(path, "/status.json")) {
            char body[STATUS_JSON_CAP];
            int blen = build_stats_json(body, sizeof(body), true);
            http_send_body(c, "200 OK", "application/json",
                           body, (size_t)blen, is_get);
            close(c);
        } else if ((is_get || is_head) && http_path_is_index(path)) {
            http_send_body(c, "200 OK", "text/html; charset=utf-8",
                           index_html, strlen(index_html), is_get);
            close(c);
        } else if ((is_get || is_head) && http_path_is(path, "/favicon.ico")) {
            http_send_empty(c, "204 No Content");
            close(c);
        } else if (!is_get && !is_head) {
            LOG("http: unsupported method from %s: %s %s", peer_ip, method, path);
            http_send_empty(c, "405 Method Not Allowed");
            close(c);
        } else {
            LOG("http: not found from %s: %s %s", peer_ip, method, path);
            http_send_empty(c, "404 Not Found");
            close(c);
        }
    }
    close(srv);
    return NULL;
}

static int env_flag_enabled(const char *name)
{
    const char *v = getenv(name);
    return v && (strcmp(v, "1") == 0 || strcmp(v, "true") == 0 ||
                 strcmp(v, "yes") == 0 || strcmp(v, "on") == 0);
}

static uint32_t parse_batch_nonces(uint32_t dispatch_quantum)
{
    char reason[256] = "";
    uint32_t value = 0;
    if (gbtc_parse_batch_nonces_value(getenv("GBTC_BATCH_NONCES"), dispatch_quantum,
                                      &value, reason, sizeof(reason)) != 0) {
        DIE("%s", reason);
    }
    return value;
}

static const gbtc_backend_t *backend_for_kind(gbtc_backend_kind_t kind)
{
    switch (kind) {
    case GBTC_BACKEND_GLES: return &gbtc_gles_backend;
    case GBTC_BACKEND_VULKAN: return &gbtc_vulkan_backend;
    case GBTC_BACKEND_OPENCL: return &gbtc_opencl_backend;
    }
    return NULL;
}

static void init_device_path_status(void)
{
    const char *device = getenv("GBTC_DEVICE");
    if (!device || !device[0]) device = "/dev/dri/renderD128";
    snprintf(g_device_path, sizeof(g_device_path), "%s", device);
}

static void set_selected_backend_status(const gbtc_backend_t *backend, const char *fallback_reason)
{
    snprintf(g_backend, sizeof(g_backend), "%s", backend ? backend->name : "unknown");
    snprintf(g_backend_api, sizeof(g_backend_api), "%s", backend ? backend->api : "none");
    if (backend && backend->kind == GBTC_BACKEND_OPENCL) {
        g_device_path[0] = '\0';
    }
    snprintf(g_fallback_reason, sizeof(g_fallback_reason), "%s", fallback_reason ? fallback_reason : "");
}

static void append_rejection(char *dst, size_t cap, const char *backend, const char *reason)
{
    size_t used = strlen(dst);
    if (used >= cap) return;
    snprintf(dst + used, cap - used, "%s%s: %s", used ? "; " : "", backend, reason);
}

static const gbtc_backend_t *select_backend_or_die(const char *requested)
{
    // GLES first: it is the mature, fastest-verified path on current hardware.
    // Explicit GBTC_BACKEND=opencl still selects the OpenCL backend directly.
    const gbtc_backend_t *priority[] = {
        &gbtc_gles_backend,
        &gbtc_opencl_backend,
        &gbtc_vulkan_backend,
    };
    char rejection_summary[512] = "";

    if (gbtc_backend_is_auto(requested)) {
        for (size_t i = 0; i < sizeof(priority) / sizeof(priority[0]); i++) {
            char reason[512] = "";
            const gbtc_backend_t *backend = priority[i];
            if (backend->probe(reason, sizeof(reason)) == 0) {
                LOG("backend probe: %s available (%s)", backend->name, reason);
                set_selected_backend_status(backend, rejection_summary);
                return backend;
            }
            LOG("backend probe: %s rejected (%s)", backend->name, reason);
            append_rejection(rejection_summary, sizeof(rejection_summary), backend->name, reason);
        }
        DIE("no usable GPU backend found (%s)", rejection_summary);
    }

    gbtc_backend_kind_t kind;
    if (gbtc_parse_backend_kind(requested, &kind) != 0) {
        DIE("GBTC_BACKEND must be auto, gles, vulkan, or opencl; this is a GPU-only miner and CPU mining is intentionally not implemented");
    }
    const gbtc_backend_t *backend = backend_for_kind(kind);
    char reason[512] = "";
    if (!backend || backend->probe(reason, sizeof(reason)) != 0) {
        DIE("requested backend %s is unavailable: %s", requested, reason);
    }
    set_selected_backend_status(backend, "");
    return backend;
}

static int print_probe_only(const char *requested)
{
    const gbtc_backend_t *priority[] = {
        &gbtc_gles_backend,
        &gbtc_opencl_backend,
        &gbtc_vulkan_backend,
    };
    init_device_path_status();
    if (!gbtc_backend_is_auto(requested)) {
        gbtc_backend_kind_t kind;
        if (gbtc_parse_backend_kind(requested, &kind) != 0) {
            fprintf(stderr, "GBTC_BACKEND must be auto, gles, vulkan, or opencl; this is a GPU-only miner and CPU mining is intentionally not implemented\n");
            return 2;
        }
        const gbtc_backend_t *backend = backend_for_kind(kind);
        char reason[512] = "";
        int ok = backend && backend->probe(reason, sizeof(reason)) == 0;
        printf("%-6s %s %s\n", backend->name, ok ? "available" : "unavailable", reason);
        return ok ? 0 : 2;
    }

    int any_ok = 0;
    for (size_t i = 0; i < sizeof(priority) / sizeof(priority[0]); i++) {
        char reason[512] = "";
        int ok = priority[i]->probe(reason, sizeof(reason)) == 0;
        if (ok) any_ok = 1;
        printf("%-6s %s %s\n", priority[i]->name, ok ? "available" : "unavailable", reason);
    }
    return any_ok ? 0 : 2;
}

typedef struct {
    int ok;
    uint32_t local_size;
    uint32_t batch_nonces;
    double seconds;
    uint64_t hashes;
    double mh_s;
    char compile_result[512];
    char error[256];
} bench_summary_t;

static double monotonic_seconds(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
}

static void json_print_string(const char *s)
{
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)(s ? s : ""); *p; p++) {
        switch (*p) {
        case '"': printf("\\\""); break;
        case '\\': printf("\\\\"); break;
        case '\b': printf("\\b"); break;
        case '\f': printf("\\f"); break;
        case '\n': printf("\\n"); break;
        case '\r': printf("\\r"); break;
        case '\t': printf("\\t"); break;
        default:
            if (*p < 0x20) printf("\\u%04x", *p);
            else putchar(*p);
            break;
        }
    }
    putchar('"');
}

static uint32_t active_dispatch_quantum(const gbtc_backend_t *backend)
{
    if (backend && backend->kind == GBTC_BACKEND_GLES) {
        return gbtc_gles_batch_quantum(&gles_config);
    }
    if (backend && backend->kind == GBTC_BACKEND_OPENCL) {
        return gbtc_opencl_active_local_size();
    }
    return 64u;
}

static const char *active_gles_kernel_name(const gbtc_backend_t *backend)
{
    if (backend && backend->kind == GBTC_BACKEND_GLES) {
        return gbtc_gles_kernel_name(gles_config.kernel);
    }
    if (backend && backend->kind == GBTC_BACKEND_OPENCL) {
        return gbtc_opencl_kernel_name();
    }
    return "";
}

static uint32_t active_gles_local_size(const gbtc_backend_t *backend)
{
    if (backend && backend->kind == GBTC_BACKEND_GLES) return gles_config.local_size;
    if (backend && backend->kind == GBTC_BACKEND_OPENCL) return gbtc_opencl_active_local_size();
    return 0;
}

static int run_benchmark_loop(const gbtc_backend_t *backend, const char *compile_result,
                              bench_summary_t *summary)
{
    memset(summary, 0, sizeof(*summary));
    summary->ok = 1;
    summary->local_size = active_gles_local_size(backend);
    snprintf(summary->compile_result, sizeof(summary->compile_result), "%s",
             compile_result ? compile_result : "");

    char reason[256] = "";
    int64_t bench_seconds = gbtc_parse_seconds_default(getenv("GBTC_BENCH_SECONDS"), 60,
                                                       "GBTC_BENCH_SECONDS", reason, sizeof(reason));
    if (bench_seconds < 0) DIE("%s", reason);
    int64_t warmup_seconds = gbtc_parse_seconds_default(getenv("GBTC_BENCH_WARMUP_SECONDS"), 5,
                                                        "GBTC_BENCH_WARMUP_SECONDS", reason, sizeof(reason));
    if (warmup_seconds < 0) DIE("%s", reason);

    uint32_t quantum = active_dispatch_quantum(backend);
    uint32_t batch_nonces = parse_batch_nonces(quantum);
    summary->batch_nonces = batch_nonces;

    gbtc_work_batch_t work = {0};
    gbtc_make_synthetic_work(&work, batch_nonces);

    double warmup_deadline = monotonic_seconds() + (double)warmup_seconds;
    while (monotonic_seconds() < warmup_deadline) {
        gbtc_backend_result_t result = {0};
        if (backend->run_batch(&work, &result) != 0) {
            summary->ok = 0;
            snprintf(summary->error, sizeof(summary->error), "warmup batch failed");
            return -1;
        }
        work.nonce_base += batch_nonces;
    }

    double start = monotonic_seconds();
    double deadline = start + (double)bench_seconds;
    uint64_t hashes = 0;
    while (monotonic_seconds() < deadline) {
        gbtc_backend_result_t result = {0};
        if (backend->run_batch(&work, &result) != 0) {
            summary->ok = 0;
            snprintf(summary->error, sizeof(summary->error), "timed batch failed");
            break;
        }
        hashes += result.hashes_done;
        work.nonce_base += batch_nonces;
    }
    double end = monotonic_seconds();
    summary->seconds = end - start;
    summary->hashes = hashes;
    summary->mh_s = summary->seconds > 0 ? (double)hashes / 1e6 / summary->seconds : 0;
    return summary->ok ? 0 : -1;
}

static void print_benchmark_json(const gbtc_backend_t *backend, const bench_summary_t *summary)
{
    printf("{\"ok\":%s,\"backend\":", summary->ok ? "true" : "false");
    json_print_string(backend ? backend->name : "unknown");
    printf(",\"backend_api\":");
    json_print_string(backend ? backend->api : "none");
    printf(",\"device_path\":");
    json_print_string(g_device_path);
    printf(",\"device_vendor\":");
    json_print_string(g_device_vendor);
    printf(",\"device_name\":");
    json_print_string(g_device_name);
    printf(",\"driver_name\":");
    json_print_string(g_driver_name);
    printf(",\"kernel\":");
    json_print_string(active_gles_kernel_name(backend));
    printf(",\"local_size\":%u", summary->local_size);
    printf(",\"batch_nonces\":%u", summary->batch_nonces);
    printf(",\"seconds\":%.6f", summary->seconds);
    printf(",\"hashes\":%" PRIu64, summary->hashes);
    printf(",\"mh_s\":%.6f", summary->mh_s);
    printf(",\"compile_result\":");
    json_print_string(summary->compile_result);
    if (!summary->ok) {
        printf(",\"error\":");
        json_print_string(summary->error);
    }
    printf("}\n");
    fflush(stdout);
}

static int run_benchmark_and_exit(const gbtc_backend_t *backend, const char *compile_result)
{
    bench_summary_t summary;
    int rc = run_benchmark_loop(backend, compile_result, &summary);
    print_benchmark_json(backend, &summary);
    backend->shutdown();
    return rc == 0 ? 0 : 2;
}

static void print_autotune_rank_json(int rank, const gbtc_backend_t *backend,
                                     const bench_summary_t *summary)
{
    printf("{\"type\":\"autotune_rank\",\"rank\":%d,\"backend\":", rank);
    json_print_string(backend ? backend->name : "unknown");
    printf(",\"kernel\":");
    json_print_string(active_gles_kernel_name(backend));
    printf(",\"local_size\":%u,\"mh_s\":%.6f,\"seconds\":%.6f,\"hashes\":%" PRIu64 ",\"ok\":%s}\n",
           summary->local_size, summary->mh_s, summary->seconds, summary->hashes,
           summary->ok ? "true" : "false");
}

static int run_gles_autotune_and_exit(const gbtc_backend_t *backend)
{
    if (!backend || backend->kind != GBTC_BACKEND_GLES) {
        DIE("GBTC_GLES_AUTOTUNE requires the gles backend");
    }

    static const uint32_t local_sizes[] = {8, 16, 32, 64, 128, 256};
    bench_summary_t summaries[sizeof(local_sizes) / sizeof(local_sizes[0])];
    int any_ok = 0;

    const char *old_local = getenv("GBTC_GLES_LOCAL_SIZE");
    char old_local_copy[32] = "";
    int had_old_local = old_local && old_local[0];
    if (had_old_local) snprintf(old_local_copy, sizeof(old_local_copy), "%s", old_local);

    for (size_t i = 0; i < sizeof(local_sizes) / sizeof(local_sizes[0]); i++) {
        char local_buf[16];
        snprintf(local_buf, sizeof(local_buf), "%u", local_sizes[i]);
        setenv("GBTC_GLES_LOCAL_SIZE", local_buf, 1);

        char init_reason[512] = "";
        memset(&summaries[i], 0, sizeof(summaries[i]));
        summaries[i].local_size = local_sizes[i];
        if (backend->init(init_reason, sizeof(init_reason)) != 0) {
            summaries[i].ok = 0;
            snprintf(summaries[i].compile_result, sizeof(summaries[i].compile_result), "%s", init_reason);
            snprintf(summaries[i].error, sizeof(summaries[i].error), "backend init failed");
            print_benchmark_json(backend, &summaries[i]);
            continue;
        }

        run_benchmark_loop(backend, init_reason, &summaries[i]);
        print_benchmark_json(backend, &summaries[i]);
        if (summaries[i].ok) any_ok = 1;
        backend->shutdown();
    }

    if (had_old_local) setenv("GBTC_GLES_LOCAL_SIZE", old_local_copy, 1);
    else unsetenv("GBTC_GLES_LOCAL_SIZE");

    for (size_t i = 0; i < sizeof(summaries) / sizeof(summaries[0]); i++) {
        for (size_t j = i + 1; j < sizeof(summaries) / sizeof(summaries[0]); j++) {
            if ((!summaries[i].ok && summaries[j].ok) ||
                (summaries[i].ok == summaries[j].ok && summaries[j].mh_s > summaries[i].mh_s)) {
                bench_summary_t tmp = summaries[i];
                summaries[i] = summaries[j];
                summaries[j] = tmp;
            }
        }
    }
    for (size_t i = 0; i < sizeof(summaries) / sizeof(summaries[0]); i++) {
        print_autotune_rank_json((int)i + 1, backend, &summaries[i]);
    }
    fflush(stdout);
    return any_ok ? 0 : 2;
}

int main(void) {
    signal(SIGINT, on_sigint);
    signal(SIGTERM, on_sigint);
    const char *backend_req = getenv("GBTC_BACKEND");
    if (!backend_req || !backend_req[0]) backend_req = "auto";
    init_device_path_status();
    if (env_flag_enabled("GBTC_PROBE_ONLY")) return print_probe_only(backend_req);

    int bench_only = env_flag_enabled("GBTC_BENCH_ONLY");
    int gles_autotune = env_flag_enabled("GBTC_GLES_AUTOTUNE");
    if (gles_autotune && !bench_only) {
        DIE("GBTC_GLES_AUTOTUNE requires GBTC_BENCH_ONLY=1");
    }

    const gbtc_backend_t *backend = select_backend_or_die(backend_req);
    if (bench_only && gles_autotune) {
        return run_gles_autotune_and_exit(backend);
    }

    char backend_reason[512] = "";
    if (backend->init(backend_reason, sizeof(backend_reason)) != 0) {
        DIE("backend %s init failed: %s", backend->name, backend_reason);
    }
    LOG("selected backend=%s api=%s %s", backend->name, backend->api, backend_reason);
    if (backend->kind == GBTC_BACKEND_OPENCL) {
        const char *vendor = NULL, *name = NULL, *version = NULL;
        gbtc_opencl_device_strings(&vendor, &name, &version);
        snprintf(g_device_vendor, sizeof(g_device_vendor), "%s", vendor);
        snprintf(g_device_name, sizeof(g_device_name), "%s", name);
        snprintf(g_driver_name, sizeof(g_driver_name), "%s", version);
    }

    if (bench_only) {
        return run_benchmark_and_exit(backend, backend_reason);
    }

    const char *btc = getenv("BTC_ADDRESS");
    const char *url = getenv("POOL_URL");
    const char *worker = getenv("WORKER_NAME");
    if (!btc) DIE("BTC_ADDRESS env not set");
    if (!url) url = "stratum+tcp://public-pool.io:3333";
    if (!worker) worker = "x";
    LOG("addr=%s pool=%s worker=%s", btc, url, worker);

    g_start_time = time(NULL);
    snprintf(g_pool_url, sizeof(g_pool_url), "%s", url);
    snprintf(g_worker, sizeof(g_worker), "%s", worker);
    snprintf(g_btc_addr, sizeof(g_btc_addr), "%s", btc);

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
    uint64_t en2_counter = 0;
    const uint32_t BATCH_NONCES = parse_batch_nonces(active_dispatch_quantum(backend));

    while (!g_stop) {
        // Drain incoming stratum messages (non-blocking-ish)
        stratum_pump(&S, 0);
        if (!S.have_job) { stratum_pump(&S, 1000); continue; }

        // Build current work: pick fresh extranonce2
        uint8_t en2[8] = {0};
        size_t en2_len = S.extranonce2_size;
        for (size_t i = 0; i < en2_len; i++) {
            en2[i] = (uint8_t)(en2_counter >> (8u * i));
        }
        en2_counter++;

        uint8_t merkle_root[32];
        compute_merkle_root(merkle_root, &S, en2, en2_len);

        gbtc_work_batch_t work = {0};
        build_header_midstate(work.midstate, work.tail3, &S, merkle_root);
        memcpy(work.target, S.target, sizeof(work.target));

        char en2_hex[32];
        bin_to_hex(en2_hex, en2, en2_len);
        char job_id_snapshot[64];
        snprintf(job_id_snapshot, sizeof(job_id_snapshot), "%s", S.job_id);
        uint32_t ntime_snapshot = S.ntime_be;
        uint64_t target_generation_snapshot = S.target_generation;

        // Sweep full 2^32 nonce range until new job arrives.
        // (1<<32) / BATCH_NONCES = 2^32 / 2^24 = 256 batches per ntime cycle
        uint32_t base = 0;
        for (int batch = 0; batch < (int)((1ull<<32) / BATCH_NONCES); batch++, base += BATCH_NONCES) {
            if (g_stop) break;
            work.nonce_base = base;
            work.nonce_count = BATCH_NONCES;
            gbtc_backend_result_t result = {0};
            if (backend->run_batch(&work, &result) != 0) {
                LOG("backend %s batch failed", backend->name);
                break;
            }

            for (uint32_t i = 0; i < result.count; i++) {
                uint8_t candidate_hash[32];
                gbtc_work_hash(candidate_hash, &work, result.nonces[i]);
                if (gbtc_raw_hash_meets_target(candidate_hash, work.target)) {
                    stratum_submit(&S, job_id_snapshot, en2_hex,
                                   ntime_snapshot, result.nonces[i]);
                } else {
                    LOG("candidate nonce=%08x failed full share-target comparison",
                        result.nonces[i]);
                }
            }

            total_hashes += result.hashes_done;
            // Per-batch instantaneous MH/s — fed to metrics timer thread (pushes 1Hz)
            struct timespec t_batch_now;
            clock_gettime(CLOCK_MONOTONIC, &t_batch_now);
            double batch_secs = (t_batch_now.tv_sec - t_batch_prev.tv_sec) +
                                (t_batch_now.tv_nsec - t_batch_prev.tv_nsec) / 1e9;
            t_batch_prev = t_batch_now;
            double inst = batch_secs > 0 ? (double)result.hashes_done / 1e6 / batch_secs : 0;
            pthread_mutex_lock(&m_mu);
            g_lifetime_hashes += result.hashes_done;
            g_inst_mh = inst;
            pthread_mutex_unlock(&m_mu);
            time_t now = time(NULL);
            if (now - t_last >= 10) {
                double secs = (double)(now - t_last);
                double mh = (double)total_hashes / 1e6 / secs;
                LOG("hashrate ~%.2f MH/s (batch=%u, found=%u)", mh, BATCH_NONCES, result.count);
                total_hashes = 0;
                t_last = now;
            }

            // Check for new job mid-sweep
            stratum_pump(&S, 0);
            if (S.target_generation != target_generation_snapshot) {
                LOG("difficulty changed, abandoning sweep");
                break;
            }
            if (strcmp(S.job_id, job_id_snapshot) != 0 && S.clean) {
                LOG("new job, abandoning sweep");
                break;
            }
        }
    }

    LOG("shutting down");
    backend->shutdown();
    return 0;
}
