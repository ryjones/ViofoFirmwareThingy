import os
#!/usr/bin/env python3
"""Cross-reference helper for the stripped, non-PIE `cardv` binary.

VMA = file offset + 0x400000 for every PROGBITS section, so addressing is 1:1.
Finds two kinds of reference to an address:
  * an 8-byte little-endian pointer anywhere in the file (tables)
  * an ADRP+ADD pair in .text (how AArch64 materialises a string address)
"""
import struct, sys

BASE = 0x400000
D = open(os.environ.get('CARDV','re/cardv'),'rb').read()

def sections():
    sh, = struct.unpack('<Q', D[0x28:0x30])
    e, n, sidx = struct.unpack('<HHH', D[0x3a:0x40])
    out = {}
    raw = []
    for i in range(n):
        o = sh + i * e
        raw.append(struct.unpack('<IIQQQQ', D[o:o+40]))
    stro = raw[sidx][4]
    for nm, typ, fl, addr, off, size in raw:
        name = D[stro+nm:D.index(b'\0', stro+nm)].decode()
        out[name] = (addr, off, size)
    return out

S = sections()
TEXT_A, TEXT_O, TEXT_SZ = S['.text']

def cstr(vma, maxlen=200):
    o = vma - BASE
    if not (0 <= o < len(D)):
        return None
    e = D.find(b'\0', o)
    if e < 0 or e - o > maxlen:
        return None
    s = D[o:e]
    try:
        t = s.decode()
    except UnicodeDecodeError:
        return None
    return t if all(9 <= c < 127 for c in s) else None

def ptr_refs(target):
    """Every place an 8-byte LE pointer to `target` is stored."""
    pat = struct.pack('<Q', target)
    out, i = [], 0
    while True:
        i = D.find(pat, i)
        if i < 0:
            break
        out.append(i + BASE)
        i += 1
    return out

def _adrp(insn, pc):
    if (insn >> 31) & 1 != 1 or (insn >> 24) & 0x1f != 0x10:
        return None
    immlo = (insn >> 29) & 3
    immhi = (insn >> 5) & 0x7ffff
    imm = (immhi << 2) | immlo
    if imm & (1 << 20):
        imm -= 1 << 21
    return ((pc & ~0xfff) + (imm << 12), insn & 0x1f)

def _add_imm(insn):
    if (insn >> 23) & 0x1ff != 0x122:
        return None
    imm = (insn >> 10) & 0xfff
    if (insn >> 22) & 1:
        imm <<= 12
    return (imm, (insn >> 5) & 0x1f, insn & 0x1f)   # imm, Rn, Rd

def code_refs(target, window=48):
    """ADRP+ADD pairs in .text that compute `target`."""
    out = []
    page = target & ~0xfff
    for off in range(0, TEXT_SZ - 3, 4):
        insn, = struct.unpack('<I', D[TEXT_O + off:TEXT_O + off + 4])
        a = _adrp(insn, TEXT_A + off)
        if not a or a[0] != page:
            continue
        base_pc, rd = a
        for j in range(4, window, 4):
            if off + j + 4 > TEXT_SZ:
                break
            i2, = struct.unpack('<I', D[TEXT_O + off + j:TEXT_O + off + j + 4])
            ai = _add_imm(i2)
            if ai and ai[1] == rd and base_pc + ai[0] == target:
                out.append(TEXT_A + off)
                break
            if _adrp(i2, 0) and (i2 & 0x1f) == rd:
                break        # register reused before the ADD
    return out

if __name__ == '__main__':
    t = int(sys.argv[1], 0)
    s = cstr(t)
    print(f"target 0x{t:x}" + (f'  {s!r}' if s else ''))
    p = ptr_refs(t)
    print(f"  pointer refs ({len(p)}): " + ' '.join(f'0x{a:x}' for a in p[:20]))
    c = code_refs(t)
    print(f"  ADRP+ADD refs ({len(c)}): " + ' '.join(f'0x{a:x}' for a in c[:20]))

def bl_refs(target):
    """Every BL in .text that calls `target`."""
    out = []
    for off in range(0, TEXT_SZ - 3, 4):
        insn, = struct.unpack('<I', D[TEXT_O + off:TEXT_O + off + 4])
        if (insn >> 26) != 0x25:
            continue
        imm = insn & 0x3ffffff
        if imm & (1 << 25):
            imm -= 1 << 26
        if TEXT_A + off + imm * 4 == target:
            out.append(TEXT_A + off)
    return out
