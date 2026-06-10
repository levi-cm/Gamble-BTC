#include "bench.h"
#include "sha256.h"

#include <errno.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

int64_t gbtc_parse_seconds_default(const char *value, int64_t default_value,
                                   const char *name, char *reason, size_t reason_cap)
{
    if (!value || !value[0]) return default_value;

    char *end = NULL;
    errno = 0;
    long long parsed = strtoll(value, &end, 10);
    if (errno || !end || *end != '\0' || parsed <= 0 || parsed > 86400) {
        set_reason(reason, reason_cap, "%s must be an integer from 1 to 86400 seconds", name);
        return -1;
    }
    return (int64_t)parsed;
}

int gbtc_parse_batch_nonces_value(const char *value, uint32_t quantum,
                                  uint32_t *out, char *reason, size_t reason_cap)
{
    if (!out || quantum == 0) {
        set_reason(reason, reason_cap, "invalid batch parser arguments");
        return -1;
    }

    if (!value || !value[0]) {
        *out = 1u << 24;
    } else {
        char *end = NULL;
        errno = 0;
        unsigned long parsed = strtoul(value, &end, 10);
        if (errno || !end || *end != '\0' || parsed > (1ul << 30)) {
            set_reason(reason, reason_cap,
                       "GBTC_BATCH_NONCES must be a power-of-two value between %u and 1073741824",
                       quantum);
            return -1;
        }
        *out = (uint32_t)parsed;
    }

    if (*out < quantum || (*out & (*out - 1u)) != 0 || (*out % quantum) != 0) {
        set_reason(reason, reason_cap,
                   "GBTC_BATCH_NONCES must be a power of two and a multiple of %u",
                   quantum);
        return -1;
    }
    return 0;
}

void gbtc_make_synthetic_work(gbtc_work_batch_t *work, uint32_t nonce_count)
{
    uint8_t header[80];
    memset(work, 0, sizeof(*work));

    for (size_t i = 0; i < sizeof(header); i++) {
        header[i] = (uint8_t)((i * 37u + 11u) & 0xffu);
    }
    header[76] = 0;
    header[77] = 0;
    header[78] = 0;
    header[79] = 0;

    gbtc_sha256_init_state(work->midstate);
    gbtc_sha256_compress(work->midstate, header);
    for (int i = 0; i < 3; i++) {
        work->tail3[i] = ((uint32_t)header[64 + 4 * i] << 24) |
                         ((uint32_t)header[64 + 4 * i + 1] << 16) |
                         ((uint32_t)header[64 + 4 * i + 2] << 8) |
                         (uint32_t)header[64 + 4 * i + 3];
    }
    work->nonce_base = 0;
    work->nonce_count = nonce_count;
}

static void put_be32(uint8_t *dst, uint32_t value)
{
    dst[0] = (uint8_t)(value >> 24);
    dst[1] = (uint8_t)(value >> 16);
    dst[2] = (uint8_t)(value >> 8);
    dst[3] = (uint8_t)value;
}

uint32_t gbtc_reference_final_word7(const gbtc_work_batch_t *work, uint32_t nonce)
{
    uint8_t block[64] = {0};
    put_be32(block + 0, work->tail3[0]);
    put_be32(block + 4, work->tail3[1]);
    put_be32(block + 8, work->tail3[2]);
    put_be32(block + 12, nonce);
    put_be32(block + 16, 0x80000000u);
    put_be32(block + 60, 640u);

    uint32_t state[8];
    memcpy(state, work->midstate, sizeof(state));
    gbtc_sha256_compress(state, block);

    uint8_t first_hash[32];
    for (int i = 0; i < 8; i++) {
        put_be32(first_hash + 4 * i, state[i]);
    }

    uint8_t final_hash[32];
    gbtc_sha256(final_hash, first_hash, sizeof(first_hash));
    return ((uint32_t)final_hash[28] << 24) |
           ((uint32_t)final_hash[29] << 16) |
           ((uint32_t)final_hash[30] << 8) |
           (uint32_t)final_hash[31];
}

uint32_t gbtc_reference_scan_word7(const gbtc_work_batch_t *work, uint32_t target_word7,
                                   uint32_t *nonces, uint32_t max_nonces)
{
    uint32_t count = 0;
    for (uint32_t i = 0; i < work->nonce_count; i++) {
        uint32_t nonce = work->nonce_base + i;
        if (gbtc_reference_final_word7(work, nonce) == target_word7) {
            if (count < max_nonces) nonces[count] = nonce;
            count++;
        }
    }
    return count;
}
