# `cardv` — the camera application

`/usr/bin/cardv` in the rootfs, **14,052,992 bytes**. This is the camera: every
mode, menu, setting, recording pipeline and network service the product has.
The rest of userspace is busybox plus `hostapd`/`wpa_supplicant` — `cardv` is
the only application of substance in the image.

Everything below was read out of the binary itself. Extract it with
`docker compose run --rm ubi` and copy `~/viofo/usr/bin/cardv` out to `/work`.

## What it is

```
ELF 64-bit LSB executable, ARM aarch64, dynamically linked,
interpreter /lib/ld-linux-aarch64.so.1, stripped
entry 0x480000, load base 0x400000, NOT position independent
```

| section | size | notes |
|---|---|---|
| `.text` | 2,835,724 | VMA `0x411d80`–`0x6c608c` |
| `.rodata` | 9,177,541 | 65% of the file — strings, tables, UI assets |
| `.data` | 1,174,464 | includes the settings table below |
| `.bss` | 1,758,504 | |
| `.devtab` | 872 | **Novatek-specific**, see below |
| `.vertab` | 1,633 | **Novatek-specific**, see below |

Only four shared libraries: `libgcc_s`, `libc`, `libm`, `libstdc++`. The entire
Novatek media SDK is statically linked in, which is why a 14 MB binary needs
almost nothing from the filesystem. It is C++ (`libstdc++`, `.gcc_except_table`,
12 `.init_array` constructors) with 318 dynamic imports — including `system`,
`popen`, `execl` and `fork`, so it shells out at runtime.

## `.vertab` — the component manifest

A plain list of 47 `Component#version_SHA.xxxxxxxx` strings between
`version_info_begin` and `version_info_end` sentinels. It names every SDK module
compiled in:

```
EthCamCmdParser  Dx  GxPower  GxVideo  GxDisplay  GxStrg  vendor_dis  GxGfx
NvtUser  UICtrl  VControl  AppCtrl  FileDB  FileSys  NameRuleCustom  NamingRule
FsLinux  fileout  MP_MovWriteLib  MP_TsWriteLib  MP_MovReadLib  bsmux  LviewNvt
GxImgFile  Exif  SizeConvert  ImageApp_Common  ImageApp_MovieMulti
ImageApp_UsbMovie  FontConv  ImageApp_Photo  MsdcNvt  PBXFile  filein
ImageApp_MoviePlay  UsockIpc  UsockCliIpc  HfsNvt  UVAC  FwSrv  EthCamSocket
ethsocketcli  EthsockCliIpc  ethsocket  EthSocketSMI
```

All the `SHA` fields are zeroed, so there is no upstream commit to correlate.
`FwSrv#1.00.028` is the firmware-update service, `UsockIpc`/`UsockCliIpc` a Unix
socket IPC layer, `EthCamSocket`/`EthSocketSMI` the networked-camera protocol,
and `MP_MovWriteLib`/`bsmux` the MP4 writer and bitstream muxer.

## `.devtab` — 21 named entry points in a stripped binary

The most useful thing in the file for reverse engineering. It is an array of
21 records of 40 bytes, `{ void *handler; char name[32] }`, and **every one of
the 21 pointers lands inside `.text`**. In a stripped 2.8 MB binary this is a
free partial symbol table:

| name | handler | name | handler |
|---|---|---|---|
| `NvtUser` | `0x004eb750` | `movie` | `0x00559580` |
| `bsmu` | `0x00531140` | `movieplay` | `0x00586350` |
| `bt` | `0x00459400` | `power` | `0x0041b4b0` |
| `fileout` | `0x0051bf10` | `sock` | `0x00445d00` |
| `fslinux` | `0x005101d0` | `sys` | `0x00415730` |
| `gxdisp` | `0x0047c3a0` | `ts` | `0x00416830` |
| `gxpower` | `0x00477110` | `uimovie` | `0x00431f00` |
| `gxvideo` | `0x0047a020` | `uvac` | `0x005904e0` |
| `iaphoto` | `0x0055ff10` | `ver` | `0x00484590` |
| `key` | `0x004166e0` | `test_msdcnvt` | `0x0043d720` |
| `mode` | `0x00451120` | | |

These are the CarDV framework's registered subsystems. `movie` is the recording
engine, `key` the button handler, `mode` the mode machine, `power` power
management, `sock` networking, `uvac` USB video class.

## The settings table

**103 records of 0x30 bytes at VMA `0x110dc28`** (file offset `0xd0dc28`), in
`.data`. This single table is the entire user-configurable surface of the
camera, and it is what produces `viofo_config.ini` on the SD card.

Record layout:

| offset | type | meaning |
|---|---|---|
| `+0x00` | `char *` | key name, e.g. `"Resolution"` |
| `+0x08` | `char *` | help text — the option list written as `#` comments into the ini |
| `+0x10` | `u32` | setting id (0 for section markers and text settings) |
| `+0x18` | `char *` | buffer for text settings, else 0 |
| `+0x20` | `u32` | buffer length for text settings |

Sections appear twice, as open and close markers, sharing one sentinel help
pointer (`0x00f7f150`). 11 sections × 2 markers + 81 settings = the 103 records,
and those 81 keys are exactly the 81 keys in `viofo_config.ini` — all 11 section
names and all 81 keys are present verbatim in the binary.

Examples:

```
[ 1] Resolution        id 0x1a   help "\n# 1 :4K 60fps (3840x2160P 60fps)\n# 2 :..."
[ 2] Video Bitrate     id 0xa1   help "0:Low; 1:Normal; 2:High; 3:Maximum"
[ 4] Loop Recording    id 0x22   help "0:Off; 1:1 Minute; 2: 2 Minutes; 3:..."
[ 9] IR LED            id 0x27   help "0:Off; 1:On; 2: Auto"
[58] Custom Text Stamp id 0      buffer 0x011df9ec, 0x20 bytes
[68] Wi-Fi             id 0x81   help "0:Off; 1:On"
[100] STA mode SSID    id 0      buffer 0x011df992, 0x20 bytes
[101] STA mode password id 0     buffer 0x011df9b2, 0x1a bytes
```

Many booleans share one help string (`0x006da360` = `"0:Off; 1:On"`), so
changing that one string changes the comment on every boolean at once.

The setting ids are a flat enum — `Resolution` 0x1a, `Wi-Fi` 0x81, `GPS` 0x92,
`Voice Notification Volume` 0x71 — and they are the handle the rest of the
application uses to get and set values. Cross-referencing an id in `.text` finds
the code that acts on that setting.

## Can it be modified?

Yes, by several routes, in rough order of effort.

### 1. Run a replacement from the SD card — no flashing at all

`/etc/init.d/S99_Sysctl` has a developer escape hatch left enabled in the
shipped firmware:

```sh
if [ -f /mnt/sd/cardv ]; then
    echo -e "\e[1;31m\rRun test process!!! \r\e[0m"
    ./mnt/sd/cardv &
elif [ -f /mnt/sd/sdlog ]; then
    ...
    cardv &
```

If a file named `cardv` exists in the root of the SD card, the camera runs
**that** instead of the flashed binary. A modified application can therefore be
tested without touching the flash at all, and recovered by deleting one file
from the card. Dropping an empty file named `sdlog` on the card selects the
logging branch instead.

Two caveats I could not verify without hardware: the path is written
`./mnt/sd/cardv`, relative, so it depends on the working directory being `/`
when the script runs; and the SD card is a FAT/exFAT volume, where the execute
bit comes from the mount options rather than the file. The mount options used in
the image are `-o dirsync,time_offset=...` with no `noexec`, which normally
leaves files executable, but the actual mount is performed outside these
scripts.

### 2. Change the init scripts

`/etc/init.d/*` is shell. Repack the rootfs (`rebuild rootfs` in the container,
then `viofo-fw pack`) to start extra daemons, wrap `cardv` with a logger, or
disable `inetd`. `/etc/profile` already enables unlimited core dumps with the
pattern `/var/log/core-%e-%p-%t`, which is useful when a patched binary crashes.

### 3. `LD_PRELOAD`

`cardv` is dynamically linked, so libc calls can be interposed. The mechanism is
already wired up and merely commented out — `/etc/profile` line 9:

```sh
#export LD_PRELOAD="libnvtlibc.so"
```

`/usr/lib/libnvtlibc.so` is a 5,896-byte shim exporting only `memcpy` and
`memset`, so it is not a general hook, but it proves the path works. Since
`cardv` imports `fopen`, `open`, `ioctl`, `system` and `popen`, a preload
library can intercept configuration reads, file naming, or the commands it
shells out to, without touching the binary.

### 4. Binary patching

The binary is **not position independent** and every one of its 15 PROGBITS
sections satisfies `VMA = file_offset + 0x400000` exactly. There is no
relocation arithmetic: a byte at VMA `0x110dc28` is at file offset `0xd0dc28`,
always. Combined with the 21 named handlers in `.devtab` and the setting ids in
the table above, this is about as tractable as patching a stripped binary gets.

Load it into a disassembler at base `0x400000`; `tools/mkelf.py` is unnecessary
here since `cardv` is already a well-formed ELF that Ghidra and IDA open
directly.

### 5. Editing the settings table

Because the table drives both the ini file and the option help text, small
changes are cheap: re-word an option list, change a text field's buffer length,
or repoint a key name. Adding a genuinely new setting means adding a 48-byte
record and new strings, which needs space — but `.rodata` is 9 MB of mostly
strings and there is room in the file for a new section if you extend it
properly.

Changing a *value range* is the interesting case: the help string is only a
comment, so widening `"0:Low; 1:Normal; 2:High; 3:Maximum"` to a fourth level
does nothing on its own. The clamp lives in the code that consumes setting id
`0xa1`, which is where the cross-reference from the id becomes useful.

## What stays hard

* No symbols beyond the 21 in `.devtab` and the 318 libc/libstdc++ imports.
* The SDK components in `.vertab` are statically linked, so there is no library
  boundary to swap out — a change to `GxVideo` behaviour means patching `.text`.
* `FwSrv`, the firmware update service, is inside this binary. Anything that
  changes how updates are validated is in the highest-risk area of the image.
* No hardware available here, so none of the above is verified on a camera —
  only on the firmware image.

## Rebuilding after a change

```sh
docker compose run --rm ubi          # rootfs at ~/viofo, app at ~/viofo/mnt/app
cp /work/re/cardv ~/viofo/usr/bin/cardv
rebuild rootfs
exit
./target/release/viofo-fw pack unpacked -o FWA329S.bin
./target/release/viofo-fw verify FWA329S.bin
```

Or skip all of it during development and put the binary on the SD card as
`/cardv`, per route 1.
