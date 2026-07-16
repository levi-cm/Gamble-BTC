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

static void put_le32(uint8_t *dst, uint32_t value)
{
    dst[0] = (uint8_t)value;
    dst[1] = (uint8_t)(value >> 8);
    dst[2] = (uint8_t)(value >> 16);
    dst[3] = (uint8_t)(value >> 24);
}

static int expect_genesis_header(const char *name, uint32_t timestamp, uint32_t nonce,
                                 const char *want_raw_hash)
{
    uint8_t header[80] = {0};
    uint8_t merkle_root[32];
    uint8_t hash[32];
    if (hex_to_bin(merkle_root,
                   "3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a",
                   sizeof(merkle_root)) != 0) {
        fprintf(stderr, "%s: invalid merkle-root vector\n", name);
        return 1;
    }
    put_le32(header, 1u);
    memcpy(header + 36, merkle_root, sizeof(merkle_root));
    put_le32(header + 68, timestamp);
    put_le32(header + 72, 0x1d00ffffu);
    put_le32(header + 76, nonce);
    gbtc_sha256d(hash, header, sizeof(header));
    return expect_hash(name, hash, want_raw_hash);
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

    failures += expect_genesis_header(
        "bitcoin-mainnet-genesis-header", 1231006505u, 2083236893u,
        "6fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000");
    failures += expect_genesis_header(
        "bitcoin-testnet3-genesis-header", 1296688602u, 414098458u,
        "43497fd7f826957108f4a30fd9cec3aeba79972084e90ead01ea330900000000");

    return failures ? 1 : 0;
}
