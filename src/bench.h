#ifndef GBTC_BENCH_H
#define GBTC_BENCH_H

#include "backend.h"

#include <stddef.h>
#include <stdint.h>

int64_t gbtc_parse_seconds_default(const char *value, int64_t default_value,
                                   const char *name, char *reason, size_t reason_cap);
int gbtc_parse_batch_nonces_value(const char *value, uint32_t quantum,
                                  uint32_t *out, char *reason, size_t reason_cap);
void gbtc_make_synthetic_work(gbtc_work_batch_t *work, uint32_t nonce_count);
void gbtc_work_hash(uint8_t hash[32], const gbtc_work_batch_t *work, uint32_t nonce);
uint32_t gbtc_reference_final_word7(const gbtc_work_batch_t *work, uint32_t nonce);
uint32_t gbtc_reference_scan_word7(const gbtc_work_batch_t *work, uint32_t target_word7,
                                   uint32_t *nonces, uint32_t max_nonces);

#endif
