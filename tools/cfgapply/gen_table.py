#!/usr/bin/env python3
"""Regenerate settings_table.h from the cardv binary.

    CARDV=re/cardv python3 tools/cfgapply/gen_table.py > tools/cfgapply/settings_table.h
"""
import os, sys, struct
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 're'))
import xref

BASE = 0x110dc20          # settings descriptor table, see cardv-re.md 3.1
STRIDE = 0x30
d = xref.D
F = lambda v: v - 0x400000

print("/* Generated from cardv .data:0x110dc20 -- see cardv-re.md section 3.1.")
print("   Regenerate with: python3 tools/cfgapply/gen_table.py > settings_table.h */")
print("struct cfg_ent { const char *key; int id; };")
print("static const struct cfg_ent CFG_TABLE[] = {")
n = 0
for i in range(103):
    a = BASE + i * STRIDE
    r = d[F(a):F(a) + STRIDE]
    t, = struct.unpack('<I', r[:4])
    if t != 1:                     # 1 = integer setting; 2/3 are text, 4/5 sections
        continue
    nm, = struct.unpack('<Q', r[8:16])
    sid, = struct.unpack('<I', r[24:28])
    print(f'    {{ "{xref.cstr(nm)}", 0x{sid:02x} }},')
    n += 1
print("};")
print(f"#define CFG_TABLE_N {n}")
