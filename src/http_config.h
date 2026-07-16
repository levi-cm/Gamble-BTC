#ifndef GBTC_HTTP_CONFIG_H
#define GBTC_HTTP_CONFIG_H

#include <netinet/in.h>
#include <stddef.h>

int gbtc_parse_http_bind(const char *value, struct in_addr *address,
                         char *display, size_t display_cap,
                         char *reason, size_t reason_cap);

#endif
