# viofo-fw — split and rebuild VIOFO A329S firmware

**No firmware, and no other binaries, are included in this repo — bring your own
`FWA329S.bin`.** Download it from VIOFO:

> <https://www.viofo.com/pages/firmware?s=A329S%20>

Unzip it and drop `FWA329S.bin` in the root of this checkout; `.gitignore` keeps it
out of the repo. Everything here operates on the file you supply. The findings
documents describe the build this was written against — check yours matches with:

```sh
viofo-fw info FWA329S.bin        # u-boot build tag should read 20260815
```

A different release will still unpack and repack correctly, but the addresses quoted
in the findings documents are specific to that build.

`FWA329S.bin` is a **Novatek NVTPACK** firmware container for a **Novatek NA51102**
(`novatek,na51102` / `nvt,ca53`, Cortex-A53) running Linux 5.10.168 with Novatek's
"CarDV" application stack. This repo contains `viofo-fw`, a Rust CLI that splits the
image into per-partition files for reverse engineering and rebuilds a flashable image
from them.

The round trip is byte-exact: unpacking the stock image and repacking it with no edits
reproduces `FWA329S.bin` bit for bit, same SHA-256. That property is the whole point —
it means anything that *does* change in your output is something you changed.

Two companion documents record what is actually inside the partitions:

* **[configuration.md](configuration.md)** — the rootfs configuration: the
  `/etc/profile_prjcfg` product definition, the boot chain, accounts and network
  daemons, and which SDK sample files are stale enough to mislead you.
* **[application-findings.md](application-findings.md)** — `application.dtb`, the ISP
  tuning database in the app partition, and its little-endian gotcha.
* **[cardv-findings.md](cardv-findings.md)** — `/usr/bin/cardv`, the camera application:
  what is in it, the settings table that generates `viofo_config.ini`, and the routes to
  modifying it (including an SD-card override that needs no flashing).
* **[cardv-re.md](cardv-re.md)** — tracing `cardv` further: the settings model and its
  191 ids, the menu subsystem, the network API, and the finding that `viofo_config.ini`
  is written but never read — plus where settings really live (a `SYSP` blob in the
  pstore partition) and what to change in the rootfs to make ini edits take effect.
  Ends with a list of open threads to pick up.
* **[api-map.json](api-map.json)** — the camera's HTTP API, all 170 commands, read out
  of `cardv`'s dispatch table rather than guessed: command number, the firmware setting
  it reads and writes, the matching `viofo_config.ini` key, the handler, and whether the
  call blocks. Regenerate with `CARDV=re/cardv python3 tools/re/dump_api_table.py --json api-map.json`;
  the derivation is [cardv-re.md](cardv-re.md) §5.

> **This can brick your camera.** A bad `loader`/`atf`/`uboot` leaves you needing a
> hardware SPI flash programmer. U-Boot verifies each partition's checksum before it
> erases anything, so a corrupt image is rejected rather than half-written — but that
> is your only safety net. Start with the `app` and `rootfs` partitions, and keep the
> stock `FWA329S.bin` somewhere safe.

---

## 1. Setup

### The tool itself

Rust, stable toolchain. Nothing else is required to unpack, inspect or repack.

```sh
# install rust if you don't have it: https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo build --release      # binary lands in target/release/viofo-fw
cargo test                 # checksum / container unit tests
```

Rust was chosen because this job is all binary layout arithmetic and checksums over a
90 MB buffer: a single static binary, no runtime, exact integer widths, and slicing
that panics loudly instead of silently reading past the end of a partition. The
dependency list is deliberately tiny — `clap` (CLI), `serde` + `toml` (manifest),
`crc32fast` (uImage CRCs), `anyhow` (errors). Everything format-specific is
hand-written in `src/`.

### Optional tools for going deeper into a partition

None of these are needed by `viofo-fw`; they are what you reach for *after* the split.

| What | Install (macOS / Homebrew) | Used for |
|---|---|---|
| `dtc` (device tree compiler) | `brew install dtc` | `01-fdt.dtb` → readable `.dts` |
| colima + docker | `brew install colima docker docker-compose` | mounting `07-rootfs.ubi` / `09-app.ubi` read-write — see §6 |
| `xz` / Python `lzma` | preinstalled | `06-kernel.lzma` → the arm64 `Image` |
| Ghidra or IDA | `brew install --cask ghidra` | AArch64 disassembly of `atf` / `uboot` |
| `binwalk` | `brew install binwalk` | quick triage of anything unrecognised |

`tools/mkelf.py` (in this repo) wraps a raw AArch64 blob in a minimal ELF at a chosen
load address, so plain `objdump -d` — or Ghidra's ELF loader — puts the code at the
right addresses without you configuring anything:

```sh
tools/mkelf.py unpacked/04-uboot.bin uboot.elf 0x7e000000
objdump -d uboot.elf > uboot.asm
```

That is how the format below was recovered: the shipped U-Boot contains Novatek's
`board/novatek/common/nvt_ivot_fw_update_utils.c`, complete with its error strings, and
the checksum routine it calls sits at `0x7E011EF8`.

---

## 2. Usage

```sh
viofo-fw info    FWA329S.bin              # partition table + container headers
viofo-fw verify  FWA329S.bin              # every Novatek checksum and uImage CRC
viofo-fw unpack  FWA329S.bin -o unpacked  # split into files + manifest.toml
viofo-fw pack    unpacked -o new.bin      # rebuild, recomputing everything
```

`info` on the stock image:

```
  id  name         offset     size       cksum    contents
  1   fdt          0x000000c8 0x00026387 0x2450   device tree blob, totalsize 0x26387
  3   atf          0x00026488 0x0000c088 ok       raw, nvt tag "bl51102" ver "0" date "0" @0x3a0
  4   uboot        0x00032548 0x000fa530 ok       raw, nvt tag "ub51102" ver "FFFFFFFF" date "20260815" @0x350
  6   kernel       0x0012ca88 0x002cb924 0x9a70   uImage "Linux-5.10.168" os=5 arch=22 type=2 comp=0 load=0x0
  7   rootfs       0x003f83c8 0x05360040 ok       CKSM v0x16040719 payload 0x5360000 @0x40
  9   app          0x05758408 0x00200040 ok       CKSM v0x16040719 payload 0x200000 @0x40
```

`unpack` strips each partition's container header into `manifest.toml` and writes just
the payload, so the files are directly usable:

```
unpacked/
  manifest.toml
  01-fdt.dtb        156,551   flattened device tree
  03-atf.bin         49,288   ARM Trusted Firmware BL31, tag "bl51102"
  04-uboot.bin    1,025,328   U-Boot, tag "ub51102", built 2026-08-15, loads at 0x7E000000
  06-kernel.lzma  2,930,916   LZMA-alone stream -> 8,654,856 byte arm64 Image
  07-rootfs.ubi  87,425,024   UBI image, 128 KiB PEBs, one dynamic volume (UBIFS)
  09-app.ubi      2,097,152   UBI image, 128 KiB PEBs, one dynamic volume (UBIFS)
```

Pass `--raw` to keep the container headers inside the partition files instead
(`container = "raw"` for everything). Both modes round-trip byte-exact.

### Prove the round trip before you trust it

```sh
viofo-fw unpack FWA329S.bin -o unpacked
viofo-fw pack   unpacked -o rebuilt.bin
cmp FWA329S.bin rebuilt.bin && echo identical
```

### Editing

Everything positional is recomputed at pack time — partition offsets, sizes, the image
total size, all four checksum words, both uImage CRCs. You never hand-edit an offset.
Partitions may grow or shrink freely; downstream ones slide and re-align.

```sh
# example: change something inside the app filesystem
ubireader_extract_files -o app_fs unpacked/09-app.ubi
# ... edit app_fs ...
# rebuild a UBI image with mkfs.ubifs + ubinize (Linux, mtd-utils), then:
cp new-app.ubi unpacked/09-app.ubi
viofo-fw pack unpacked -o FWA329S.bin
```

`manifest.toml` is the editable description of the image. Reordering, removing, or
adding `[[partition]]` blocks works; so does pointing `file` somewhere else.

---

## 3. Flash size budget

From the `nand@2,f0400000` node of the device tree — a 128 MiB SPI NAND. A partition
you grow must still fit its NAND slot, and U-Boot refuses the update with
*"Partition[%d] Size is too smaller than that you wanna update"* if it does not.

| id | label | NAND offset | NAND size | in this image | headroom |
|----|-------|------------|-----------|---------------|----------|
| 0 | loader | 0x0000000 | 0x40000 | *not shipped* | — |
| 1 | fdt | 0x0040000 | 0x40000 | 0x26387 | 0x19C79 |
| 2 | fdt.restore | 0x0080000 | 0x40000 | *not shipped* | — |
| 3 | atf | 0x00C0000 | 0x40000 | 0xC088 | 0x33F78 |
| 4 | uboot | 0x0100000 | 0x1C0000 | 0xFA530 | 0xC5AD0 |
| 5 | uenv | 0x02C0000 | 0x40000 | *not shipped* | — |
| 6 | kernel | 0x0300000 | 0x400000 | 0x2CB924 | 0x1346DC |
| 7 | rootfs | 0x0700000 | 0x6920000 | 0x5360040 | 0x15BFFC0 (~22 MiB) |
| 8 | pstore | 0x7020000 | 0x200000 | *not shipped* | — |
| 9 | app | 0x7220000 | 0xDA0000 | 0x200040 | 0xB9FFC0 (~11.6 MiB) |
| 10 | par | 0x7FC0000 | 0x20000 | *not shipped* | — |

A firmware file only needs to carry the partitions it wants to replace; the ones absent
from the table are left alone on the device. There is plenty of room in `rootfs` and
`app`, which is where camera features live.

---

## 4. The format, as reverse engineered

Everything here was recovered from the image itself and confirmed against the shipped
U-Boot. All integers are little-endian unless noted.

### 4.1 Image header — `NVTPACK_FW_HDR2`

```
0x00  16  GUID           072E01D6 BC10 914F B28A352F82261A50
                         ({D6012E07-10BC-4F91-B28A-352F82261A50})
0x10  u32 version        0x16071515
0x14  u32 header_size    0x80   — also the offset of the partition table
0x18  u32 partition_count
0x1C  u32 total_size     — equals the file size
0x20  u32 chksum_method  0
0x24  u32 chksum_value   — low 16 bits are the corrective word
0x28  ..  zero to header_size
0x80      partition_count × { u32 offset; u32 size; u32 id }
```

Partition data begins right after the table, at `header_size + 12 × count` (0xC8 here).
Each subsequent partition starts on a **0x40 boundary measured from that first offset**,
zero-padded. `viofo-fw` re-derives the alignment from the image it unpacks and stores
it as `firmware.align`, so an image built with a different packer still round-trips.

### 4.2 The Novatek checksum

The one non-obvious piece. From the routine at `0x7E011EF8` in the `uboot` partition:

```c
uint32_t nvt_chksum(void *buf, uint32_t len) {
    uint32_t sum = 0;
    uint16_t *p = buf;
    for (uint32_t i = 0; i < (len >> 1); i++)
        sum += p[i] + i;          /* the loop index is added too */
    return sum & 0xFFFF;
}
```

Adding the index is what defeats every off-the-shelf checksum guess. Every caller
compares the result against **zero**, so a region is valid when it sums to zero; the
producer parks a corrective 16-bit word inside the region to make that true. Given a
region with its corrective slot zeroed, the word to store is simply `-sum & 0xFFFF`.

Four regions carry such a word in this image:

| region | slot | verified value |
|---|---|---|
| whole file | `0x24` (image header) | `0xEA7A` |
| `atf` partition | tag + `0x1E` = `0x3BE` | `0x65EA` |
| `uboot` partition | tag + `0x1E` = `0x36E` | `0x924C` |
| `rootfs`, `app` | CKSM header `+0x0C` | `0xA0F4`, `0x7450` |

`fdt` and `kernel` ship with a non-zero residue — the kernel relies on its uImage CRCs
instead, and the device tree on nothing — so `viofo-fw verify` reports those as a note,
not a failure, and `pack` leaves them alone.

### 4.3 `CKSM` container — `rootfs`, `app`

```
0x00  "CKSM"
0x04  u32 version       0x16040719
0x08  u32 (0)
0x0C  u32 corrective checksum word
0x10  u32 data offset   0x40
0x14  u32 data length
0x18  u32 (0)
0x1C  u32 (9)
0x20  ..  zero to 0x40
0x40      payload — a UBI image
```

U-Boot checksums `[0x10] + [0x14] + [0x18]` bytes from the start of the header, i.e.
the whole partition. Mismatching versions produce
*"Wrong HEADER_CHKSUM_VERSION %08X(uboot) %08X(root-fs)"*; a missing magic produces
*"root-fs has no CKSM header"*.

### 4.4 Novatek build tag — `atf`, `uboot`

A 32-byte record at a fixed link-time offset (`0x3A0` in `atf`, `0x350` in `uboot`):

```
+0x00 char tag[8]        "bl51102\0" / "ub51102 "
+0x08 char version[8]    "FFFFFFFF"
+0x10 char date[8]       "20260815"
+0x18 u32  partition size
+0x1C u16  magic 0xAA55
+0x1E u16  corrective checksum word
```

`viofo-fw` locates it by scanning for the `0xAA55` magic preceded by a size field equal
to the partition length, records the offset in the manifest, and on pack rewrites both
the size and the checksum word. U-Boot compares the tag against its own expected string
(*"uboot pat%d, tag not match %8s(expect) != %8s(bin)"*), so leave the tag text alone
unless you know what the other side expects.

### 4.5 uImage container — `kernel`

Stock U-Boot legacy image header (big-endian), 0x40 bytes, followed by an LZMA-alone
stream that decompresses to an 8,654,856-byte arm64 `Image` (`ARMd` magic at +0x38):

```
os = 5 (Linux)  arch = 22 (arm64)  type = 2 (kernel)  comp = 0  name = "Linux-5.10.168"
```

`comp` is 0 even though the payload is LZMA — Novatek's U-Boot decompresses it itself.
Both the header CRC32 and the data CRC32 are recomputed by `pack`.

### 4.6 Partition ids

From the `nvtpack/index` node of the device tree (`ver = "NVTPACK_FW_INI_16072017"`),
which also names the file each partition was built from:

| id | name | source file |
|----|------|-------------|
| 0 | loader | |
| 1 | fdt | `nvt-evb.bin` |
| 2 | fdt.restore | |
| 3 | atf | `atf.bin` |
| 4 | uboot | `u-boot.bin` |
| 5 | uenv | |
| 6 | kernel | `Image.bin` |
| 7 | rootfs | `rootfs.ubifs.bin` |
| 8 | pstore | |
| 9 | app | `appfs.cardv.ubifs.nand.bin` |
| 10 | par | |

---

## 5. Working on each partition

**`01-fdt.dtb`** — `dtc -I dtb -O dts 01-fdt.dtb -o fdt.dts`. This is the map of the
whole board: the NAND partition table, the `nvtpack` index above, pinmux, sensors,
clocks. Edit and `dtc -I dts -O dtb` back; it needs no checksum fixup.

**`03-atf.bin` / `04-uboot.bin`** — AArch64. Use `tools/mkelf.py` with load address
`0x7E000000` for U-Boot (it is the `u64` at offset `+0x08` of the blob). The firmware
update logic, including everything documented above, lives around `0x7E00F900`–
`0x7E012C90`. `pack` re-solves the tag checksum for you after any edit.

**`06-kernel.lzma`** —
`python3 -c 'import lzma,sys; sys.stdout.buffer.write(lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(open("unpacked/06-kernel.lzma","rb").read()))' > Image`.
To go back, recompress with LZMA-alone settings matching the header (`lc=3 lp=0 pb=2`,
64 MiB dictionary) and drop the result back in place.

**`07-rootfs.ubi` / `09-app.ubi`** — UBI images, 128 KiB PEBs, VID header at `+0x800`,
data at `+0x1000`, one dynamic volume each (`rootfs`, 805 LEBs; `app`, 73 LEBs), UBIFS
with LZO compression. Mount them read-write and edit them in place — see §6.

The camera's user-facing behaviour — menus, recording modes, the settings in
`viofo_config.ini` — is described in VIOFO's user manual for the camera and implemented in the
`app` and `rootfs` partitions. That is where features get added; what is already in
them is written up in [configuration.md](configuration.md) and
[application-findings.md](application-findings.md).

Short version of both: the product is Novatek SDK project **W49U** on NA51102, with one
IMX678 and two IMX675 sensors, an ST7701S DSI panel and Broadcom (AMPAK AP6611C) Wi-Fi.
`/etc/profile_prjcfg` lists ~60 enabled features, but only three are read by any script
— the rest are compile-time switches baked into `/usr/bin/cardv` (14 MB, stripped), so
flipping them in that file changes nothing.

---

## 6. Mounting `rootfs` and `app` for editing (macOS)

UBIFS is a Linux filesystem on simulated NAND, so macOS cannot mount it directly. The
setup below gets you a genuine read-write mount: a **colima** VM supplies the kernel
(`nandsim` emulates the camera's SPI NAND; `ubi` + `ubifs` do the mounting), and an
**Alpine** container supplies `mtd-utils`. `docker-compose.yml` wires the two together.

The VM's own filesystem is never modified. The `kmod` image carries the kernel modules
and inserts them at run time, so `colima restart` puts everything back to stock.

### One-time setup

```sh
brew install colima docker docker-compose        # if you don't have them

# the repo must be visible inside the VM; /Volumes is not mounted by default
colima stop
colima start --mount "$(pwd -P):w"

# compose binds by absolute path; record the physical one (matters if you reach
# the repo through a symlink)
echo "VIOFO_ROOT=$(pwd -P)" > .env

docker compose build          # ~100 MB: pulls the modules for the VM's kernel
```

`docker compose build` runs *inside* the VM, so `uname -r` in `docker/kmod.Dockerfile`
is the VM's kernel and the modules baked in match it. After a VM kernel upgrade the
`kmod` container refuses to run and tells you to
`docker compose build --no-cache kmod`.

### Mount and edit

```sh
docker compose run --rm ubi
```

Both volumes come up at once, laid out the way the camera lays them out, and you land
in a shell in the root of it:

```
  /root/viofo                  <- rootfs   805 LEBs x 126976 B, min I/O 2048
  /root/viofo/mnt/app          <- app       73 LEBs x 126976 B, min I/O 2048

  both read-write, PEB 131072, compression lzo

  edit under /root/viofo, then run:
      rebuild           both images
      rebuild rootfs    just 07-rootfs.ubi
      rebuild app       just 09-app.ubi
```

`~/viofo` is the rootfs; `~/viofo/mnt/app` is the app volume, which is exactly where
`/etc/init.d/S07_APP_Overlay` mounts it on the device — and it attaches as `ubi1_0`
here just as it does there, so paths inside the tree are the camera's real paths.

Edit with ordinary tools — you are root in a Linux namespace, so ownership, modes and
symlinks all behave. Then:

```sh
rebuild           # writes unpacked/07-rootfs.ubi and unpacked/09-app.ubi
exit
```

`rebuild` runs `mkfs.ubifs -r <mount>` followed by `ubinize`, taking every geometry
parameter from the attached volume rather than from hardcoded numbers. It unmounts the
app volume while packing the rootfs, so app files never leak into the rootfs image, and
remounts it afterwards. Back on the host:

```sh
./target/release/viofo-fw pack unpacked -o FWA329S.bin
./target/release/viofo-fw verify FWA329S.bin
```

The rebuilt `.ubi` will not be byte-identical to the original — UBIFS lays nodes out
differently each time, and the rootfs comes out a little larger (~686 PEBs vs 667) —
but it mounts identically and carries your changes. If you want a byte-exact baseline
again, re-run `viofo-fw unpack`.

Overridable with environment variables: `VIOFO_MNT` (default `~/viofo`), `APP_SUBDIR`
(default `mnt/app`), `ROOTFS_IMG`, `APP_IMG`, `COMPR`, and `ROOTFS_OUT` / `APP_OUT` to
make `rebuild` write somewhere other than the source images.

### What is inside

`rootfs` is a Buildroot-style busybox userland — no device nodes, no setuid binaries,
no hard links, everything `root:root`, 362 symlinks, ~1700 files. The interesting entry
points are `/etc/init.d/` (`S15_NvtAppInit` starts the camera application,
`S05_FS_Overlay` sets up the writable overlay, `S07_APP_Overlay` mounts the app volume)
and `/usr/bin`. `app` holds the camera's tuning data: `application.dtb`, `sensor/*.cfg`,
`motor/`. Both are documented in detail in [configuration.md](configuration.md) and
[application-findings.md](application-findings.md).

### Notes and undo

* `nandsim` is created with `first_id_byte=0x20 second_id_byte=0xf1 third_id_byte=0x00
  fourth_id_byte=0x1d` — 128 MiB, 2048-byte pages, 128 KiB erase blocks, matching the
  camera's chip, so LEB size and header offsets come out identical to the real device.
* It is split with `parts=840,109`: the two MTD partitions are the exact erase-block
  counts of the camera's `rootfs` (0x6920000) and `app` (0xDA0000) NAND slots. An image
  that attaches here therefore also fits on the camera; if it does not, `ubiformat`
  says so instead of you finding out at flash time.
* Both services need `privileged` — one to `insmod`, the other to `mount`.
* `/dev` is bind-mounted from the VM so the container sees `/dev/ubi0_0` as the kernel
  creates it.
* To undo everything: `colima restart` (unloads the modules),
  `docker compose down --rmi local` (drops the images), and remove the `mounts:` entry
  from `~/.colima/default/colima.yaml` if you no longer want `/Volumes/W` shared.

---

## 7. Layout of this repo

```
README.md                 this file
configuration.md          findings: the rootfs configuration
application-findings.md   findings: application.dtb, the ISP tuning database
cardv-findings.md         findings: the camera application and how to modify it
cardv-re.md               findings: tracing settings, menus and the config flow
tools/re/                 cross-reference helpers for the stripped cardv binary
tools/cfgapply/           LD_PRELOAD shim that applies viofo_config.ini edits
docker/re.Dockerfile      GNU binutils + capstone, for disassembling cardv
src/checksum.rs           the Novatek checksum, and solving for a corrective word
src/format.rs             NVTPACK header, partition table, CKSM / uImage / build-tag containers
src/manifest.rs           manifest.toml schema
src/main.rs               info / verify / unpack / pack
tools/mkelf.py            wrap a raw AArch64 blob in an ELF at a given load address
docker-compose.yml        the two services below
docker/kmod.Dockerfile    MTD/UBI modules for the colima VM's kernel
docker/ubi.Dockerfile     Alpine + mtd-utils
tools/kmod-load.sh        insmod ubi/ubifs/nandsim, create the simulated NAND
tools/ubi-container.sh    format, attach, mount, and provide `rebuild`
```

Licensed under Apache-2.0 (see `LICENSE`).
