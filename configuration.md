# What's in the rootfs configuration

Everything below was read out of `07-rootfs.ubi` (mount it with
`docker compose run --rm ubi`, see §6 of the [README](README.md)). Sizes and
paths are from the shipped image; nothing here is inferred from documentation.

## `/etc` inventory

```
application.dtb     293 B    vestigial, see "stale files" below
bootchartd.conf      43 B
firmware.info        75 B    vestigial
fstab               309 B
group / passwd    10/29 B
hostname              7 B    "NVTEVM"
hosts                25 B
inetd.conf         3016 B
init.d/                      19 scripts, the boot chain
inittab            3478 B
localtime -> timezone/localtime
mdev.conf           989 B  + mdev-script/
network/interfaces  409 B
profile             763 B
profile_prjcfg     6481 B    <- the interesting one
resolv.conf          24 B
services            666 B
sysctl.conf        2218 B
timezone/
udhcpd.conf        4307 B
udhcpdw.conf       4234 B
wifiap_wpa2.conf   2387 B
wpa_supplicant.conf 110 B
```

## `/etc/profile_prjcfg` — the project configuration

This is the build-time product definition, emitted by Novatek's SDK and sourced
at runtime by both `/etc/profile` and `/etc/init.d/rcS`. It is the single most
informative file in the image: 6.5 KB of `export` lines that describe the whole
product.

### Build identity

```sh
export MODEL=/home/hebo/work_disk_300G/project/W49U/nt98530_w49u/\
na51102_linux_sdk/configs/Linux/cfg_530_CARDV_W49U/ModelConfig.mk
export YQ_DX_MODEL="530_CARDV_W49U"
export YQCONFIG_PLATFORM_NAME="YQCONFIG_PLATFORM_NAME_W49U"
export UI_STYLE="CARDV"
export NVT_ROOTFS_ETC="CARDV_B80"
```

The firmware is ODM-built from Novatek's `na51102_linux_sdk` as project
**W49U**, on the NT98530 / NA51102 platform. The `YQCONFIG_*` prefix throughout
the file is the ODM's own namespace.

### Memory map

| symbol | address | size |
|---|---|---|
| `BOARD_DRAM` | `0x00000000` | `0x80000000` (2 GiB) |
| `BOARD_FDT` | `0x00100000` | `0x00100000` |
| `BOARD_SHMEM` | `0x00A00000` | `0x00100000` |
| `BOARD_LOADER` | `0x01000000` | `0x00100000` |
| `BOARD_ATF` | `0x01F00000` | `0x00100000` |
| `BOARD_LINUXTMP` | `0x02000000` | `0x72F00000` |
| `BOARD_ALL_IN_ONE` | `0x74F00000` | `0x07800000` |
| `BOARD_KERNEL_IMG` | `0x7C700000` | `0x01900000` |
| `BOARD_UBOOT` | `0x7E000000` | `0x01000000` |
| `BOARD_LINUX` | `0x00000000` | `0x18000000` (384 MiB given to Linux) |

`BOARD_UBOOT_ADDR` is `0x7E000000` — the same load address the `uboot`
partition's own header carries, and the one to hand `tools/mkelf.py`.

### Storage

```sh
export EMBMEM="EMBMEM_SPI_NAND"
export EMBMEM_BLK_SIZE="0x20000"
export NVT_ROOTFS_TYPE="NVT_ROOTFS_TYPE_NAND_UBI"
export NVT_ROOTFS_RW_PART_EN="NVT_ROOTFS_RW_PART_EN_OFF"
```

`0x20000` is the 128 KiB erase block the `nandsim` setup in §6 of the README
reproduces, and `NAND_UBI` is the branch of `S07_APP_Overlay` that mounts the
app volume as `ubi1_0` on `/mnt/app`.

### Hardware

```sh
export SENSOR1="sen_imx678"                 # 4 MIPI lanes
export SENSOR2="sen_imx675_242_241x2"       # 2 lanes
export SENSOR3="sen_imx675_242_241x2"       # 2 lanes
export SENSOR4="sen_off"
export SENSOR5="sen_off"
export LCD1="disp_ifdsi_lcd1_st7701s_t23p44"   # ST7701S, DSI
export LCD2="disp_off"
export NVT_SDIO_WIFI="NVT_SDIO_WIFI_BRCM"
export WIFI_BRCM_MDL="WIFI_BRCM_MDL_6611c0_ampk6611c0"   # AMPAK AP6611C
export USB1_TYPE="USB1_HOST"
export GSENSOR_IC="gsensor_xxx"
```

Three camera channels: one **IMX678** and two **IMX675**. That matches
`application.dtb` in the app partition exactly (see
[application-findings.md](application-findings.md)) and the three MIPI
interfaces in `sensor/sensor.dtb`, where channel 0 has 4 data lanes and
channels 1 and 2 have 2 each.

Explicitly off: `NVT_ETHERNET_NONE`, `USB2_NONE`, `HDMI_OFF`, `TSE_DISABLE`,
`EIS_DISABLE`, `GYRO_NONE`, `BT_MDL_NONE`, `touchscreen_off`,
`NVT_USB_WIFI_NONE`, `NVT_USB_4G_NONE`, `NVT_OPTEE_INSTALL=DISABLE`.

`GSENSOR_IC="gsensor_xxx"` is a placeholder that will not resolve to a real
module path — see the note on `S10_SysInit2` below.

### Userspace

```sh
export NVT_CFG_APP="hfs lviewd nvtrtspd mem sw_dbg i2c_access cardv BlueTooth"
export NVT_CFG_APP_EXTERNAL="hostapd wpa_supplicant wireless_tool dhd_priv \
memtester iperf-3 libiconv dosfstools exfat-utils rtwpriv"
export AAC_PLUGIN="AAC_PLUGIN_FAAC"
export NVT_CURL_SSL="NVT_CURL_SSL_OPENSSL"
export NVT_BINARY_FILE_STRIP="yes"
```

`cardv` is the camera application — `/usr/bin/cardv`, 14,052,992 bytes,
stripped. `nvtrtspd` is an RTSP server and `lviewd` a live-view daemon.
`BlueTooth` is listed even though `BT_MDL="BT_MDL_NONE"`.

### Kernel and U-Boot configs

```sh
export NVT_CFG_KERNEL_CFG="na51102_a64_evb_cardv_defconfig_release"
export NVT_CFG_UBOOT_CFG="nvt-na51102_a64_nand_defconfig"
export NVT_LINUX_SMP="NVT_LINUX_SMP_ON"
```

### The `YQCONFIG_*` feature flags

About 60 of them, every one set to `"yes"` — the file lists only what is
enabled, so it reads as a feature manifest for the product:

> parking monitor (+ standby), GPS with geofence and time-zone/DST handling,
> time lapse, motion detection, crash/EMR trigger, G-sensor, PIP recording,
> dynamic bitrate adjustment, VPE dewarp, licence-plate and free-text stamping,
> voice prompts, AI speech, temperature detection, slow-card detection, super-
> capacitor power handling, audio amplifier, audio noise suppression and
> resampling, IR light, Wi-Fi auto-off / connection state / app authorisation,
> LCD sleep, watchdog, pstore with CRC check, USB and external storage, 12/24 h
> clock, format reminder, menu config file support, and the `W49_NEW_UI`.

**Most of these do nothing at runtime.** Only three are read by any script in
the image — `S10_SysInit2` uses `YQCONFIG_GSENSOR_FUNCTION_SUPPORT` and
`YQCONFIG_WDTCHDOG_FUNCTION_SUPPORT` to decide whether to `insmod` a driver,
and tests `YQCONFIG_TOUCH_FUNCTION_SUPPORT`, which `profile_prjcfg` never sets
at all. The rest were compile-time switches for `cardv`; editing them here will
not turn a feature on. The `NVT_*`, `EMBMEM`, `SENSORn` and `LCDn` variables
*are* live — `S05_FS_Overlay`, `S07_APP_Overlay`, `S14_MMC_FS`, `S25_Net`,
`S10_SysInit2` and `mdev-script/autosd.sh` all branch on them.

## Boot chain

`/etc/inittab`:

```
::sysinit:sh /etc/init.d/rcS
ttyS0::respawn:-/bin/login -f root
::restart:/sbin/init
::ctrlaltdel:/sbin/reboot
::shutdown:/etc/init.d/rcK
```

`/etc/init.d/` runs in order:

| script | what it does |
|---|---|
| `S00_PreReady` | early setup |
| `S05_FS_Overlay` | writable overlay for `/etc`, `/var`, `/lib/modules` on `/mnt/overlay_rw0` |
| `S06_SDA_Detect` | storage detection |
| `S07_APP_Overlay` | attaches the app UBI volume and mounts it on `/mnt/app` |
| `S10_SysInit2` | loads drivers (g-sensor, watchdog, touchscreen) |
| `S14_MMC_FS` | SD card filesystem |
| `S15_NvtAppInit` | starts `inetd`, `crond`, `isp_demon`, then the camera app |
| `S25_Net` | Wi-Fi / Ethernet bring-up, branching on `NVT_SDIO_WIFI` |
| `S98_Pstore` | crash log store |
| `S99_Sysctl` | applies `/etc/sysctl.conf` |

Plus `BACK_S07_SysInit`, `BS_Net_eth`, `BS_Net_wifi`, `BS_Net_wifiap`,
`BS_Net_wifiap8189ftv`, `K00_Sys`, `K99_Sys`, `rcS`, `rcK`.

`/etc/profile` also enables core dumps unconditionally
(`ulimit -c unlimited`, pattern `/var/log/core-%e-%p-%t`) — useful when
debugging a patched `cardv`.

## Accounts and network daemons

Factual observations about the shipped image. Whether any of it is reachable
depends on which interface the application actually brings up at runtime, which
is inside `cardv` and not traced here.

* `/etc/passwd` is `root::0:0:root:/root:/bin/sh` — the password field is empty,
  and there is no `/etc/shadow`.
* `/etc/inittab` auto-logs the serial console in as root (`login -f root`).
* `S15_NvtAppInit` starts `inetd` unconditionally, at line 5, before anything
  else.
* `/etc/inetd.conf` has these services uncommented:

  | port | service | as | notes |
  |---|---|---|---|
  | 7 | `echo` tcp+udp | root | internal |
  | 13 | `daytime` tcp+udp | root | internal |
  | 37 | `time` tcp+udp | root | internal |
  | 21 | `ftpd -w /mnt/sd` | root | `-w` = writable |
  | 69 | `tftpd -l -c /home` | root | `-c` = create allowed |

* `telnetd` appears in `S25_Net` but is commented out.
* `/etc/network/interfaces` gives `wlan0` the static address `192.168.1.2`.

## Stale files — do not be misled by these

Several files are Novatek SDK samples that were never updated for this product.
They describe hardware and software this camera does not have.

* **`/etc/firmware.info`** says
  `SDK_VER="NVT_NT96660_Linux_V0.4.8"`, `BUILDDATE="Tue Mar 1 18:25:28 CST 2016"`.
  Wrong SoC (NT96660, not NA51102) and eight years stale. The real build stamp
  is in the `uboot` partition's Novatek tag: `20260815`.
* **`/etc/application.dtb`** (293 bytes) binds `sensor@1` to `nvt_sen_imx291`
  and `sensor@2` to `nvt_sen_imx323` — neither sensor is in this camera. The
  real ISP configuration is the 317 KB `application.dtb` inside the *app*
  partition.
* **`/etc/wpa_supplicant.conf`** contains `ssid="MYSSID"` / `psk="myssidpwd"`.
* **`/etc/wifiap_wpa2.conf`** contains `ssid=680apwpa2`,
  `wpa_passphrase=12345678`, and `driver=rtl871xdrv` — a Realtek driver string
  on a board configured for Broadcom Wi-Fi. The AP name and passphrase the
  camera actually uses are set from the user's settings at runtime, not from
  this file.
* **`/etc/udhcpd.conf`** serves a `192.168.0.x` pool on `eth0`, but
  `NVT_ETHERNET="NVT_ETHERNET_NONE"`.
* **`/etc/hostname`** is `NVTEVM`, the Novatek evaluation-board default.
* **`/etc/resolv.conf`** is `nameserver 192.168.0.1`.
* The `sensor/` directory in the app partition ships all 135 sensor drivers
  from the SDK; only `sen_imx678_530.cfg` is relevant, and there is no
  `imx675` cfg there at all.

## Where to change things

* Startup behaviour, extra daemons, disabling `inetd`: `/etc/init.d/*`,
  particularly `S15_NvtAppInit`.
* Kernel module loading: `S10_SysInit2`.
* Network addressing: `/etc/network/interfaces`, `S25_Net`, `BS_Net_*`.
* Product feature flags: mostly *not* here — they are compiled into
  `/usr/bin/cardv`.
* Image quality: not here either — see
  [application-findings.md](application-findings.md).
