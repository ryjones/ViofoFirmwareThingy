#!/usr/bin/env python3
"""Emit the settings schema exactly as cardv defines it, as JSON.

    CARDV=re/cardv python3 tools/re/gen_schema_json.py > firmware-schema.json

Source of truth is the descriptor table at .data:0x110dc20 -- see
cardv-re.md section 3.1. Option lists are parsed out of the help strings
the camera writes as `#` comments into viofo_config.ini.
"""
import os, re, sys, json, struct
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import xref

BASE, STRIDE, COUNT = 0x110dc20, 0x30, 103
TYPE = {1: "int", 2: "text", 3: "time", 4: "section_open", 5: "section_close"}
d = xref.D
F = lambda v: v - 0x400000

def parse_options(help_text):
    """'0:Off; 1:On' and the multi-line '# 1 :4K 60fps (...)' forms."""
    if not help_text:
        return None
    out = []
    for m in re.finditer(r'(?:^|[;\n])\s*#?\s*(\d+)\s*:\s*([^;\n]+)', help_text):
        out.append({"value": int(m.group(1)), "label": m.group(2).strip()})
    seen, uniq = set(), []
    for o in out:
        if o["value"] in seen:
            continue
        seen.add(o["value"])
        uniq.append(o)
    return uniq or None

settings, section, order = [], None, 0
for i in range(COUNT):
    a = BASE + i * STRIDE
    r = d[F(a):F(a) + STRIDE]
    t, = struct.unpack('<I', r[:4])
    nm, hp = struct.unpack('<QQ', r[8:24])
    sid, = struct.unpack('<I', r[24:28])
    buf, = struct.unpack('<Q', r[32:40])
    blen, = struct.unpack('<I', r[40:44])
    name, help_text = xref.cstr(nm), xref.cstr(hp, 4000)
    if t == 4:
        section = name
        continue
    if t == 5:
        continue
    entry = {
        "key": name,
        "section": section,
        "order": order,
        "type": TYPE[t],
        "record_index": i,
    }
    order += 1
    if t == 1:
        entry["id"] = sid
        entry["help"] = help_text or ""
        opts = parse_options(help_text)
        if opts:
            entry["options"] = opts
    else:
        entry["buffer"] = f"0x{buf:x}"
        entry["buffer_length"] = blen
        entry["help"] = help_text or ""
        # The buffer is the raw allocation; the camera documents a smaller
        # user-visible cap in the help text ("Maximum length: 11 characters").
        m = re.search(r"Maximum length:\s*(\d+)", help_text or "")
        if m:
            entry["max_length"] = int(m.group(1))
    settings.append(entry)

print(json.dumps({
    "source": "cardv .data:0x110dc20",
    "firmware": "VIOFO_A329S_V2.2_260815",
    "table_records": COUNT,
    "settings": settings,
}, indent=2))
