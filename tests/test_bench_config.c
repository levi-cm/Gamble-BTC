#include "bench.h"

#include <stdint.h>
#include <stdio.h>

static int expect_i64(const char *name, int64_t got, int64_t want)
{
    if (got != want) {
        fprintf(stderr, "%s: got %lld want %lld\n",
                name, (long long)got, (long long)want);
        return 1;
    }
    return 0;
}

static int expect_u32(const char *name, uint32_t got, uint32_t want)
{
    if (got != want) {
        fprintf(stderr, "%s: got %u want %u\n", name, got, want);
        return 1;
    }
    return 0;
}

int main(void)
{
    int failures = 0;
    char reason[128] = "";
    uint32_t value = 0;

    failures += expect_i64("default-seconds",
                           gbtc_parse_seconds_default(NULL, 60, "GBTC_BENCH_SECONDS", reason, sizeof(reason)),
                           60);
    failures += expect_i64("valid-seconds",
                           gbtc_parse_seconds_default("15", 60, "GBTC_BENCH_SECONDS", reason, sizeof(reason)),
                           15);
    if (gbtc_parse_seconds_default("0", 60, "GBTC_BENCH_SECONDS", reason, sizeof(reason)) >= 0) {
        fprintf(stderr, "zero seconds unexpectedly accepted\n");
        failures++;
    }
    if (gbtc_parse_seconds_default("abc", 60, "GBTC_BENCH_SECONDS", reason, sizeof(reason)) >= 0) {
        fprintf(stderr, "non-numeric seconds unexpectedly accepted\n");
        failures++;
    }

    failures += expect_i64("default-warmup",
                           gbtc_parse_seconds_default(NULL, 5, "GBTC_BENCH_WARMUP_SECONDS", reason, sizeof(reason)),
                           5);

    if (gbtc_parse_batch_nonces_value(NULL, 64, &value, reason, sizeof(reason)) != 0) {
        fprintf(stderr, "default batch parse failed: %s\n", reason);
        failures++;
    } else {
        failures += expect_u32("default-batch", value, 1u << 24);
    }

    if (gbtc_parse_batch_nonces_value("1024", 64, &value, reason, sizeof(reason)) != 0) {
        fprintf(stderr, "valid batch parse failed: %s\n", reason);
        failures++;
    } else {
        failures += expect_u32("valid-batch", value, 1024);
    }

    if (gbtc_parse_batch_nonces_value("96", 64, &value, reason, sizeof(reason)) == 0) {
        fprintf(stderr, "non-power-of-two batch unexpectedly accepted\n");
        failures++;
    }
    if (gbtc_parse_batch_nonces_value("64", 128, &value, reason, sizeof(reason)) == 0) {
        fprintf(stderr, "batch below quantum unexpectedly accepted\n");
        failures++;
    }
    if (gbtc_parse_batch_nonces_value("bogus", 64, &value, reason, sizeof(reason)) == 0) {
        fprintf(stderr, "bogus batch unexpectedly accepted\n");
        failures++;
    }

    gbtc_work_batch_t work = {0};
    gbtc_make_synthetic_work(&work, 256);
    uint32_t found[8] = {0};
    uint32_t even_nonce = 42;
    uint32_t odd_nonce = 43;
    uint32_t even_word = gbtc_reference_final_word7(&work, even_nonce);
    uint32_t odd_word = gbtc_reference_final_word7(&work, odd_nonce);
    uint32_t even_count = gbtc_reference_scan_word7(&work, even_word, found, 8);
    int saw_even = 0;
    for (uint32_t i = 0; i < even_count && i < 8; i++) {
        if (found[i] == even_nonce) saw_even = 1;
    }
    if (!saw_even) {
        fprintf(stderr, "reference scan did not report even nonce %u\n", even_nonce);
        failures++;
    }

    uint32_t odd_count = gbtc_reference_scan_word7(&work, odd_word, found, 8);
    int saw_odd = 0;
    for (uint32_t i = 0; i < odd_count && i < 8; i++) {
        if (found[i] == odd_nonce) saw_odd = 1;
    }
    if (!saw_odd) {
        fprintf(stderr, "reference scan did not report odd nonce %u\n", odd_nonce);
        failures++;
    }

    return failures ? 1 : 0;
}
