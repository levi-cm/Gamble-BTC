#include "backend.h"

#include <stdio.h>
#include <string.h>

static int expect_parse(const char *name, gbtc_backend_kind_t want)
{
    gbtc_backend_kind_t got = GBTC_BACKEND_GLES;
    if (gbtc_parse_backend_kind(name, &got) != 0) {
        fprintf(stderr, "failed to parse backend %s\n", name);
        return 1;
    }
    if (got != want) {
        fprintf(stderr, "backend %s parsed to %s\n", name, gbtc_backend_kind_name(got));
        return 1;
    }
    return 0;
}

int main(void)
{
    int failures = 0;

    failures += expect_parse("gles", GBTC_BACKEND_GLES);
    failures += expect_parse("vulkan", GBTC_BACKEND_VULKAN);
    failures += expect_parse("opencl", GBTC_BACKEND_OPENCL);
    failures += expect_parse("cuda", GBTC_BACKEND_CUDA);

    gbtc_backend_kind_t ignored = GBTC_BACKEND_GLES;
    if (gbtc_parse_backend_kind("cpu", &ignored) == 0) {
        fprintf(stderr, "cpu backend parsed successfully, but this miner is iGPU-only\n");
        failures++;
    }
    if (gbtc_parse_backend_kind("bogus", &ignored) == 0) {
        fprintf(stderr, "bogus backend parsed successfully\n");
        failures++;
    }
    if (strcmp(gbtc_backend_kind_name(GBTC_BACKEND_VULKAN), "vulkan") != 0) {
        fprintf(stderr, "vulkan kind name mismatch\n");
        failures++;
    }
    if (strcmp(gbtc_backend_kind_name(GBTC_BACKEND_CUDA), "cuda") != 0) {
        fprintf(stderr, "cuda kind name mismatch\n");
        failures++;
    }

    return failures ? 1 : 0;
}
