#!/usr/bin/env python3
"""Extract the HTTP command dispatch table from the stripped `cardv` binary.

`cardv` answers `http://192.168.1.254/?custom=1&cmd=N` from one table in .data.
`XML_QueryCmd` (the handler behind cmd=3002) walks it with a 24-byte stride and
stops on a zero command number, so the layout is:

    struct { u32 cmd; u32 wifi_cmd_id; void *handler; u32 flag_wait; u32 setting_id; }

* `handler` non-NULL  -> the command is served in the HTTP thread.
* `handler` NULL      -> `wifi_cmd_id` is posted to the WiFiCmd task, which has
                         its own {u32 id; u32 pad; void *fn} table.
* `flag_wait`         -> non-zero means the dispatcher blocks on FLG_ID_WIFICMD
                         for these bits before replying, i.e. the command is
                         asynchronous.
* `setting_id`        -> the firmware setting this command reads and writes, the
                         same numbering used by viofo_config.ini and by
                         firmware-schema.json. Zero for commands that are not
                         settings.

The table base is not reached by ADRP+ADD; it is stored in a global that
`WifiCmd_SetCmdTable` (0x44eeb0) is handed at init, so we follow that pointer.

Usage:  CARDV=re/cardv python3 tools/re/dump_api_table.py [--json out.json]
Requires the cardv binary (see cardv-re.md section 8) and the xref cache that
tools/re/allrefs.py builds.
"""
import argparse, bisect, json, os, pickle, re, struct, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import xref                                            # noqa: E402

D, BASE = xref.D, 0x400000
TABLE_PTR_GLOBAL = 0x1042ba8      # arg to WifiCmd_SetCmdTable at 0x4461bc
WIFI_TABLE       = 0x110fb60      # {u32 id; u32 pad; u64 fn}, ends at 0xffffffff
SET_CMD_TABLE    = 0x44eeb0

# Handler names are recovered from the __func__-style literals cardv keeps. Only
# these families are accepted: every dispatch-table handler belongs to one of them,
# and anything else matching in the same function body is ordinary string data --
# the OTA handler holds "eth_md5", which would otherwise pass for a symbol.
FAMILIES = ('WiFiCmd', 'WifiCmd', 'XML_', 'OTA_', 'System_', 'CarDV', 'Storage')

def u32(vma): return struct.unpack_from('<I', D, vma - BASE)[0]
def u64(vma): return struct.unpack_from('<Q', D, vma - BASE)[0]

def ident_sites():
    """Every ADRP+ADD-referenced identifier-looking string, by referencing pc.

    cardv is stripped, but each function keeps its own name in a __func__ style
    literal, so the first such string inside a function names it."""
    cache = os.environ.get('CARDV', 're/cardv') + '.allrefs.pkl'
    if os.path.exists(cache):
        refs = pickle.load(open(cache, 'rb'))
    else:
        import allrefs
        refs = allrefs.REFS
    # A function-name literal, not just any word: require an internal underscore,
    # so plain string data in the same function ("range", "querry", "unknown")
    # cannot be mistaken for the function's name.
    ident = re.compile(r'^[A-Za-z][A-Za-z0-9]*(_[A-Za-z0-9]+)+$')
    out = []
    for addr, sites in refs.items():
        s = xref.cstr(addr)
        if s and ident.match(s):
            out += [(pc, s) for pc in sites]
    out.sort()
    return out

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--json', help='write the map here')
    ap.add_argument('--schema', default='firmware-schema.json',
                    help='firmware-schema.json, for the ini key names')
    args = ap.parse_args()

    base = u64(TABLE_PTR_GLOBAL)
    rows, vma = [], base
    while u32(vma):
        cmd, wid = u32(vma), u32(vma + 4)
        rows.append(dict(cmd=cmd, wifi_cmd_id=wid, fn=u64(vma + 8),
                         flag_wait=u32(vma + 16), setting_id=u32(vma + 20)))
        vma += 24

    wifi, w = {}, WIFI_TABLE
    while u32(w) != 0xffffffff and w < base:
        wifi[u32(w)] = u64(w + 8)
        w += 16

    sites = ident_sites()
    # Bound each handler by the next function prologue, not merely by the next
    # handler: a function with no name literal of its own would otherwise
    # inherit the name of whatever function follows it.
    ta, to, tsz = xref.TEXT_A, xref.TEXT_O, xref.TEXT_SZ
    code = struct.unpack_from(f'<{tsz // 4}I', D, to)
    starts = sorted({ta + i * 4 for i, w in enumerate(code)
                     if (w & 0xffc07fff) == 0xa9807bfd}    # stp x29,x30,[sp,#-N]!
                    | {r['fn'] for r in rows if r['fn']}   # and every known handler,
                    | set(wifi.values()) - {0})            # for functions with no such
                                                           # prologue
    def name(fn):
        if not fn: return None
        i = bisect.bisect_right(starts, fn)
        end = starts[i] if i < len(starts) else fn + 0x800
        k = bisect.bisect_left(sites, (fn, ''))
        while k < len(sites) and sites[k][0] < end:
            if sites[k][1].startswith(FAMILIES):
                return sites[k][1]
            k += 1
        return None      # no literal of its own; say so rather than guess

    ini = {}
    if os.path.exists(args.schema):
        for e in json.load(open(args.schema))['settings']:
            if 'id' in e:
                ini.setdefault(e['id'], []).append((e['key'], e['section']))

    out = []
    for r in rows:
        fn = r['fn'] or wifi.get(r['wifi_cmd_id'], 0)
        keys = ini.get(r['setting_id'], [])
        out.append(dict(cmd=r['cmd'],
                        setting_id=r['setting_id'] or None,
                        ini_keys=[k for k, _ in keys],
                        section=keys[0][1] if keys else None,
                        handler=name(fn), handler_addr=hex(fn) if fn else None,
                        dispatch='http' if r['fn'] else ('wificmd' if r['wifi_cmd_id'] else None),
                        blocks_on=hex(r['flag_wait']) if r['flag_wait'] else None))

    settings = [r for r in out if r['setting_id']]
    print(f"table at 0x{base:x}: {len(out)} commands, "
          f"{len(settings)} of them settings (these are what cmd=3014 reports), "
          f"{sum(1 for r in settings if r['ini_keys'])} present in viofo_config.ini")
    if args.json:
        json.dump(out, open(args.json, 'w'), indent=1)
        print('wrote', args.json)
    else:
        for r in out:
            sid = f"0x{r['setting_id']:02x}" if r['setting_id'] else '    '
            print(f"{r['cmd']:6d} {sid} {(r['ini_keys'] or [''])[0]:<30} {r['handler'] or ''}")

main()
