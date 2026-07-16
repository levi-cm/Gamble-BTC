#include "http_config.h"

#include <arpa/inet.h>
#include <stdio.h>
#include <string.h>

static int expect_bind(const char *name, const char *value, const char *want)
{
    struct in_addr address;
    char display[INET_ADDRSTRLEN];
    char reason[128] = "";
    if (gbtc_parse_http_bind(value, &address, display, sizeof(display),
                             reason, sizeof(reason)) != 0) {
        fprintf(stderr, "%s: parse failed: %s\n", name, reason);
        return 1;
    }
    if (strcmp(display, want) != 0) {
        fprintf(stderr, "%s: got %s want %s\n", name, display, want);
        return 1;
    }
    return 0;
}

int main(void)
{
    int failures = 0;
    failures += expect_bind("safe-default", NULL, "127.0.0.1");
    failures += expect_bind("safe-empty-default", "", "127.0.0.1");
    failures += expect_bind("explicit-loopback", "127.0.0.1", "127.0.0.1");
    failures += expect_bind("lan-opt-in", "0.0.0.0", "0.0.0.0");
    failures += expect_bind("specific-lan", "192.168.10.5", "192.168.10.5");

    struct in_addr address;
    char display[INET_ADDRSTRLEN];
    char reason[128] = "";
    if (gbtc_parse_http_bind("localhost", &address, display, sizeof(display),
                             reason, sizeof(reason)) == 0) {
        fprintf(stderr, "hostname bind unexpectedly accepted\n");
        failures++;
    }
    if (gbtc_parse_http_bind("1.2.3.999", &address, display, sizeof(display),
                             reason, sizeof(reason)) == 0) {
        fprintf(stderr, "malformed IPv4 bind unexpectedly accepted\n");
        failures++;
    }
    return failures ? 1 : 0;
}
