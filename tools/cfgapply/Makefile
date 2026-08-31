# Build inside the RE container:  docker compose run --rm re make -C tools/cfgapply
# `?=` will not override make's built-in CC, so set it explicitly. On an arm64
# host the native cc also produces aarch64 objects, but be explicit anyway.
CC      := aarch64-linux-gnu-gcc
CFLAGS  ?= -O2 -Wall -Wextra -fPIC -shared
TARGET  := libcfgapply.so

$(TARGET): cfgapply.c settings_table.h
	$(CC) $(CFLAGS) -o $@ cfgapply.c -lpthread
	@echo
	@echo 'glibc versions required (camera has 2.35):'
	@aarch64-linux-gnu-readelf -V $@ | grep -oE 'GLIBC_2\.[0-9]+' | sort -uV | tr '\n' ' '; echo

settings_table.h:
	CARDV=$(CARDV) python3 gen_table.py > $@

clean:
	rm -f $(TARGET)
