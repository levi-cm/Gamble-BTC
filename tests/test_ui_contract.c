#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char *read_file(const char *path)
{
    FILE *fp = fopen(path, "rb");
    if (!fp) return NULL;
    if (fseek(fp, 0, SEEK_END) != 0) {
        fclose(fp);
        return NULL;
    }
    long len = ftell(fp);
    if (len < 0) {
        fclose(fp);
        return NULL;
    }
    rewind(fp);
    char *buf = calloc((size_t)len + 1u, 1u);
    if (!buf) {
        fclose(fp);
        return NULL;
    }
    if (fread(buf, 1u, (size_t)len, fp) != (size_t)len) {
        free(buf);
        fclose(fp);
        return NULL;
    }
    fclose(fp);
    return buf;
}

static int expect_contains(const char *src, const char *needle)
{
    if (!strstr(src, needle)) {
        fprintf(stderr, "missing UI contract text: %s\n", needle);
        return 1;
    }
    return 0;
}

int main(void)
{
    int failures = 0;
    char *src = read_file("src/main.c");
    if (!src) {
        fprintf(stderr, "failed to read src/main.c\n");
        return 1;
    }

    if (strstr(src, "HD 4600 iGPU")) {
        fprintf(stderr, "UI title still hardcodes HD 4600\n");
        failures++;
    }
    failures += expect_contains(src, "id=rig_title");
    failures += expect_contains(src, "waiting for stratum job");
    failures += expect_contains(src, "o.job!=null");
    failures += expect_contains(src, "stratum+tcp://public-pool.io:3333");
    if (strstr(src, "stratum+tcp://public-pool.io:21496")) {
        fprintf(stderr, "runtime fallback still uses old public-pool port\n");
        failures++;
    }

    free(src);
    return failures ? 1 : 0;
}
