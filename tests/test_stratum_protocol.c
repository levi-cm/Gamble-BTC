#include "stratum_protocol.h"

#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static int failures;

static int hex_value(char c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static void decode_hex(uint8_t *out, const char *hex, size_t bytes)
{
    for (size_t i = 0; i < bytes; i++) {
        out[i] = (uint8_t)((hex_value(hex[2u * i]) << 4) | hex_value(hex[2u * i + 1u]));
    }
}

static void expect_true(const char *name, bool value)
{
    if (!value) {
        fprintf(stderr, "%s: expected true\n", name);
        failures++;
    }
}

static void expect_false(const char *name, bool value)
{
    if (value) {
        fprintf(stderr, "%s: expected false\n", name);
        failures++;
    }
}

static void expect_ok(const char *name, int result, const char *reason)
{
    if (result != 0) {
        fprintf(stderr, "%s: expected success: %s\n", name, reason);
        failures++;
    }
}

static void expect_rejected(const char *name, int result)
{
    if (result == 0) {
        fprintf(stderr, "%s: malformed input accepted\n", name);
        failures++;
    }
}

static void expect_target(const char *name, const uint32_t got[8], const uint32_t want[8])
{
    if (memcmp(got, want, 8 * sizeof(*got)) != 0) {
        fprintf(stderr, "%s: target mismatch\n", name);
        failures++;
    }
}

static void test_difficulty_targets_and_full_compare(void)
{
    static const uint32_t diff1[8] = {
        0x00000000u, 0xffff0000u, 0, 0, 0, 0, 0, 0,
    };
    static const uint32_t diff2[8] = {
        0x00000000u, 0x7fff8000u, 0, 0, 0, 0, 0, 0,
    };
    static const uint32_t diff3[8] = {
        0x00000000u, 0x55550000u, 0, 0, 0, 0, 0, 0,
    };
    uint32_t target[8];
    char reason[256] = "";

    expect_ok("difficulty-1", gbtc_target_from_difficulty(1.0, target, reason, sizeof(reason)), reason);
    expect_target("difficulty-1-target", target, diff1);
    expect_ok("difficulty-2", gbtc_target_from_difficulty(2.0, target, reason, sizeof(reason)), reason);
    expect_target("difficulty-2-target", target, diff2);
    expect_ok("difficulty-3", gbtc_target_from_difficulty(3.0, target, reason, sizeof(reason)), reason);
    expect_target("difficulty-3-target", target, diff3);
    expect_rejected("zero-difficulty", gbtc_target_from_difficulty(0.0, target, reason, sizeof(reason)));
    expect_rejected("nan-difficulty", gbtc_target_from_difficulty(NAN, target, reason, sizeof(reason)));
    expect_rejected("huge-difficulty", gbtc_target_from_difficulty(1e100, target, reason, sizeof(reason)));

    uint8_t equal_hash[32];
    uint8_t lower_hash[32];
    uint8_t higher_hash[32];
    uint32_t lower_value[8];
    uint32_t higher_value[8];
    gbtc_target_to_raw_hash(diff1, equal_hash);
    memcpy(lower_value, diff1, sizeof(lower_value));
    memcpy(higher_value, diff1, sizeof(higher_value));
    lower_value[1]--;
    higher_value[1]++;
    gbtc_target_to_raw_hash(lower_value, lower_hash);
    gbtc_target_to_raw_hash(higher_value, higher_hash);
    expect_true("equal-hash-accepted", gbtc_raw_hash_meets_target(equal_hash, diff1));
    expect_true("lower-hash-accepted", gbtc_raw_hash_meets_target(lower_hash, diff1));
    expect_false("higher-hash-rejected", gbtc_raw_hash_meets_target(higher_hash, diff1));
    uint8_t genesis_hash[32];
    decode_hex(genesis_hash,
               "6fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000",
               sizeof(genesis_hash));
    expect_true("genesis-hash-meets-diff1", gbtc_raw_hash_meets_target(genesis_hash, diff1));
}

static void test_subscribe_validation(void)
{
    const char *valid =
        "{\"id\":1,\"result\":[[[\"mining.notify\",\"token\"]],\"a1b2c3d4\",4],\"error\":null}";
    gbtc_stratum_subscription_t sub = {0};
    char reason[256] = "";
    expect_ok("valid-subscribe",
              gbtc_parse_subscribe_response(valid, 1, &sub, reason, sizeof(reason)), reason);
    expect_true("subscribe-extranonce1", strcmp(sub.extranonce1, "a1b2c3d4") == 0);
    expect_true("subscribe-extranonce1-len", sub.extranonce1_len == 4);
    expect_true("subscribe-extranonce2-size", sub.extranonce2_size == 4);

    expect_rejected("oversized-extranonce2",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],\"aa\",9],\"error\":null}",
                        1, &sub, reason, sizeof(reason)));
    expect_rejected("negative-extranonce2",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],\"aa\",-1],\"error\":null}",
                        1, &sub, reason, sizeof(reason)));
    expect_rejected("fractional-extranonce2",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],\"aa\",2.5],\"error\":null}",
                        1, &sub, reason, sizeof(reason)));
    expect_rejected("non-string-extranonce1",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],42,4],\"error\":null}",
                        1, &sub, reason, sizeof(reason)));
    expect_rejected("odd-extranonce1",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],\"abc\",4],\"error\":null}",
                        1, &sub, reason, sizeof(reason)));
    expect_rejected("wrong-subscribe-id",
                    gbtc_parse_subscribe_response(valid, 2, &sub, reason, sizeof(reason)));
    expect_rejected("trailing-json",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],\"aa\",4]} garbage",
                        1, &sub, reason, sizeof(reason)));
    expect_rejected("oversized-extranonce1",
                    gbtc_parse_subscribe_response(
                        "{\"id\":1,\"result\":[[],\"0000000000000000000000000000000000\",4]}",
                        1, &sub, reason, sizeof(reason)));
}

static void test_auth_and_share_responses(void)
{
    bool accepted = false;
    char reason[256] = "";
    expect_ok("auth-accepted",
              gbtc_parse_boolean_response(
                  "{\"id\":2,\"result\":true,\"error\":null}",
                  2, &accepted, reason, sizeof(reason)), reason);
    expect_true("auth-accepted-value", accepted);

    expect_ok("auth-denied",
              gbtc_parse_boolean_response(
                  "{\"id\":2,\"result\":false,\"error\":[24,\"unauthorized\",null]}",
                  2, &accepted, reason, sizeof(reason)), reason);
    expect_false("auth-denied-value", accepted);

    expect_ok("share-accepted",
              gbtc_parse_boolean_response(
                  "{\"id\":17,\"result\":true,\"error\":null}",
                  17, &accepted, reason, sizeof(reason)), reason);
    expect_true("share-accepted-value", accepted);
    expect_ok("share-rejected",
              gbtc_parse_boolean_response(
                  "{\"id\":18,\"result\":false,\"error\":[23,\"low difficulty\",null]}",
                  18, &accepted, reason, sizeof(reason)), reason);
    expect_false("share-rejected-value", accepted);

    expect_rejected("numeric-auth-result",
                    gbtc_parse_boolean_response(
                        "{\"id\":2,\"result\":1,\"error\":null}",
                        2, &accepted, reason, sizeof(reason)));
    // The boolean result is authoritative; pools attach varying error
    // payloads to accepts. A true result counts as accepted.
    expect_ok("true-with-error",
              gbtc_parse_boolean_response(
                  "{\"id\":2,\"result\":true,\"error\":[1,\"bad\",null]}",
                  2, &accepted, reason, sizeof(reason)), reason);
    expect_true("true-with-error-value", accepted);
    expect_ok("true-with-empty-error",
              gbtc_parse_boolean_response(
                  "{\"id\":2,\"result\":true,\"error\":[]}",
                  2, &accepted, reason, sizeof(reason)), reason);
    expect_true("true-with-empty-error-value", accepted);
    expect_rejected("wrong-response-id",
                    gbtc_parse_boolean_response(
                        "{\"id\":3,\"result\":true,\"error\":null}",
                        2, &accepted, reason, sizeof(reason)));
}

typedef struct {
    const char *const *lines;
    size_t line_count;
    size_t next_line;
    size_t notification_count;
    char notification_methods[2][32];
} stratum_response_fixture_t;

static const char *stratum_fixture_read_line(void *context)
{
    stratum_response_fixture_t *fixture = context;
    if (fixture->next_line >= fixture->line_count) return NULL;
    return fixture->lines[fixture->next_line++];
}

static void stratum_fixture_handle_notification(void *context, const char *json,
                                                const char *method)
{
    stratum_response_fixture_t *fixture = context;
    (void)json;
    if (fixture->notification_count < 2u) {
        snprintf(fixture->notification_methods[fixture->notification_count],
                 sizeof(fixture->notification_methods[0]), "%s", method);
    }
    fixture->notification_count++;
}

static void test_auth_wait_skips_notifications(void)
{
    static const char *const lines[] = {
        "{\"id\":null,\"method\":\"mining.set_difficulty\",\"params\":[4096]}",
        "{\"method\":\"mining.notify\",\"params\":["
        "\"job-1\","
        "\"0000000000000000000000000000000000000000000000000000000000000000\","
        "\"0102\",\"0304\",[],\"20000000\",\"1d00ffff\",\"495fab29\",true]}",
        "{\"id\":2,\"result\":true,\"error\":null}",
    };
    stratum_response_fixture_t fixture = {
        .lines = lines,
        .line_count = sizeof(lines) / sizeof(lines[0]),
    };
    bool accepted = false;
    char reason[256] = "";

    expect_ok("auth-after-notifications",
              gbtc_stratum_wait_boolean_response(
                  2, stratum_fixture_read_line, stratum_fixture_handle_notification,
                  &fixture, &accepted, reason, sizeof(reason)), reason);
    expect_true("auth-after-notifications-accepted", accepted);
    expect_true("auth-notifications-dispatched", fixture.notification_count == 2u);
    expect_true("auth-first-notification",
                strcmp(fixture.notification_methods[0], "mining.set_difficulty") == 0);
    expect_true("auth-second-notification",
                strcmp(fixture.notification_methods[1], "mining.notify") == 0);
}

static void test_subscribe_wait_skips_notifications(void)
{
    static const char *const lines[] = {
        "{\"id\":null,\"method\":\"mining.set_difficulty\",\"params\":[4096]}",
        "{\"method\":\"mining.notify\",\"params\":["
        "\"job-1\","
        "\"0000000000000000000000000000000000000000000000000000000000000000\","
        "\"0102\",\"0304\",[],\"20000000\",\"1d00ffff\",\"495fab29\",true]}",
        "{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}",
    };
    stratum_response_fixture_t fixture = {
        .lines = lines,
        .line_count = sizeof(lines) / sizeof(lines[0]),
    };
    gbtc_stratum_subscription_t subscription = {0};
    char reason[256] = "";

    expect_ok("subscribe-after-notifications",
              gbtc_stratum_wait_subscribe_response(
                  1, stratum_fixture_read_line,
                  stratum_fixture_handle_notification, &fixture,
                  &subscription, reason, sizeof(reason)), reason);
    expect_true("subscribe-after-notifications-extranonce",
                strcmp(subscription.extranonce1, "a1b2c3d4") == 0);
    expect_true("subscribe-notifications-dispatched",
                fixture.notification_count == 2u);
    expect_true("subscribe-first-notification",
                strcmp(fixture.notification_methods[0], "mining.set_difficulty") == 0);
    expect_true("subscribe-second-notification",
                strcmp(fixture.notification_methods[1], "mining.notify") == 0);
}

static void test_difficulty_and_notify_messages(void)
{
    char reason[256] = "";
    double difficulty = 0;
    uint32_t target[8];
    expect_ok("variable-difficulty",
              gbtc_parse_set_difficulty(
                  "{\"id\":null,\"method\":\"mining.set_difficulty\",\"params\":[4096]}",
                  &difficulty, target, reason, sizeof(reason)), reason);
    expect_true("variable-difficulty-value", difficulty == 4096.0);
    expect_rejected("difficulty-string",
                    gbtc_parse_set_difficulty(
                        "{\"method\":\"mining.set_difficulty\",\"params\":[\"4096\"]}",
                        &difficulty, target, reason, sizeof(reason)));
    expect_rejected("difficulty-array-missing",
                    gbtc_parse_set_difficulty(
                        "{\"method\":\"mining.set_difficulty\",\"params\":null}",
                        &difficulty, target, reason, sizeof(reason)));
    expect_rejected("difficulty-extra-param",
                    gbtc_parse_set_difficulty(
                        "{\"method\":\"mining.set_difficulty\",\"params\":[4096,1]}",
                        &difficulty, target, reason, sizeof(reason)));

    const char *notify =
        "{\"method\":\"mining.notify\",\"params\":["
        "\"job-1\","
        "\"0000000000000000000000000000000000000000000000000000000000000000\","
        "\"0102\",\"0304\",[],\"20000000\",\"1d00ffff\",\"495fab29\",true]}";
    gbtc_stratum_job_t job = {0};
    expect_ok("valid-notify", gbtc_parse_notify(notify, &job, reason, sizeof(reason)), reason);
    expect_true("notify-job-id", strcmp(job.job_id, "job-1") == 0);
    expect_true("notify-clean", job.clean);

    expect_rejected("notify-prevhash-type",
                    gbtc_parse_notify(
                        "{\"method\":\"mining.notify\",\"params\":[\"j\",7,\"00\",\"00\",[],\"20000000\",\"1d00ffff\",\"495fab29\",true]}",
                        &job, reason, sizeof(reason)));
    expect_rejected("notify-merkle-type",
                    gbtc_parse_notify(
                        "{\"method\":\"mining.notify\",\"params\":[\"j\",\"0000000000000000000000000000000000000000000000000000000000000000\",\"00\",\"00\",[9],\"20000000\",\"1d00ffff\",\"495fab29\",true]}",
                        &job, reason, sizeof(reason)));
    expect_rejected("notify-clean-type",
                    gbtc_parse_notify(
                        "{\"method\":\"mining.notify\",\"params\":[\"j\",\"0000000000000000000000000000000000000000000000000000000000000000\",\"00\",\"00\",[],\"20000000\",\"1d00ffff\",\"495fab29\",1]}",
                        &job, reason, sizeof(reason)));
    expect_rejected("notify-oversized-job-id",
                    gbtc_parse_notify(
                        "{\"method\":\"mining.notify\",\"params\":[\"0123456789012345678901234567890123456789012345678901234567890123\",\"0000000000000000000000000000000000000000000000000000000000000000\",\"00\",\"00\",[],\"20000000\",\"1d00ffff\",\"495fab29\",true]}",
                        &job, reason, sizeof(reason)));
    expect_rejected("notify-version-length",
                    gbtc_parse_notify(
                        "{\"method\":\"mining.notify\",\"params\":[\"j\",\"0000000000000000000000000000000000000000000000000000000000000000\",\"00\",\"00\",[],\"200000\",\"1d00ffff\",\"495fab29\",true]}",
                        &job, reason, sizeof(reason)));
}

static void test_json_escaping(void)
{
    char escaped[128];
    char reason[128] = "";
    expect_ok("json-escape",
              gbtc_json_escape("pool\"</script>\\\n\x01", escaped, sizeof(escaped),
                               reason, sizeof(reason)), reason);
    expect_true("json-escape-value",
                strcmp(escaped, "pool\\\"</script>\\\\\\n\\u0001") == 0);
    expect_rejected("json-escape-small-buffer",
                    gbtc_json_escape("abcdef", escaped, 4, reason, sizeof(reason)));
}

int main(void)
{
    test_difficulty_targets_and_full_compare();
    test_subscribe_validation();
    test_auth_and_share_responses();
    test_subscribe_wait_skips_notifications();
    test_auth_wait_skips_notifications();
    test_difficulty_and_notify_messages();
    test_json_escaping();
    return failures ? 1 : 0;
}
