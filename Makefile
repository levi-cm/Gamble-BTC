CC ?= gcc
CFLAGS ?= -O3 -march=native -mtune=native -Wall -Wextra -std=c11
CPPFLAGS ?= -Isrc
LDLIBS ?= -lEGL -lGLESv2 -lcjson -lpthread -lm

SRC := src/main.c src/sha256.c src/backend.c src/gles_tuning.c src/bench.c
OBJ := $(SRC:src/%.c=build/%.o)
TEST_BINS := build/test_sha256 build/test_backend_selection build/test_gles_tuning build/test_bench_config

.PHONY: all test clean

all: miner

miner: $(OBJ)
	$(CC) $(CFLAGS) $(OBJ) -o $@ $(LDLIBS)

build/%.o: src/%.c
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) $(CFLAGS) -c $< -o $@

build/test_sha256: tests/test_sha256.c src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) -Wall -Wextra -std=c11 $< src/sha256.c -o $@

build/test_backend_selection: tests/test_backend_selection.c src/backend.c src/backend.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) -Wall -Wextra -std=c11 $< src/backend.c -o $@

build/test_gles_tuning: tests/test_gles_tuning.c src/gles_tuning.c src/gles_tuning.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) -Wall -Wextra -std=c11 $< src/gles_tuning.c -o $@

build/test_bench_config: tests/test_bench_config.c src/bench.c src/bench.h src/backend.h src/sha256.c src/sha256.h
	@mkdir -p $(@D)
	$(CC) $(CPPFLAGS) -Wall -Wextra -std=c11 $< src/bench.c src/sha256.c -o $@

test: $(TEST_BINS)
	./build/test_sha256
	./build/test_backend_selection
	./build/test_gles_tuning
	./build/test_bench_config

clean:
	rm -rf build miner
