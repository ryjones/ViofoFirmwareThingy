import os
"""One pass over .text collecting every ADRP+ADD computed address -> [call sites].
Cached to re/allrefs.pkl so later queries are instant."""
import struct, pickle, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import xref
CACHE=os.environ.get('CARDV','re/cardv')+'.allrefs.pkl'
def build():
    d, TA, TO, TSZ = xref.D, xref.TEXT_A, xref.TEXT_O, xref.TEXT_SZ
    insns = struct.unpack(f'<{TSZ//4}I', d[TO:TO+TSZ//4*4])
    pend = {}          # Rd -> (page, pc)
    out = {}
    for i, insn in enumerate(insns):
        pc = TA + i*4
        a = xref._adrp(insn, pc)
        if a:
            pend[a[1]] = (a[0], pc)
            continue
        ai = xref._add_imm(insn)
        if ai:
            imm, rn, rd = ai
            if rn in pend:
                page, apc = pend[rn]
                out.setdefault(page+imm, []).append(apc)
            if rd in pend and rd != rn:
                del pend[rd]
            continue
        # any other write to a register invalidates it (approximate: Rd field)
        rd = insn & 0x1f
        if rd in pend and (insn >> 26) not in (0x25,):   # not BL
            del pend[rd]
    return out
if os.path.exists(CACHE):
    REFS = pickle.load(open(CACHE,'rb'))
else:
    REFS = build()
    pickle.dump(REFS, open(CACHE,'wb'))
if __name__ == '__main__':
    print(f"{len(REFS)} distinct ADRP+ADD targets in .text")
    for a in sys.argv[1:]:
        v = int(a, 0)
        r = REFS.get(v, [])
        print(f"  0x{v:x} {xref.cstr(v)!r}: {len(r)} refs " + ' '.join(f'0x{x:x}' for x in r[:8]))
