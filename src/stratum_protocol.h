#ifndef GBTC_STRATUM_PROTOCOL_H
#define GBTC_STRATUM_PROTOCOL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define GBTC_MAX_EXTRANONCE1_HEX 32u
#define GBTC_MAX_EXTRANONCE2_SIZE 8u
#define GBTC_MAX_JOB_ID 63u
#define GBTC_MAX_COINBASE_HEX 1022u
#define GBTC_MAX_MERKLE_BRANCHES 32u

typedef struct {
    char extranonce1[GBTC_MAX_EXTRANONCE1_HEX + 1u];
    size_t extranonce1_len;
    size_t extranonce2_size;
} gbtc_stratum_subscription_t;

typedef struct {
    char job_id[GBTC_MAX_JOB_ID + 1u];
    uint8_t prevhash[32];
    char coinb1_hex[GBTC_MAX_COINBASE_HEX + 1u];
    char coinb2_hex[GBTC_MAX_COINBASE_HEX + 1u];
    size_t merkle_count;
    uint8_t merkle[GBTC_MAX_MERKLE_BRANCHES][32];
    uint32_t version_be;
    uint32_t nbits_be;
    uint32_t ntime_be;
    bool clean;
} gbtc_stratum_job_t;

typedef const char *(*gbtc_stratum_line_reader_fn)(void *context);
typedef void (*gbtc_stratum_notification_handler_fn)(void *context,
                                                      const char *json,
                                                      const char *method);

int gbtc_target_from_difficulty(double difficulty, uint32_t target[8],
                                char *reason, size_t reason_cap);
void gbtc_target_to_raw_hash(const uint32_t target[8], uint8_t raw_hash[32]);
bool gbtc_raw_hash_meets_target(const uint8_t raw_hash[32], const uint32_t target[8]);

int gbtc_parse_subscribe_response(const char *json, int expected_id,
                                  gbtc_stratum_subscription_t *subscription,
                                  char *reason, size_t reason_cap);
int gbtc_stratum_wait_subscribe_response(int expected_id,
                                         gbtc_stratum_line_reader_fn read_line,
                                         gbtc_stratum_notification_handler_fn handle_notification,
                                         void *context,
                                         gbtc_stratum_subscription_t *subscription,
                                         char *reason, size_t reason_cap);
int gbtc_parse_boolean_response(const char *json, int expected_id, bool *accepted,
                                char *reason, size_t reason_cap);
int gbtc_stratum_wait_boolean_response(int expected_id,
                                       gbtc_stratum_line_reader_fn read_line,
                                       gbtc_stratum_notification_handler_fn handle_notification,
                                       void *context, bool *accepted,
                                       char *reason, size_t reason_cap);
int gbtc_parse_set_difficulty(const char *json, double *difficulty, uint32_t target[8],
                              char *reason, size_t reason_cap);
int gbtc_parse_notify(const char *json, gbtc_stratum_job_t *job,
                      char *reason, size_t reason_cap);

int gbtc_json_escape(const char *input, char *output, size_t output_cap,
                     char *reason, size_t reason_cap);

#endif
