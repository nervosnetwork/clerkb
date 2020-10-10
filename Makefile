TARGET := riscv64-unknown-elf
CC := $(TARGET)-gcc
LD := $(TARGET)-gcc
OBJCOPY := $(TARGET)-objcopy
CFLAGS := -fPIC -O3 -fvisibility=hidden -fno-builtin-memcmp -I c/deps/ckb-c-stdlib -I c/deps/ckb-c-stdlib/molecule -I c -Wall -Werror -Wno-nonnull -Wno-nonnull-compare -Wno-unused-function -g
LDFLAGS := -Wl,-static -fdata-sections -ffunction-sections -Wl,--gc-sections

# docker pull nervos/ckb-riscv-gnu-toolchain:gnu-bionic-20191012
BUILDER_DOCKER := nervos/ckb-riscv-gnu-toolchain@sha256:aae8a3f79705f67d505d1f1d5ddc694a4fd537ed1c7e9622420a470d59ba2ec3

all: build/poa

all-via-docker:
	docker run --rm -v `pwd`:/code ${BUILDER_DOCKER} bash -c "cd /code && make"

build/poa: c/poa.c
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $<
	$(OBJCOPY) --only-keep-debug $@ $@.debug
	$(OBJCOPY) --strip-debug --strip-all $@

fmt:
	clang-format -i -style=Google $(wildcard c/*.h c/*.c)
	git diff --exit-code $(wildcard c/*.h c/*.c)

clean:
	rm -rf build/poa build/poa.debug

dist: clean all

.PHONY: all all-via-docker dist clean fmt
