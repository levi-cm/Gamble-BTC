CC ?= gcc
CFLAGS ?= -O3 -march=native -mtune=native -Wall -Wextra -std=c11
CPPFLAGS ?= -Isrc
LDLIBS ?= -lEGL -lGLESv2 -lcjson -lpthread -lm
TEST_CFLAGS ?= -O2 -Wall -Wextra -Wpedantic -std=c11
TEST_LDFLAGS ?=

SRC := src/main.c src/sha256.c src/backend.c src/gles_tuning.c src/bench.c src/stratum_protocol.c src/http_config.c
OBJ := $(SRC:src/%.c=build/%.o)
TEST_BINS := build/test_sha256 build/test_backend_selection build/test_gles_tuning build/test_bench_config build/test_ui_contract build/test_stratum_protocol build/test_http_config

.PHONY: all test test-asan test-ubsan static-analysis clean

all: miner

miner: $(OBJ)
	$(CC) $(CFLAGS) $(OBJ) -o $@ $(LDLIBS)

build/%.o: src/%.c
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(CFLAGS) -c $< -o $@

build/test_sha256: tests/test_sha256.c src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/sha256.c -o $@ $(TEST_LDFLAGS)

build/test_backend_selection: tests/test_backend_selection.c src/backend.c src/backend.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/backend.c -o $@ $(TEST_LDFLAGS)

build/test_gles_tuning: tests/test_gles_tuning.c src/gles_tuning.c src/gles_tuning.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/gles_tuning.c -o $@ $(TEST_LDFLAGS)

build/test_bench_config: tests/test_bench_config.c src/bench.c src/bench.h src/backend.h src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/bench.c src/sha256.c -o $@ $(TEST_LDFLAGS)

build/test_ui_contract: tests/test_ui_contract.c src/main.c
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< -o $@ $(TEST_LDFLAGS)

build/test_stratum_protocol: tests/test_stratum_protocol.c src/stratum_protocol.c src/stratum_protocol.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/stratum_protocol.c -o $@ $(TEST_LDFLAGS) -lcjson -lm

build/test_http_config: tests/test_http_config.c src/http_config.c src/http_config.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/http_config.c -o $@ $(TEST_LDFLAGS)

test: $(TEST_BINS)
	./build/test_sha256
	./build/test_backend_selection
	./build/test_gles_tuning
	./build/test_bench_config
	./build/test_ui_contract
	./build/test_stratum_protocol
	./build/test_http_config

test-asan:
	$(MAKE) clean
	$(MAKE) test TEST_CFLAGS="-O1 -g -Wall -Wextra -Wpedantic -std=c11 -fsanitize=address -fno-omit-frame-pointer" TEST_LDFLAGS="-fsanitize=address"

test-ubsan:
	$(MAKE) clean
	$(MAKE) test TEST_CFLAGS="-O1 -g -Wall -Wextra -Wpedantic -std=c11 -fsanitize=undefined -fno-omit-frame-pointer" TEST_LDFLAGS="-fsanitize=undefined"

static-analysis:
	cppcheck --enable=warning,performance,portability --check-level=exhaustive --error-exitcode=1 \
		--inline-suppr --suppress=missingIncludeSystem src tests

clean:
	rm -rf build miner
