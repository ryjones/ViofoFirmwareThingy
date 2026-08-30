#!/usr/bin/env python3
"""Wrap a raw AArch64 blob in a minimal ELF so objdump/Ghidra/IDA load it at
the right address.

    tools/mkelf.py unpacked/04-uboot.bin uboot.elf 0x7e000000
    objdump -d uboot.elf > uboot.asm

Load addresses for the A329S image (see README.md):
    03-atf.bin    0x00000000   (position dependent; check the vector table)
    04-uboot.bin  0x7e000000   (u64 at +0x08 of the blob)
"""
import struct
import sys

if len(sys.argv) != 4:
    sys.exit(__doc__)

raw = open(sys.argv[1], "rb").read()
base = int(sys.argv[3], 0)

EHSIZE, PHSIZE, SHSIZE = 64, 56, 64
EM_AARCH64 = 183

data_off = EHSIZE + PHSIZE
shstr = b"\0.text\0.shstrtab\0"
str_off = data_off + len(raw)
sh_off = str_off + len(shstr)

ehdr = struct.pack(
    "<16sHHIQQQIHHHHHH",
    b"\x7fELF\x02\x01\x01\0" + b"\0" * 8,
    2,            # ET_EXEC
    EM_AARCH64,
    1,            # EV_CURRENT
    base,         # e_entry
    EHSIZE,       # e_phoff
    sh_off,       # e_shoff
    0,            # e_flags
    EHSIZE, PHSIZE, 1,   # ehsize, phentsize, phnum
    SHSIZE, 3, 2,        # shentsize, shnum, shstrndx
)
phdr = struct.pack("<IIQQQQQQ", 1, 5, data_off, base, base, len(raw), len(raw), 0x1000)
sh_null = b"\0" * SHSIZE
sh_text = struct.pack("<IIQQQQIIQQ", 1, 1, 6, base, data_off, len(raw), 0, 0, 4, 0)
sh_str = struct.pack("<IIQQQQIIQQ", 7, 3, 0, 0, str_off, len(shstr), 0, 0, 1, 0)

open(sys.argv[2], "wb").write(ehdr + phdr + raw + shstr + sh_null + sh_text + sh_str)
print(f"{sys.argv[2]}: {len(raw)} bytes of AArch64 at 0x{base:x}")
