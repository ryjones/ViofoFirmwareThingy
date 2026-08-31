import os
"""Map PLT stub addresses in cardv to imported symbol names via .rela.plt."""
import struct
d=open(os.environ.get('CARDV','re/cardv'),'rb').read(); B=0x400000; F=lambda v:v-B
sh,=struct.unpack('<Q',d[0x28:0x30]); e,n,si=struct.unpack('<HHH',d[0x3a:0x40])
raw=[struct.unpack('<IIQQQQ',d[sh+i*e:sh+i*e+40]) for i in range(n)]
stro=raw[si][4]
S={d[stro+r[0]:d.index(b'\0',stro+r[0])].decode():r for r in raw}
_,_,_,dyn_a,dyn_o,dyn_sz=S['.dynsym']
_,_,_,str_a,str_o,str_sz=S['.dynstr']
_,_,_,rp_a,rp_o,rp_sz=S['.rela.plt']
_,_,_,plt_a,plt_o,plt_sz=S['.plt']
def sym(i):
    o=dyn_o+i*24
    nm,=struct.unpack('<I',d[o:o+4])
    return d[str_o+nm:d.index(b'\0',str_o+nm)].decode()
rel=[]
for i in range(rp_sz//24):
    o=rp_o+i*24
    off,info,add=struct.unpack('<QQq',d[o:o+24])
    rel.append((off, info>>32, info&0xffffffff))
PLT={}
for i,(off,symi,typ) in enumerate(rel):
    PLT[plt_a+32+i*16]=sym(symi)
if __name__=='__main__':
    import sys
    print(f".plt 0x{plt_a:x} size 0x{plt_sz:x}, {len(rel)} JUMP_SLOTs -> {len(PLT)} stubs")
    if len(sys.argv)>1:
        for a in sys.argv[1:]:
            v=int(a,0); print(f"  0x{v:x} -> {PLT.get(v,'??')}")
    else:
        for a in sorted(PLT)[:8]: print(f"  0x{a:x} {PLT[a]}")
