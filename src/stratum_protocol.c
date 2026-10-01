#include "stratum_protocol.h"

#include <cjson/cJSON.h>

#include <limits.h>
#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#define BIG_LIMBS 12u

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

static void shift_left(uint32_t limbs[BIG_LIMBS], unsigned bits)
{
    unsigned words = bits / 32u;
    unsigned rem = bits % 32u;
    if (words) {
        for (size_t i = BIG_LIMBS; i-- > 0;) {
            limbs[i] = i >= words ? limbs[i - words] : 0u;
        }
    }
    if (rem) {
        uint32_t carry = 0;
        for (size_t i = 0; i < BIG_LIMBS; i++) {
            uint32_t next = limbs[i] >> (32u - rem);
            limbs[i] = (limbs[i] << rem) | carry;
            carry = next;
        }
    }
}

static void shift_right(uint32_t limbs[BIG_LIMBS], unsigned bits)
{
    unsigned words = bits / 32u;
    unsigned rem = bits % 32u;
    if (words) {
        for (size_t i = 0; i < BIG_LIMBS; i++) {
            limbs[i] = i + words < BIG_LIMBS ? limbs[i + words] : 0u;
        }
    }
    if (rem) {
        uint32_t carry = 0;
        for (size_t i = BIG_LIMBS; i-- > 0;) {
            uint32_t next = limbs[i] << (32u - rem);
            limbs[i] = (limbs[i] >> rem) | carry;
            carry = next;
        }
    }
}

static void divide_u64(uint32_t limbs[BIG_LIMBS], uint64_t divisor)
{
    uint64_t remainder = 0;
    for (size_t i = BIG_LIMBS; i-- > 0;) {
        __uint128_t current = ((__uint128_t)remainder << 32u) | limbs[i];
        limbs[i] = (uint32_t)(current / divisor);
        remainder = (uint64_t)(current % divisor);
    }
}

int gbtc_target_from_difficulty(double difficulty, uint32_t target[8],
                                char *reason, size_t reason_cap)
{
    if (!target || !isfinite(difficulty) || difficulty < 1.0 || difficulty > 1e30) {
        set_reason(reason, reason_cap, "difficulty must be finite and from 1 to 1e30");
        return -1;
    }

    union {
        double value;
        uint64_t bits;
    } encoded = { .value = difficulty };
    unsigned exponent = (unsigned)((encoded.bits >> 52u) & 0x7ffu);
    uint64_t mantissa = (UINT64_C(1) << 52u) | (encoded.bits & UINT64_C(0x000fffffffffffff));
    int shift = 1075 - (int)exponent;

    uint32_t value[BIG_LIMBS] = {0};
    value[6] = 0xffff0000u;
    if (shift >= 0) shift_left(value, (unsigned)shift);
    divide_u64(value, mantissa);
    if (shift < 0) shift_right(value, (unsigned)-shift);

    for (size_t i = 8; i < BIG_LIMBS; i++) {
        if (value[i] != 0) {
            set_reason(reason, reason_cap, "difficulty produced an out-of-range target");
            return -1;
        }
    }
    bool nonzero = false;
    for (size_t i = 0; i < 8; i++) {
        target[7u - i] = value[i];
        nonzero = nonzero || value[i] != 0;
    }
    if (!nonzero) {
        set_reason(reason, reason_cap, "difficulty produced a zero target");
        return -1;
    }
    return 0;
}

void gbtc_target_to_raw_hash(const uint32_t target[8], uint8_t raw_hash[32])
{
    uint8_t display[32];
    for (size_t i = 0; i < 8; i++) {
        display[4u * i] = (uint8_t)(target[i] >> 24u);
        display[4u * i + 1u] = (uint8_t)(target[i] >> 16u);
        display[4u * i + 2u] = (uint8_t)(target[i] >> 8u);
        display[4u * i + 3u] = (uint8_t)target[i];
    }
    for (size_t i = 0; i < 32; i++) raw_hash[i] = display[31u - i];
}

bool gbtc_raw_hash_meets_target(const uint8_t raw_hash[32], const uint32_t target[8])
{
    if (!raw_hash || !target) return false;
    for (size_t i = 0; i < 8; i++) {
        size_t p = 31u - 4u * i;
        uint32_t word = ((uint32_t)raw_hash[p] << 24u) |
                        ((uint32_t)raw_hash[p - 1u] << 16u) |
                        ((uint32_t)raw_hash[p - 2u] << 8u) |
                        (uint32_t)raw_hash[p - 3u];
        if (word < target[i]) return true;
        if (word > target[i]) return false;
    }
    return true;
}

static cJSON *parse_root(const char *json, char *reason, size_t reason_cap)
{
    if (!json) {
        set_reason(reason, reason_cap, "missing JSON message");
        return NULL;
    }
    const char *end = NULL;
    cJSON *root = cJSON_ParseWithLengthOpts(json, strlen(json) + 1u, &end, 1);
    if (!root || !cJSON_IsObject(root)) {
        cJSON_Delete(root);
        set_reason(reason, reason_cap, "message must be one complete JSON object");
        return NULL;
    }
    return root;
}

static int json_integer(const cJSON *item, int minimum, int maximum, int *out)
{
    if (!cJSON_IsNumber(item) || !isfinite(item->valuedouble) ||
        floor(item->valuedouble) != item->valuedouble ||
        item->valuedouble < minimum || item->valuedouble > maximum) {
        return -1;
    }
    *out = (int)item->valuedouble;
    return 0;
}

static int response_id(const cJSON *root, int expected_id)
{
    int id = 0;
    return json_integer(cJSON_GetObjectItemCaseSensitive(root, "id"), 0, INT_MAX, &id) == 0 &&
           id == expected_id ? 0 : -1;
}

static int hex_value(char c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static int valid_hex(const char *hex, size_t min_len, size_t max_len)
{
    if (!hex) return -1;
    size_t len = strlen(hex);
    if (len < min_len || len > max_len || (len & 1u)) return -1;
    for (size_t i = 0; i < len; i++) {
        if (hex_value(hex[i]) < 0) return -1;
    }
    return 0;
}

static int decode_hex_exact(uint8_t *out, const char *hex, size_t bytes)
{
    if (valid_hex(hex, bytes * 2u, bytes * 2u) != 0) return -1;
    for (size_t i = 0; i < bytes; i++) {
        out[i] = (uint8_t)((hex_value(hex[2u * i]) << 4u) | hex_value(hex[2u * i + 1u]));
    }
    return 0;
}

static int null_error(const cJSON *root)
{
    cJSON *error = cJSON_GetObjectItemCaseSensitive(root, "error");
    return !error || cJSON_IsNull(error);
}

int gbtc_parse_subscribe_response(const char *json, int expected_id,
                                  gbtc_stratum_subscription_t *subscription,
                                  char *reason, size_t reason_cap)
{
    if (!subscription) return -1;
    cJSON *root = parse_root(json, reason, reason_cap);
    if (!root) return -1;
    cJSON *result = cJSON_GetObjectItemCaseSensitive(root, "result");
    cJSON *en1 = cJSON_IsArray(result) ? cJSON_GetArrayItem(result, 1) : NULL;
    cJSON *en2 = cJSON_IsArray(result) ? cJSON_GetArrayItem(result, 2) : NULL;
    int en2_size = 0;
    gbtc_stratum_subscription_t parsed = {0};
    if (response_id(root, expected_id) != 0 || !null_error(root) ||
        !cJSON_IsArray(result) || cJSON_GetArraySize(result) < 3 ||
        !cJSON_IsString(en1) || !en1->valuestring ||
        valid_hex(en1->valuestring, 2u, GBTC_MAX_EXTRANONCE1_HEX) != 0 ||
        json_integer(en2, 1, (int)GBTC_MAX_EXTRANONCE2_SIZE, &en2_size) != 0) {
        cJSON_Delete(root);
        set_reason(reason, reason_cap, "invalid mining.subscribe response");
        return -1;
    }
    snprintf(parsed.extranonce1, sizeof(parsed.extranonce1), "%s", en1->valuestring);
    parsed.extranonce1_len = strlen(parsed.extranonce1) / 2u;
    parsed.extranonce2_size = (size_t)en2_size;
    *subscription = parsed;
    cJSON_Delete(root);
    return 0;
}

int gbtc_parse_boolean_response(const char *json, int expected_id, bool *accepted,
                                char *reason, size_t reason_cap)
{
    if (!accepted) return -1;
    cJSON *root = parse_root(json, reason, reason_cap);
    if (!root) return -1;
    cJSON *result = cJSON_GetObjectItemCaseSensitive(root, "result");
    if (response_id(root, expected_id) != 0 || !cJSON_IsBool(result) ||
        (cJSON_IsTrue(result) && !null_error(root))) {
        cJSON_Delete(root);
        set_reason(reason, reason_cap, "invalid boolean Stratum response");
        return -1;
    }
    *accepted = cJSON_IsTrue(result);
    cJSON_Delete(root);
    return 0;
}

static int stratum_wait_response_line(gbtc_stratum_line_reader_fn read_line,
                                      gbtc_stratum_notification_handler_fn handle_notification,
                                      void *context, const char **response_json,
                                      char *reason, size_t reason_cap)
{
    if (!read_line || !handle_notification || !response_json) {
        set_reason(reason, reason_cap, "missing Stratum response callback or output");
        return -1;
    }

    for (;;) {
        const char *line = read_line(context);
        if (!line) {
            set_reason(reason, reason_cap, "timeout or connection closed");
            return -1;
        }

        cJSON *root = parse_root(line, reason, reason_cap);
        if (!root) return -1;

        cJSON *method = cJSON_GetObjectItemCaseSensitive(root, "method");
        if (cJSON_IsString(method) && method->valuestring) {
            handle_notification(context, line, method->valuestring);
            cJSON_Delete(root);
            continue;
        }

        cJSON_Delete(root);
        *response_json = line;
        return 0;
    }
}

int gbtc_stratum_wait_subscribe_response(int expected_id,
                                         gbtc_stratum_line_reader_fn read_line,
                                         gbtc_stratum_notification_handler_fn handle_notification,
                                         void *context,
                                         gbtc_stratum_subscription_t *subscription,
                                         char *reason, size_t reason_cap)
{
    const char *response_json = NULL;
    if (!subscription ||
        stratum_wait_response_line(read_line, handle_notification, context,
                                   &response_json, reason, reason_cap) != 0) {
        if (!subscription) set_reason(reason, reason_cap, "missing Stratum subscription output");
        return -1;
    }
    return gbtc_parse_subscribe_response(response_json, expected_id, subscription,
                                         reason, reason_cap);
}

int gbtc_stratum_wait_boolean_response(int expected_id,
                                       gbtc_stratum_line_reader_fn read_line,
                                       gbtc_stratum_notification_handler_fn handle_notification,
                                       void *context, bool *accepted,
                                       char *reason, size_t reason_cap)
{
    const char *response_json = NULL;
    if (!accepted ||
        stratum_wait_response_line(read_line, handle_notification, context,
                                   &response_json, reason, reason_cap) != 0) {
        if (!accepted) set_reason(reason, reason_cap, "missing Stratum authorization output");
        return -1;
    }
    return gbtc_parse_boolean_response(response_json, expected_id, accepted,
                                       reason, reason_cap);
}

static int method_params(cJSON *root, const char *method_name, cJSON **params,
                         char *reason, size_t reason_cap)
{
    cJSON *method = cJSON_GetObjectItemCaseSensitive(root, "method");
    *params = cJSON_GetObjectItemCaseSensitive(root, "params");
    if (!cJSON_IsString(method) || !method->valuestring ||
        strcmp(method->valuestring, method_name) != 0 || !cJSON_IsArray(*params)) {
        set_reason(reason, reason_cap, "invalid %s notification", method_name);
        return -1;
    }
    return 0;
}

int gbtc_parse_set_difficulty(const char *json, double *difficulty, uint32_t target[8],
                              char *reason, size_t reason_cap)
{
    if (!difficulty || !target) return -1;
    cJSON *root = parse_root(json, reason, reason_cap);
    if (!root) return -1;
    cJSON *params = NULL;
    cJSON *item = NULL;
    if (method_params(root, "mining.set_difficulty", &params, reason, reason_cap) == 0) {
        item = cJSON_GetArrayItem(params, 0);
    }
    if (!item || cJSON_GetArraySize(params) != 1 || !cJSON_IsNumber(item) ||
        gbtc_target_from_difficulty(item->valuedouble, target, reason, reason_cap) != 0) {
        cJSON_Delete(root);
        if (!reason || !reason[0]) set_reason(reason, reason_cap, "invalid mining.set_difficulty value");
        return -1;
    }
    *difficulty = item->valuedouble;
    cJSON_Delete(root);
    return 0;
}

static uint32_t read_be32(const uint8_t bytes[4])
{
    return ((uint32_t)bytes[0] << 24u) | ((uint32_t)bytes[1] << 16u) |
           ((uint32_t)bytes[2] << 8u) | (uint32_t)bytes[3];
}

int gbtc_parse_notify(const char *json, gbtc_stratum_job_t *job,
                      char *reason, size_t reason_cap)
{
    if (!job) return -1;
    cJSON *root = parse_root(json, reason, reason_cap);
    if (!root) return -1;
    cJSON *params = NULL;
    if (method_params(root, "mining.notify", &params, reason, reason_cap) != 0 ||
        cJSON_GetArraySize(params) != 9) {
        cJSON_Delete(root);
        return -1;
    }
    cJSON *jid = cJSON_GetArrayItem(params, 0);
    cJSON *prev = cJSON_GetArrayItem(params, 1);
    cJSON *cb1 = cJSON_GetArrayItem(params, 2);
    cJSON *cb2 = cJSON_GetArrayItem(params, 3);
    cJSON *branches = cJSON_GetArrayItem(params, 4);
    cJSON *version = cJSON_GetArrayItem(params, 5);
    cJSON *nbits = cJSON_GetArrayItem(params, 6);
    cJSON *ntime = cJSON_GetArrayItem(params, 7);
    cJSON *clean = cJSON_GetArrayItem(params, 8);
    gbtc_stratum_job_t parsed = {0};
    uint8_t number[4];

    if (!cJSON_IsString(jid) || !jid->valuestring || strlen(jid->valuestring) > GBTC_MAX_JOB_ID ||
        !cJSON_IsString(prev) || decode_hex_exact(parsed.prevhash, prev->valuestring, 32u) != 0 ||
        !cJSON_IsString(cb1) || valid_hex(cb1->valuestring, 0u, GBTC_MAX_COINBASE_HEX) != 0 ||
        !cJSON_IsString(cb2) || valid_hex(cb2->valuestring, 0u, GBTC_MAX_COINBASE_HEX) != 0 ||
        !cJSON_IsArray(branches) || cJSON_GetArraySize(branches) > (int)GBTC_MAX_MERKLE_BRANCHES ||
        !cJSON_IsString(version) || decode_hex_exact(number, version->valuestring, 4u) != 0 ||
        !cJSON_IsString(nbits) || !cJSON_IsString(ntime) || !cJSON_IsBool(clean)) {
        cJSON_Delete(root);
        set_reason(reason, reason_cap, "invalid mining.notify fields");
        return -1;
    }
    parsed.version_be = read_be32(number);
    if (decode_hex_exact(number, nbits->valuestring, 4u) != 0) goto invalid;
    parsed.nbits_be = read_be32(number);
    if (decode_hex_exact(number, ntime->valuestring, 4u) != 0) goto invalid;
    parsed.ntime_be = read_be32(number);
    parsed.merkle_count = (size_t)cJSON_GetArraySize(branches);
    for (size_t i = 0; i < parsed.merkle_count; i++) {
        cJSON *branch = cJSON_GetArrayItem(branches, (int)i);
        if (!cJSON_IsString(branch) || decode_hex_exact(parsed.merkle[i], branch->valuestring, 32u) != 0) {
            goto invalid;
        }
    }
    snprintf(parsed.job_id, sizeof(parsed.job_id), "%s", jid->valuestring);
    snprintf(parsed.coinb1_hex, sizeof(parsed.coinb1_hex), "%s", cb1->valuestring);
    snprintf(parsed.coinb2_hex, sizeof(parsed.coinb2_hex), "%s", cb2->valuestring);
    parsed.clean = cJSON_IsTrue(clean);
    *job = parsed;
    cJSON_Delete(root);
    return 0;

invalid:
    cJSON_Delete(root);
    set_reason(reason, reason_cap, "invalid mining.notify hex field");
    return -1;
}

int gbtc_json_escape(const char *input, char *output, size_t output_cap,
                     char *reason, size_t reason_cap)
{
    static const char hex[] = "0123456789abcdef";
    if (!input || !output || output_cap == 0) return -1;
    size_t used = 0;
    for (const unsigned char *p = (const unsigned char *)input; *p; p++) {
        char encoded[6];
        const char *piece = encoded;
        size_t piece_len = 0;
        switch (*p) {
        case '"': piece = "\\\""; piece_len = 2; break;
        case '\\': piece = "\\\\"; piece_len = 2; break;
        case '\b': piece = "\\b"; piece_len = 2; break;
        case '\f': piece = "\\f"; piece_len = 2; break;
        case '\n': piece = "\\n"; piece_len = 2; break;
        case '\r': piece = "\\r"; piece_len = 2; break;
        case '\t': piece = "\\t"; piece_len = 2; break;
        default:
            if (*p < 0x20u || *p >= 0x80u) {
                encoded[0] = '\\'; encoded[1] = 'u'; encoded[2] = '0'; encoded[3] = '0';
                encoded[4] = hex[*p >> 4u]; encoded[5] = hex[*p & 0x0fu];
                piece_len = sizeof(encoded);
            } else {
                encoded[0] = (char)*p;
                piece_len = 1;
            }
        }
        if (used + piece_len >= output_cap) {
            output[0] = '\0';
            set_reason(reason, reason_cap, "escaped JSON string exceeds output buffer");
            return -1;
        }
        memcpy(output + used, piece, piece_len);
        used += piece_len;
    }
    output[used] = '\0';
    return 0;
}
