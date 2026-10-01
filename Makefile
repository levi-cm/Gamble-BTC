CC ?= gcc
CFLAGS ?= -O3 -march=native -mtune=native -flto -Wall -Wextra -std=c11
CPPFLAGS ?= -Isrc
LDLIBS ?= -lEGL -lGLESv2 -lcjson -lpthread -lm
TEST_CFLAGS ?= -O2 -Wall -Wextra -Wpedantic -std=c11
TEST_LDFLAGS ?=

SRC := src/main.c src/sha256.c src/backend.c src/gles_tuning.c src/opencl.c src/cuda.c src/bench.c src/stratum_protocol.c src/http_config.c
OBJ := $(SRC:src/%.c=build/%.o)
TEST_BINS := build/test_sha256 build/test_backend_selection build/test_gles_tuning build/test_bench_config build/test_ui_contract build/test_stratum_protocol build/test_http_config build/test_opencl_conformance build/test_cuda_conformance build/test_overflow_recovery

.PHONY: all test test-scripts test-asan test-ubsan static-analysis clean

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

# GPU-gated: skips cleanly when no OpenCL GPU is present.
build/test_opencl_conformance: tests/test_opencl_conformance.c tests/gpu_conformance_common.h src/opencl.c src/opencl.h src/bench.c src/bench.h src/backend.h src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/opencl.c src/bench.c src/sha256.c -o $@ $(TEST_LDFLAGS)

# GPU-gated: skips cleanly when no CUDA device is present.
build/test_cuda_conformance: tests/test_cuda_conformance.c tests/gpu_conformance_common.h src/cuda.c src/cuda.h src/bench.c src/bench.h src/backend.h src/backend.c src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/cuda.c src/backend.c src/bench.c src/sha256.c -o $@ $(TEST_LDFLAGS)

# GPU-gated: skips cleanly when no GPU backend is present.
build/test_overflow_recovery: tests/test_overflow_recovery.c tests/gpu_conformance_common.h src/cuda.c src/cuda.h src/opencl.c src/opencl.h src/bench.c src/bench.h src/backend.h src/backend.c src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(TEST_CFLAGS) $< src/cuda.c src/opencl.c src/backend.c src/bench.c src/sha256.c -o $@ $(TEST_LDFLAGS)

test: $(TEST_BINS) test-scripts
	./build/test_sha256
	./build/test_backend_selection
	./build/test_gles_tuning
	./build/test_bench_config
	./build/test_ui_contract
	./build/test_stratum_protocol
	./build/test_http_config
	./build/test_opencl_conformance
	./build/test_cuda_conformance
	./build/test_overflow_recovery

test-asan:
	$(MAKE) clean
	$(MAKE) test TEST_CFLAGS="-O1 -g -Wall -Wextra -Wpedantic -std=c11 -fsanitize=address -fno-omit-frame-pointer" TEST_LDFLAGS="-fsanitize=address"

test-ubsan:
	$(MAKE) clean
	$(MAKE) test TEST_CFLAGS="-O1 -g -Wall -Wextra -Wpedantic -std=c11 -fsanitize=undefined -fno-omit-frame-pointer" TEST_LDFLAGS="-fsanitize=undefined"

static-analysis:
	cppcheck --enable=warning,performance,portability --check-level=exhaustive --error-exitcode=1 \
		--inline-suppr --suppress=missingIncludeSystem src tests

test-scripts:
	./tests/test_start_miner_script.sh

clean:
	rm -rf build miner
