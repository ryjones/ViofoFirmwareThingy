# Tracing `cardv`: settings, the menu, and `viofo_config.ini`

Working notes from reverse engineering `/usr/bin/cardv` (14 MB, aarch64, stripped,
non-PIE, load base `0x400000`). Companion to [cardv-findings.md](cardv-findings.md),
which covers the binary's overall shape.

**Headline result:** `cardv` **writes** `viofo_config.ini` but never reads it. The
function that would parse a value back into a setting is present in the binary and has
no callers. See [§4](#4-the-viofo_configini-flow) for the evidence.

---

## 1. Method

`objdump -d` refuses the binary outright:

```
objdump: 're/cardv': invalid section index: 32
```

Its section table has an out-of-range link. The fix is the same trick already in this
repo for raw blobs — carve `.text` and wrap it in a synthetic ELF at the right VMA:

```sh
python3 -c "d=open('re/cardv','rb').read(); open('re/cardv.text','wb').write(d[0x11d80:0x11d80+0x2b430c])"
python3 tools/mkelf.py re/cardv.text re/cardv-text.elf 0x411d80
objdump -d --start-address=0x436c00 --stop-address=0x436d00 re/cardv-text.elf
```

Three small helpers do the rest (in `re/`, alongside the extracted binary):

| script | what it does |
|---|---|
| `xref.py` | `cstr`, `ptr_refs` (8-byte LE pointers anywhere), `code_refs`/`bl_refs` (ADRP+ADD and BL) |
| `allrefs.py` | one pass over `.text` building **every** ADRP+ADD computed address → call sites, cached |
| `plt.py` | maps PLT stubs to imported symbol names via `.rela.plt` + `.dynsym` |

```sh
CARDV=re/cardv python3 tools/re/allrefs.py 0x6da2a8   # who references this address
CARDV=re/cardv python3 tools/re/plt.py 0x410650       # which libc function is this stub
```

`.dynsym` is intact even though the binary is stripped, so all 311 PLT stubs resolve:
`0x410650` = `access`, `0x410d80` = `fopen`, `0x410e40` = `fgets`, `0x410e10` = `strcmp`,
`0x410ae0` = `__isoc99_sscanf`, `0x4106d0` = `strcpy`, `0x411340` = `printf`.

### The `__func__` trick

The single most useful lever. `cardv` logs errors with `printf("ERR:%s() ...", __func__)`,
so **2,191 function-name strings** survive in `.rodata`. Each is referenced by ADRP+ADD
from inside the function it names, so `allrefs` turns a name into a code address:

```
0x006db5e0 MenuConfig_SaveCfgFile   referenced from 0x436d4c, 0x436e7c
0x006db620 Menu_LoadString          referenced from 0x436948, 0x43698c
```

Top module prefixes by count: `ImageApp` (97), `FsLinux` (96), `WiFiCmd` (89),
`UIFlowWndMovie` (64), `UVAC` (63), `FileSys` (51), `UIFlowWndPlayThumb` (46),
`System` (44), `MovieExe` (43).

---

## 2. Functions identified so far

| address | name | notes |
|---|---|---|
| `0x4514a0` | *(boot flag reader)* | `fopen("/proc/cmdline")`, `strstr("boot_update_fw")`, `sscanf("boot_update_fw=%d ")` |
| `0x4515c0` | **`set_setting(id, value)`** | `w0` = setting id, `w1` = value |
| `0x4515e0` | **`get_setting(id)`** | `w0` = setting id, returns the value |
| `0x436880` | **`Menu_LoadString`** | apply an ini value to a record — **no callers** |
| `0x4369f0` | **`Menu_SaveString`** | format a record's value for the ini |
| `0x436ab0` | *(help-text getter)* | returns `record->help`, with special cases for ids `0x11`, `0x19`, `0x1a` |
| `0x436c00` | **`MenuConfig_SaveCfgFile(path)`** | `mkdir -p`, `fopen(path,"w")`, walk the table, write |
| `0x436ea0` | *(save wrapper)* | `MenuConfig_SaveCfgFile("/mnt/sd/Config/viofo_config.ini")` |
| `0x436ec0` | **`MenuConfig_CheckFile`** | `access()`, then save |
| `0x456110` | **`Load_MenuInfo`** | settings init at boot; reads the `boot_update_fw` flag |
| `0x5b5590` | **`yq_get_config_from_ini`** | generic `[section][key]` reader for `/yq_config.ini` |

---

## 3. The settings model

### 3.1 The descriptor table

**103 records of 0x30 bytes at VMA `0x110dc20`** (file offset `0xd0dc20`), in `.data`.
Note the base is `0x110dc20`, not `0x110dc28` — the first field is a type tag:

| offset | type | meaning |
|---|---|---|
| `+0x00` | `u32` | record type (see below) |
| `+0x04` | `u32` | 0 |
| `+0x08` | `char *` | key name, e.g. `"Resolution"` |
| `+0x10` | `char *` | help text — becomes the `#` comment in the ini |
| `+0x18` | `u32` | setting id |
| `+0x1c` | `u32` | 0 |
| `+0x20` | `char *` | value buffer, for text settings |
| `+0x28` | `u32` | buffer length |

Record types, confirmed against the writer's switch at `0x436cd8`:

| type | count | meaning |
|---|---|---|
| 4 | 11 | section **open** — writes `[%s]\n` |
| 5 | 11 | section **close** |
| 1 | 75 | integer setting — writes `%s=%d` |
| 2 | 4 | text setting — writes `%s="%s"` |
| 3 | 2 | time setting `hh:mm:ss` — also `%s="%s"` |

11 sections × 2 markers + 75 + 4 + 2 = 103, and 75 + 4 + 2 = **81 keys**, exactly the
81 keys in `viofo_config.ini`.

The six non-integer settings:

| record | type | buffer | len | key |
|---|---|---|---|---|
| 38 | 3 | `0x11dfab8` | 8 | `Auto HDR Timer Start` |
| 39 | 3 | `0x11dfac4` | 8 | `Auto HDR Timer Stop` |
| 58 | 2 | `0x11df9ec` | 32 | `Custom Text Stamp` |
| 59 | 2 | `0x11df9cc` | 32 | `License Plate Number` |
| 100 | 2 | `0x11df992` | 32 | `STA mode SSID` |
| 101 | 2 | `0x11df9b2` | 26 | `STA mode password` |

The help strings are shared where possible — `0x6da360` (`"0:Off; 1:On"`) backs every
plain boolean, so editing that one string changes the comment on all of them.

### 3.2 Setting ids

Ids are a flat enum reaching `0xee`. Scanning `.text` for `MOVZ w0, #imm` followed within
four instructions by a call to `get_setting`/`set_setting` yields:

* **169** ids read via `get_setting`
* **174** ids written via `set_setting`
* **191** distinct ids in total, spanning `0x01`–`0xee`

All 75 integer ids from the ini table appear among them, so no ini key is orphaned.

**But only 75 of 191 ids are exposed in `viofo_config.ini`** — roughly 60% of the
application's settings surface never reaches that file. Ids near the top of the range
(`0xe2`–`0xef`) are initialised as a block by `Load_MenuInfo` at `0x456080`, which sets
`0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xea, 0xeb, 0xec` to constants at boot.
`0xe6` in particular gates the ini file: `MenuConfig_SaveCfgFile` opens with
`get_setting(0xe6)` and returns immediately if it is zero, which lines up with
`YQCONFIG_MENU_CONFIG_FILE_SUPPORT` in `/etc/profile_prjcfg`.

---

## 4. The `viofo_config.ini` flow

### 4.1 Only three references to the path

`/mnt/sd/Config/viofo_config.ini` lives at `0x6da2a8` and is referenced from exactly
three places:

```
0x436ea4   save wrapper       -> MenuConfig_SaveCfgFile(path)
0x436ed0   MenuConfig_CheckFile
0x45631c   tail call          -> MenuConfig_SaveCfgFile(path)
```

All three **write**. There is no `fopen(path, "r")` anywhere. It is also the only
config path string in the binary — the sole other `.ini` literal is an unrelated
`/yq_config.ini` at `0xde5f60`.

### 4.2 `MenuConfig_CheckFile` regenerates rather than loads

```
x20 = "/mnt/sd/Config/viofo_config.ini"
w1  = 0                       ; F_OK
bl  access
cbnz w0, not_exists           ; access() != 0 -> file absent
w19 = 0
bl  save_wrapper              ; file present -> WRITE it
...
not_exists:
printf("%s: config file %s is not exists\r\n", "MenuConfig_CheckFile", path)
w19 = -1
```

If the file is present the camera **overwrites** it with its current settings. If it is
absent it logs and returns −1.

### 4.2a What actually triggers the write

The routine at **`0x456290`** is the export. It copies the firmware version
string `VIOFO_A329S_V2.2_260815` (`0x6d6b88`) into the record buffer, sets a
length field to `0x6f8`, and tail-calls `MenuConfig_SaveCfgFile` at `0x45631c`.
It bails early in two cases, one of which logs `Viofo test : %d, not to save
menu` (`0x703490`).

It has **11 callers** across the menu code (`0x414bc8`, `0x441ee8`, `0x442334`,
`0x456398`, `0x4563c8`, `0x456490`, `0x45a410`, `0x45a510`, `0x45dcf8`,
`0x46935c`, `0x46b798`) — this is the "save the menu" path, invoked by user
action, not an init path. It sits immediately after `Load_MenuInfo`
(`0x456110`), which *is* boot-time settings init; the adjacency makes the two
easy to conflate.

`MenuConfig_SaveCfgFile` itself opens with `get_setting(0xe6)` and returns
immediately if that is zero. `0xe6` is **Export Settings**, reachable over HTTP
as `cmd=9352`. So Export Settings On is necessary but not sufficient: the export
still has to be requested.

Notably `WiFiCmd_OnExeSetExportSetting` (`0x4434b0`), the handler behind
`cmd=9352`, is only `set_setting(0xe6, value); return 1`. **Toggling the setting
over the API writes no file.**

Two dispatch-table commands *do* call the export routine:

| cmd | handler | body |
| --- | --- | --- |
| **3021** | `0x441ee0` | `bl 0x456290; return 1` — the export, and nothing else |
| **8230** | `0x442320` | `printf("system_reboot")`, export, `msleep(200)`, reboot |

Neither is reliable as a network-triggered export, because `0x456290` guards
twice before writing:

```
0x4562a4   *global_a == 1  -> b 0x455a20   ; a run of set_setting calls --
                                           ; 0x1a<-33, 0x20<-0, 0x21<-0, 0x22<-1 ...
                                           ; this is a DEFAULTS RESET, not an export
0x4562b8   *global_b == 2  -> printf("Viofo test : %d, not to save menu")
```

Live result with `9352 = 1`: `cmd=3021` answered `<Status>0</Status>` and wrote
no file, at the card root or under `Config/`. Stopping recording first
(`cmd=2001&par=0`, `cmd=2016` then reporting 0) did not change that. The status
is the dispatcher's ack, not the writer's.

The first guard deserves a warning: on that branch `cmd=3021` **applies default
settings** rather than exporting. A full `cmd=3014` diff before and after every
call made during this work showed no setting changed, so the branch was not
taken here — but anyone probing `3021` should diff `3014` around it.

Checked live: with `cmd=9352` reporting `1`, after a power cycle the card root
holds only `DCIM` — no `viofo_config.ini`, and `GET /Config/` 404s. Booting
regenerates nothing.

### 4.3 The parser exists but is dead code

`Menu_LoadString` at `0x436880` is a complete per-record apply function. It takes a
record pointer and a value string, and:

* for type 3, `strcmp`s the record name against `"Auto HDR Timer Start"` / `"Auto HDR
  Timer Stop"` and `sscanf`s the value as `%02d:%02d:%02d`;
* otherwise checks `strlen(value) < record->buflen` and `strcpy`s it into
  `record->buffer`, logging `set str [%s] ---> [%s]`.

It has **no callers**. Checked two independent ways:

* the scripted scan finds no `BL`, no `B`, and no address-taken reference (neither
  ADRP+ADD nor an 8-byte pointer to it anywhere in the file);
* a full `objdump -d` of the binary — 710,701 lines — contains **zero** control
  transfers to `436880` of any kind (`b`, `bl`, conditional, `cbz`/`tbz`), against 2 for
  `MenuConfig_SaveCfgFile` at `436c00`, 927 calls to `get_setting` and 739 to
  `set_setting`.

The linker kept it; nothing reaches it.

Corroborating: no ini key-name string is referenced from code. Of the 206 name and help
pointers in the table, the only ones with code references are the two HDR timer names
(from inside `Menu_LoadString` itself), the `Live Video Source` help (from the help
getter), and `"Video Settings"` — and that last one is a false positive, a Matroska
element name in the MP4/MKV muxer's tag table at `0x6b2e48`.

### 4.4 What this means

On this build (u-boot tag `20260815`), **editing `viofo_config.ini` and putting it back
on the card does not change any setting.** The file is an export of the camera's current
configuration, rewritten when an export is requested (and, via
`MenuConfig_CheckFile`, whenever the camera finds one already there). The SDK clearly supports loading —
`Menu_LoadString` is written and compiled — but the call was not wired up in this
firmware.

Two caveats. First, this is a static conclusion: it says the code path does not exist,
which is strong, but it is not the same as watching the camera ignore an edited file.
Second, a *different* ini reader does exist — `yq_get_config_from_ini` at `0x5b5590`,
a generic `[section][key]` lookup over `/yq_config.ini`, with errors like
`ERR:%s() no file [%s]` and `ERR:%s() [%s][%s] value point is NULL`. That file does not
ship in the rootfs, and its path and role are not yet traced.

---

## 5. The network API

`cardv` serves Novatek's HTTP camera API — the protocol the phone app speaks — on
`http://192.168.1.254/?custom=1&cmd=N[&par=V][&str=S]`. The handlers are the `XML_*`
family, 30 of them, all named by `__func__` strings, plus a much larger `WiFiCmd_OnExe*`
family that runs on a separate task.

### 5.1 The dispatch table

Every command the camera accepts is one row of a single table in `.data`. The table base
is not reached by ADRP+ADD; it is stored in a global that a registration function
(`0x44eeb0`) is handed at init, from `0x4461bc`:

```
0x1042ba8  ->  0x11103d8      the table
```

`WifiCmd_DispatchCmd` (`0x44ec50`) linear-scans it, and `XML_QueryCmd` — the handler
behind `cmd=3002` — walks the same rows with a 24-byte stride, stopping on a zero
command number. That fixes the layout:

```c
struct cmd_entry {          // 24 bytes
    u32   cmd;              // the number in ?custom=1&cmd=N   (0 terminates)
    u32   wifi_cmd_id;      // 0x140200xx, for the WiFiCmd task
    void *handler;          // served inline on the HTTP thread, or NULL
    u32   flag_wait;        // FLG_ID_WIFICMD bits to block on, or 0
    u32   setting_id;       // the firmware setting id, or 0
};
```

**170 commands**, from `1001` to `9364`.

* `handler != NULL` — answered on the HTTP thread. These are the `XML_*` getters.
* `handler == NULL` — `wifi_cmd_id` is posted to the WiFiCmd task, which has its own
  `{u32 id; u32 pad; void *fn}` table at `0x110fb60` (129 entries) holding the
  `WiFiCmd_OnExe*` setters.
* `flag_wait != 0` — the dispatcher blocks on those event-flag bits before replying, so
  the command is asynchronous. `0x10` is shared by `3010` (format card) and `9317`
  (format SSD); `0x80` is `3011` (factory reset); `0x1` marks the sensor and HDR
  switches, which restart the capture pipeline.

### 5.2 `setting_id` is the ini↔HTTP bridge

The fifth field is the same setting id used by the `set_setting`/`get_setting` pair in
§3 and by `viofo_config.ini`. That makes the mapping between the two interfaces a
lookup, not a guess:

| cmd | setting id | ini key |
|-----|-----------|---------|
| 8222 | `0x1a` | `Resolution` |
| 2003 | `0x22` | `Loop Recording` |
| 8205 | `0x97` | `Parking Mode` |
| 9361 | `0xeb` | `Dewarp Front Cam` |

66 of the ini's 81 keys resolve this way. The rest are the text fields and times, which
have no command of their own.

This supersedes any attempt to derive the mapping by matching key names against VIOFO's
`CMD_KEY` strings — that approach produced confident wrong answers, and the reasoning is
kept in the app repo's `docs/camera-http-api.md` §7 as a warning.

### 5.3 What `cmd=3014` reports

`XML_QueryCmd_CurSts` (`cmd=3014`) returns the current value of every setting. Against a
live A329S it returned 93 command/value pairs, and the table says exactly which 93:
**the commands whose `setting_id` is non-zero — 93 of the 170.** Checked both
directions on the real capture, the two sets are identical with no exceptions.

That closes the "22 unidentified commands" gap left by working from VIOFO's app
database, which describes only 87 of them. All 22 are ordinary table rows:

| cmd | setting id | handler | ini key |
|-----|-----------|---------|---------|
| 2001 | `0x18` | `WiFiCmd_OnExeMovieRec` | — |
| 2002 | `0x19` | `WiFiCmd_OnExeSetMovieRecSize` | — |
| 2005 | `0x05` | — | — |
| 2006 | `0x23` | `WiFiCmd_OnExeSetMotionDet` | — |
| 2012 | `0x86` | `WiFiCmd_OnExeSetAutoRecording` | — |
| 2016 | `0x18` | `XML_GetMovieRecStatus` | — |
| 2020–2024 | `0x2e`–`0x31`, `0x28` | — | — |
| 3007 | `0x37` | — | — |
| 3009 | `0x3b` | `WiFiCmd_OnExeTV` | — |
| 3028 | `0x11` | — | `Live Video Source` |
| 3033 | `0x80` | — | — |
| 8216 | `0x28` | `WiFiCmd_OnExeSensorRotate` | — |
| 8217 | `0xad` | `WiFiCmd_OnExeFlipMirror` | — |
| 8224 | `0xaa` | `WiFiCmd_OnExeSetSensor1Rotate` | — |
| 9302 | `0xbd` | — | — |
| 9341 | `0xdd` | `WiFiCmd_OnExeSetParkingHybirdMode` | `Hybrid Parking mode` |
| 9353 | `0xe7` | `WiFiCmd_OnExeSetImportSetting` | — |
| 9362 | `0xec` | `WiFiCmd_OnExeSetGspGeofenceStandby` | `Low Power Impact Recording` |

Note `3028` and `8202` both carry setting `0x11`, and `2004` and `9318` both carry
`0x1c` — a command number is not a unique name for a setting in either direction.

### 5.4 Export and import are toggles, not triggers

`9352` and `9353` look like actions and are not. Both compile to a single
`set_setting(id, par)`:

```
443500  WiFiCmd_OnExeSetImportSetting
443510      mov  w0, #0xe7          ; setting id
443514      bl   4515c0             ; set_setting(id, par)
```

Setting `0xe6` is *Export Settings* and `0xe7` is *Import Settings*. The export flag has
a real consumer: `MenuConfig_SaveCfgFile` (`0x436c00`) opens with
`get_setting(0xe6)` and returns immediately if it is zero, so **`viofo_config.ini` is
only written at all while Export Settings is on.** That is why probing `9352&par=0`
during an earlier session silently stopped the camera exporting its config.

`0xe7` has no such consumer in this build. The only reads are the range-clamp at
`0x452414` and the reset-to-defaults at `0x4560c8` — nothing acts on it. So `cmd=9353`
stores a flag and, in `V2.2_260815`, nothing imports anything. It should still be
treated as dangerous rather than as a read: it is a write to a persisted flag whose
behaviour in another firmware build is unknown.

### 5.5 Other handlers worth knowing

Recovered from string references inside each handler's body:

| cmd | what it does |
|-----|--------------|
| 3002 | lists every command number the camera supports — the API describing itself |
| 3012 | firmware version string |
| 3025 | OTA check against `http://115.29.201.46:8020/download/filedesc.xml` |
| 3026 | **firmware update** — downloads `http://%s%s` to `A:\FWA329S.bin` |
| 3029 | current SSID and passphrase, in clear |
| 2019 | live-view URLs (`rtsp://%s/xxx.mov`, `http://%s:8192`) |
| 8003 | MAC address |
| 8058 | GPS fix, satellites, lat/lon/alt/speed |
| 8228 / 8229 | licence plate string |
| 8230 | `system_reboot` |
| 8231 | app quit |
| 9327 | change current storage, restarts the capture mode |

`XML_GetMenuItem` (`0x44a820`, `cmd=3031`) walks an array of 24-byte records and emits
one `<Item>` per record, but the array is caller-supplied — reached via
`ldr x7, [x21, #16]` off the request object — so the handler only serialises a menu that
is assembled elsewhere. A bare `cmd=3031` answers `<Status>-21</Status>`; it wants a
`par`. Finding who fills that field is still the open thread for menu labels.

### 5.6 Extracting it yourself

```sh
CARDV=re/cardv python3 tools/re/dump_api_table.py                 # human-readable
CARDV=re/cardv python3 tools/re/dump_api_table.py --json api-map.json
```

`api-map.json` in this repo is that output: 170 rows of command number, setting id, ini
keys, handler name and address, dispatch route, and blocking flags.

Handler names come from the `__func__` literals described in §1, and 96 of the 170
resolve. Two rules keep that from turning into invention:

* a candidate literal must lie between the handler's address and the next function
  prologue *or* the next known handler, whichever comes first — otherwise a function
  with no literal of its own inherits its neighbour's name;
* it must belong to one of the handler families (`WiFiCmd`, `XML_`, `OTA_`, `System_`,
  `CarDV`, `Storage`). Every handler in this table does. Without that rule the OTA
  command at `9310` picks up `eth_md5`, which is one of its own `par` keywords rather
  than its name.

A handler that satisfies neither is reported as unnamed.

## 6. The menu system

Menu screens are named in `__func__` strings. The dedicated windows are:

```
UIMenuWndSetup                  UIMenuWndSetupGpsGeofence
UIMenuWndSetupADAS              UIMenuWndSetupGpsMsg
UIMenuWndSetupAICalibration     UIMenuWndSetupHDRTimer
UIMenuWndSetupBSD               UIMenuWndSetupStorage
UIMenuWndSetupCarNumber         UIMenuWndSetupTimeZone
UIMenuWndSetupDateTime          UIMenuWndSetupUserDefinedInfo
UIMenuWndSetupDefaultSetting    UIMenuWndSetupVersion
UIMenuWndSetupFormat            UIMenuWndSetupVoiceCommand
UIMenuWndSetupFormatConfirm     UIMenuWndSetupVolume
UIMenuWndFileMange              UIMenuWndPlayConfirmDel
UIMenuWndUSB
```

Ordinary list-of-options settings do not get their own window; they run through the
generic `MenuCommonItem` (the item list) and `MenuCommonOption` (the value picker),
with `MenuCommonConfirm` for yes/no. Custom actions are `MenuCustom_Format`,
`MenuCustom_Default`, `MenuCustom_Version`, `MenuCustom_FwUpdate`,
`MenuCustom_DeleteAll`, `MenuCustom_ProtectAll`, `MenuCustom_UnProtectAll`,
`MenuCustom_Beep`. The widget layer is `UxMenu_*` (`UxMenu_SetItemData`,
`UxMenu_GetRange`, …).

**Menu labels are not in the binary.** `"Loop Recording"`, `"Screen Saver"`,
`"Time Zone"` and friends occur exactly once each — only as ini key names — and there
are no localised strings at all (`Deutsch`, `Français`, `简体中文`, `Русский` are all
absent). The on-screen text therefore comes from an external UI resource keyed by string
id, which is why the menu tree cannot be enumerated from strings alone.

### Open: item-by-item reachability

Answering "is every `viofo_config.ini` key reachable from the GUI" needs the menu item
tables — the arrays that bind a menu row to a setting id and a string id. Those have not
been located yet. What is established:

* every ini setting id is genuinely used by code (`get_setting`/`set_setting`);
* the reverse does not hold — 116 of the 191 ids are *not* in the ini;
* so the ini is a subset of the settings surface, not a superset of the menu.

The next step is to find the table consumed by `MenuCommonItem`, then intersect its
setting ids with the 75 from the ini table.

---

## 7. Where settings actually live, and how to make ini edits stick

### 7.1 The persistent store

`Load_MenuInfo` (`0x456110`) does not read the ini. It reads a blob out of **PStore**
under the tag **`SYSP`**, via the accessor at `0x5b6d40`:

```c
uiFWUpdate = read_boot_update_fw();          /* 0x4514a0, from /proc/cmdline   */
settings   = *(void **)0x10443a8;            /* global -> the blob             */
PStore_op("SYSP", settings, 0, 0x6f8);       /* 1784 bytes                     */
if (settings->len != 0x6f8 || settings->magic != 0xAAAAAAAA) {
    printf("PStore reset info.\n");
    memset(settings, 0, 0x6f8);
    settings->len = 0x6f8;
    /* ... reinitialise to defaults, then write back ... */
}
```

Blob layout, from the initialiser at `0x451430`:

| offset | value |
|---|---|
| `+0x000` | `u32` magic `0xAAAAAAAA` |
| `+0x004` | `u8` `0xBA` |
| `+0x006` | `u8` `0x34` |
| `+0x0A4` | `u32` length, `0x6F8` |
| `+0x6F0` | `u32` trailer magic `0x55555555` |
| `+0x6F4` | `u32` checksum over the preceding `0x6F4` bytes (routine at `0x5b3c30`) |

Total `0x6F8` = 1784 bytes. Failures log `PStore Read sys param fail, use default
flags` and `PStore CRC ERROR, so reset info.`. The store is the **`pstore` partition**
— NAND `0x7020000`, 2 MiB, id 8 — which is *not* carried in the firmware image, so it
cannot be pre-seeded by editing `FWA329S.bin`. Also visible here: the firmware version
string `VIOFO_A329S_V2.2_260815`.

### 7.2 Supplying the missing half

Since values are applied through `set_setting(id, value)` at `0x4515c0`, and `cardv` is
not position independent, an `LD_PRELOAD` shim can call that function directly at its
absolute address. `tools/cfgapply/` implements exactly that:

* the constructor reads `/mnt/sd/Config/viofo_config.ini` **immediately**, before
  `MenuConfig_CheckFile` gets a chance to overwrite it;
* a thread waits until the `SYSP` blob's magic and length appear at `*(void **)0x10443a8`
  — i.e. until `Load_MenuInfo` has finished — so PStore values cannot clobber the
  injected ones;
* then it walks the parsed pairs and calls `set_setting(id, value)`.

The key→id map is generated straight from the binary, so it cannot drift:

```sh
docker compose run --rm re bash -c 'cd tools/cfgapply && make'
```

That produces `libcfgapply.so`, aarch64, requiring only `GLIBC_2.17` and `GLIBC_2.34`
— the camera runs Buildroot glibc **2.35**, so it loads. No libc function is
interposed, so no `dlsym` and no `libdl`.

### 7.3 Try it without flashing anything

`/etc/init.d/S99_Sysctl` runs `/mnt/sd/cardv` in preference to the flashed binary if
that file exists (see cardv-findings.md). It is executed directly, so a shell script
with a shebang works — which gives a completely reversible test:

```
SD card root:
  cardv                 <- a script, mode is irrelevant on FAT
  libcfgapply.so
  Config/viofo_config.ini   <- your edits
```

```sh
#!/bin/sh
export LD_PRELOAD=/mnt/sd/libcfgapply.so
exec /usr/bin/cardv
```

Delete the two files to go back to stock. Watch the serial console for
`cfgapply: parsed N setting(s)` and `cfgapply: applied N setting(s)`.

### 7.4 Making it permanent in `07-rootfs.ubi`

Two changes, both inside the rootfs:

```sh
docker compose run --rm ubi
cp /work/tools/cfgapply/libcfgapply.so ~/viofo/usr/lib/
# put the preload in front of every cardv branch in the script
sed -i '1a export LD_PRELOAD=/usr/lib/libcfgapply.so' ~/viofo/etc/init.d/S99_Sysctl
rebuild rootfs
exit
./target/release/viofo-fw pack unpacked -o FWA329S.bin
```

`S99_Sysctl` starts `cardv` from four different branches, so exporting the variable once
at the top of the script covers all of them rather than patching each `cardv &`.

### 7.5 Caveats

* **Untested on hardware.** It compiles, the ABI matches, and every address is derived
  from static analysis — but nobody has watched it run on a camera. Use the SD-card
  route in 7.3 first.
* **Every address is specific to one build** (`VIOFO_A329S_V2.2_260815`, u-boot tag
  `20260815`). `cardv` is not position independent, so they are stable for that build
  and meaningless for any other. `viofo-fw info` prints the tag.
* Only the 75 integer settings are applied. The six text and time settings live in
  buffers rather than behind setting ids, and would need a separate path.
* The shim does not stop `MenuConfig_CheckFile` from rewriting the ini afterwards, so
  the file on the card ends up reflecting what the camera actually has — which is
  arguably the right behaviour, but it means your edits are consumed once at boot.

## 8. Reproducing

```sh
docker compose run --rm ubi                 # mount the rootfs
cp ~/viofo/usr/bin/cardv /work/re/          # extract (re/ is gitignored)
exit

docker compose run --rm re                  # GNU binutils + capstone
objdump -d re/cardv > re/cardv.dis
CARDV=re/cardv python3 tools/re/allrefs.py  # build the xref cache (~20 s)
```

## 9. Where to pick this up

In rough order of value to someone continuing:

1. **Find the menu item tables.** The generic `MenuCommonItem` window and
   `XML_GetMenuItem` both consume arrays that are built at runtime. Locating the builder
   gives the menu tree, and intersecting its setting ids with the 75 in the ini table
   answers the reachability question outright. `cmd=3031` reaches `XML_GetMenuItem` from
   outside and wants a `par`; finding which `par` values it accepts may be a cheaper
   route to the same answer than reading the builder.
2. **Enumerate the remaining non-ini setting ids.** 27 of them now have names for free,
   because §5.2 pairs them with an HTTP command whose handler is named (`0x18`
   is `WiFiCmd_OnExeMovieRec`, `0x86` is `WiFiCmd_OnExeSetAutoRecording`, and so on).
   The rest are known only by number; each `get_setting(id)` call site says what reads
   it. Naming them would roughly double the documented configuration surface.
3. **Trace `/yq_config.ini`.** `yq_get_config_from_ini` is a real reader with
   `[section][key]` lookup, but the file does not ship in the rootfs. Where it comes
   from, and what reads out of it, is unknown.
4. **Confirm the write-only finding on hardware**, and test `tools/cfgapply` by the
   SD-card route in §7.3. Edit a value in `/mnt/sd/Config/viofo_config.ini`, reboot, and
   see whether the file comes back changed or reverted. The static evidence says
   reverted.
5. **Decode the UI resource** that holds the on-screen menu labels. None of them are in
   the binary, so this is a separate format to work out.
