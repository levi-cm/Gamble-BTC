#ifndef GBTC_SHA256_H
#define GBTC_SHA256_H

#include <stddef.h>
#include <stdint.h>

void gbtc_sha256_init_state(uint32_t state[8]);
void gbtc_sha256_compress(uint32_t state[8], const uint8_t block[64]);
void gbtc_sha256(uint8_t out[32], const uint8_t *data, size_t len);
void gbtc_sha256d(uint8_t out[32], const uint8_t *data, size_t len);

#endif
