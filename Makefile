TARGET := riscv64-unknown-elf
CC := $(TARGET)-gcc
LD := $(TARGET)-gcc
OBJCOPY := $(TARGET)-objcopy
CFLAGS := -fPIC -O3 -fvisibility=hidden -fno-builtin-memcmp -I deps/ckb-c-stdlib -I deps/ckb-c-stdlib/molecule -I c -Wall -Werror -Wno-nonnull -Wno-nonnull-compare -Wno-unused-function -g
LDFLAGS := -Wl,-static -fdata-sections -ffunction-sections -Wl,--gc-sections

ENVIRONMENT := debug

SIMULATOR_CC := gcc
SIMULATOR_CLANG := clang
SIMULATOR_LIB := deps/simulator/target/release/libckb_x64_simulator.a
SIMULATOR_CFLAGS := -fno-builtin-printf -fno-builtin-memcmp -I deps/ckb-c-stdlib -I deps/ckb-c-stdlib/molecule -Wall -Werror -Wno-nonnull -Wno-unused-function -g -DCKB_STDLIB_NO_SYSCALL_IMPL -DBLAKE2_REF_C
SIMULATOR_COVERAGE_CFLAGS := -fprofile-arcs -ftest-coverage -Wno-nonnull-compare
SIMULATOR_UNDEFINED_CFLAGS := -fsanitize=undefined -fsanitize=implicit-conversion -fsanitize=local-bounds -fsanitize=unsigned-integer-overflow -fsanitize=nullability
SIMULATOR_ADDRESS_CFLAGS := -fsanitize=address
SIMULATOR_LDFLAGS := -lpthread -ldl

# docker pull nervos/ckb-riscv-gnu-toolchain:bionic-20190702
BUILDER_DOCKER := nervos/ckb-riscv-gnu-toolchain@sha256:7b168b4b109a0f741078a71b7c4dddaf1d283a5244608f7851f5714fbad273ba

all: build/$(ENVIRONMENT)/poa build/$(ENVIRONMENT)/state

all-via-docker:
	mkdir -p build/$(ENVIRONMENT)
	docker run --rm -v `pwd`:/code ${BUILDER_DOCKER} bash -c "cd /code && make"

simulators: build/$(ENVIRONMENT)/poa_sim build/$(ENVIRONMENT)/state_sim

test: all simulators
	cd tests && cargo test
	scripts/run_sim_tests.sh $(ENVIRONMENT)

coverage: test
	mkdir -p build/coverage
	gcovr -r . -e deps --html --html-details -o build/coverage/coverage.html -s

build/$(ENVIRONMENT)/poa: c/poa.c
	mkdir -p build/$(ENVIRONMENT)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $<
	$(OBJCOPY) --strip-debug --strip-all $@ $@.strip

build/$(ENVIRONMENT)/state: c/state.c
	mkdir -p build/$(ENVIRONMENT)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $<
	$(OBJCOPY) --strip-debug --strip-all $@ $@.strip

build/$(ENVIRONMENT)/poa_sim: c/poa.c ${SIMULATOR_LIB}
	mkdir -p build/$(ENVIRONMENT)
	$(SIMULATOR_CC) $(SIMULATOR_CFLAGS) $(SIMULATOR_COVERAGE_CFLAGS) -o $@ $^ $(SIMULATOR_LDFLAGS)
	$(SIMULATOR_CLANG) $(SIMULATOR_CFLAGS) $(SIMULATOR_UNDEFINED_CFLAGS) -o $@.ubsan $^ $(SIMULATOR_LDFLAGS)
	$(SIMULATOR_CLANG) $(SIMULATOR_CFLAGS) $(SIMULATOR_ADDRESS_CFLAGS) -o $@.asan $^ $(SIMULATOR_LDFLAGS)

build/$(ENVIRONMENT)/state_sim: c/state.c ${SIMULATOR_LIB}
	mkdir -p build/$(ENVIRONMENT)
	$(SIMULATOR_CC) $(SIMULATOR_CFLAGS) $(SIMULATOR_COVERAGE_CFLAGS) -o $@ $^ $(SIMULATOR_LDFLAGS)
	$(SIMULATOR_CLANG) $(SIMULATOR_CFLAGS) $(SIMULATOR_UNDEFINED_CFLAGS) -o $@.ubsan $^ $(SIMULATOR_LDFLAGS)
	$(SIMULATOR_CLANG) $(SIMULATOR_CFLAGS) $(SIMULATOR_ADDRESS_CFLAGS) -o $@.asan $^ $(SIMULATOR_LDFLAGS)

${SIMULATOR_LIB}:
	cd deps/simulator && cargo build --release

fmt:
	clang-format -i -style=Google $(wildcard c/*.h c/*.c)
	cd tests; cargo fmt --all
	git diff --exit-code $(wildcard c/*.h c/*.c)

clean:
	rm -rf build/$(ENVIRONMENT)/poa build/$(ENVIRONMENT)/poa.strip
	rm -rf build/$(ENVIRONMENT)/state build/$(ENVIRONMENT)/state.strip
	rm -rf build/coverage
	cd deps/simulator && cargo clean
	cd tests && cargo clean

dist: clean all simulators

.PHONY: all all-via-docker dist clean fmt
