#include "http_config.h"

#include <arpa/inet.h>
#include <stdarg.h>
#include <stdio.h>

static void set_reason(char *reason, size_t reason_cap, const char *fmt, ...)
{
    if (!reason || reason_cap == 0) return;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(reason, reason_cap, fmt, ap);
    va_end(ap);
}

int gbtc_parse_http_bind(const char *value, struct in_addr *address,
                         char *display, size_t display_cap,
                         char *reason, size_t reason_cap)
{
    if (!address || !display || display_cap < INET_ADDRSTRLEN) {
        set_reason(reason, reason_cap, "invalid HTTP bind parser arguments");
        return -1;
    }
    if (!value || !value[0]) value = "127.0.0.1";
    if (inet_pton(AF_INET, value, address) != 1 ||
        !inet_ntop(AF_INET, address, display, display_cap)) {
        set_reason(reason, reason_cap, "GBTC_HTTP_BIND must be a valid IPv4 address");
        return -1;
    }
    return 0;
}
