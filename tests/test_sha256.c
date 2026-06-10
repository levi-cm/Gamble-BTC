#include "sha256.h"

#include <stdint.h>
#include <stdio.h>
#include <string.h>

static int hexval(char c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static int hex_to_bin(uint8_t *out, const char *hex, size_t bytes)
{
    for (size_t i = 0; i < bytes; i++) {
        int hi = hexval(hex[2 * i]);
        int lo = hexval(hex[2 * i + 1]);
        if (hi < 0 || lo < 0) return -1;
        out[i] = (uint8_t)((hi << 4) | lo);
    }
    return 0;
}

static int expect_hash(const char *name, const uint8_t got[32], const char *want_hex)
{
    uint8_t want[32];
    if (hex_to_bin(want, want_hex, sizeof(want)) != 0) {
        fprintf(stderr, "%s: invalid expected hex\n", name);
        return 1;
    }
    if (memcmp(got, want, sizeof(want)) != 0) {
        fprintf(stderr, "%s: hash mismatch\n", name);
        return 1;
    }
    return 0;
}

int main(void)
{
    int failures = 0;
    uint8_t out[32];

    gbtc_sha256(out, (const uint8_t *)"", 0);
    failures += expect_hash("sha256-empty", out,
                            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934"
                            "ca495991b7852b855");

    gbtc_sha256(out, (const uint8_t *)"abc", 3);
    failures += expect_hash("sha256-abc", out,
                            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9"
                            "cb410ff61f20015ad");

    gbtc_sha256d(out, (const uint8_t *)"", 0);
    failures += expect_hash("sha256d-empty", out,
                            "5df6e0e2761359d30a8275058e299fcc0381534545f55cf4"
                            "3e41983f5d4c9456");

    gbtc_sha256d(out, (const uint8_t *)"abc", 3);
    failures += expect_hash("sha256d-abc", out,
                            "4f8b42c22dd3729b519ba6f68d2da7cc5b2d606d05daed"
                            "5ad5128cc03e6c6358");

    return failures ? 1 : 0;
}
