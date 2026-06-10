#include "gles_tuning.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int expect_int(const char *name, int got, int want)
{
    if (got != want) {
        fprintf(stderr, "%s: got %d want %d\n", name, got, want);
        return 1;
    }
    return 0;
}

static int expect_u32(const char *name, uint32_t got, uint32_t want)
{
    if (got != want) {
        fprintf(stderr, "%s: got %u want %u\n", name, got, want);
        return 1;
    }
    return 0;
}

static int expect_contains(const char *name, const char *haystack, const char *needle)
{
    if (!strstr(haystack, needle)) {
        fprintf(stderr, "%s: missing substring: %s\n", name, needle);
        return 1;
    }
    return 0;
}

static int expect_kernel_parse(const char *name, gbtc_gles_kernel_t want)
{
    gbtc_gles_kernel_t got = GBTC_GLES_KERNEL_UNROLLED;
    if (gbtc_gles_parse_kernel(name, &got) != 0) {
        fprintf(stderr, "%s: parse failed\n", name);
        return 1;
    }
    if (got != want) {
        fprintf(stderr, "%s: parsed to %s\n", name, gbtc_gles_kernel_name(got));
        return 1;
    }
    return 0;
}

static int test_parse_local_sizes(void)
{
    int failures = 0;
    uint32_t local_size = 0;
    int is_auto = 0;
    char reason[128] = "";

    failures += expect_int("default-local",
                           gbtc_gles_parse_local_size(NULL, &local_size, &is_auto, reason, sizeof(reason)),
                           0);
    failures += expect_u32("default-local-size", local_size, 64);
    failures += expect_int("default-local-auto", is_auto, 0);

    failures += expect_int("auto-local",
                           gbtc_gles_parse_local_size("auto", &local_size, &is_auto, reason, sizeof(reason)),
                           0);
    failures += expect_u32("auto-local-size", local_size, 64);
    failures += expect_int("auto-local-flag", is_auto, 1);

    failures += expect_int("valid-local",
                           gbtc_gles_parse_local_size("128", &local_size, &is_auto, reason, sizeof(reason)),
                           0);
    failures += expect_u32("valid-local-size", local_size, 128);
    failures += expect_int("valid-local-auto", is_auto, 0);

    if (gbtc_gles_parse_local_size("7", &local_size, &is_auto, reason, sizeof(reason)) == 0) {
        fprintf(stderr, "local size 7 unexpectedly accepted\n");
        failures++;
    }
    if (gbtc_gles_parse_local_size("512", &local_size, &is_auto, reason, sizeof(reason)) == 0) {
        fprintf(stderr, "local size 512 unexpectedly accepted\n");
        failures++;
    }
    if (gbtc_gles_parse_local_size("bogus", &local_size, &is_auto, reason, sizeof(reason)) == 0) {
        fprintf(stderr, "bogus local size unexpectedly accepted\n");
        failures++;
    }

    return failures;
}

static int test_parse_kernels(void)
{
    int failures = 0;

    failures += expect_kernel_parse("unrolled", GBTC_GLES_KERNEL_UNROLLED);
    failures += expect_kernel_parse("partial", GBTC_GLES_KERNEL_PARTIAL);
    failures += expect_kernel_parse("looped", GBTC_GLES_KERNEL_LOOPED);
    failures += expect_kernel_parse("altbool", GBTC_GLES_KERNEL_ALTBOOL);
    failures += expect_kernel_parse("dualnonce", GBTC_GLES_KERNEL_DUALNONCE);

    gbtc_gles_kernel_t ignored = GBTC_GLES_KERNEL_UNROLLED;
    if (gbtc_gles_parse_kernel("cpu", &ignored) == 0) {
        fprintf(stderr, "cpu parsed as a GLES kernel\n");
        failures++;
    }

    failures += expect_int("unrolled-nonces", (int)gbtc_gles_kernel_nonces_per_invocation(GBTC_GLES_KERNEL_UNROLLED), 1);
    failures += expect_int("dualnonce-nonces", (int)gbtc_gles_kernel_nonces_per_invocation(GBTC_GLES_KERNEL_DUALNONCE), 2);
    failures += expect_u32("unrolled-invocation-nonce",
                           gbtc_gles_invocation_nonce(GBTC_GLES_KERNEL_UNROLLED, 1000, 7, 0),
                           1007);
    failures += expect_u32("dualnonce-even-lane",
                           gbtc_gles_invocation_nonce(GBTC_GLES_KERNEL_DUALNONCE, 1000, 7, 0),
                           1014);
    failures += expect_u32("dualnonce-odd-lane",
                           gbtc_gles_invocation_nonce(GBTC_GLES_KERNEL_DUALNONCE, 1000, 7, 1),
                           1015);

    return failures;
}

static int test_build_kernel_sources(void)
{
    int failures = 0;
    char reason[256] = "";
    gbtc_gles_config_t cfg = {
        .kernel = GBTC_GLES_KERNEL_UNROLLED,
        .local_size = 128,
        .local_size_auto = 0,
    };

    char *src = gbtc_gles_build_kernel_src(&cfg, reason, sizeof(reason));
    if (!src) {
        fprintf(stderr, "unrolled source generation failed: %s\n", reason);
        return 1;
    }
    failures += expect_contains("unrolled-local-size", src, "layout(local_size_x = 128) in;");
    failures += expect_contains("unrolled-target", src, "final_h7 == 0u");
    free(src);

    cfg.kernel = GBTC_GLES_KERNEL_ALTBOOL;
    cfg.local_size = 32;
    src = gbtc_gles_build_kernel_src(&cfg, reason, sizeof(reason));
    if (!src) {
        fprintf(stderr, "altbool source generation failed: %s\n", reason);
        return 1;
    }
    failures += expect_contains("altbool-ch", src, "#define CH(x,y,z) ((z)^((x)&((y)^(z))))");
    failures += expect_contains("altbool-local-size", src, "layout(local_size_x = 32) in;");
    free(src);

    cfg.kernel = GBTC_GLES_KERNEL_DUALNONCE;
    cfg.local_size = 64;
    src = gbtc_gles_build_kernel_src(&cfg, reason, sizeof(reason));
    if (!src) {
        fprintf(stderr, "dualnonce source generation failed: %s\n", reason);
        return 1;
    }
    failures += expect_contains("dualnonce-first", src, "uint nonce0 = I.nonce_base + global_idx * 2u;");
    failures += expect_contains("dualnonce-second", src, "uint nonce1 = nonce0 + 1u;");
    failures += expect_contains("dualnonce-store-second", src, "O.nonces[idx] = nonce1;");
    free(src);

    return failures;
}

int main(void)
{
    int failures = 0;
    failures += test_parse_local_sizes();
    failures += test_parse_kernels();
    failures += test_build_kernel_sources();
    return failures ? 1 : 0;
}
