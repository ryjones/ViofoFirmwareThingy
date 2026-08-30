# What's in `application.dtb`

`/application.dtb` in the **app** partition (`09-app.ubi`), 316,953 bytes. It is
the camera's **ISP tuning database** — image-quality parameters, not code and
not user-facing settings. Mount the volume with `docker compose run --rm ubi`
(§6 of the [README](README.md)); the file is at `~/viofo/mnt/app/application.dtb`.

Not to be confused with `/etc/application.dtb` in the *rootfs*, which is a
293-byte SDK leftover naming sensors this camera does not have — see
[configuration.md](configuration.md).

## Two things to know before editing it

**The cells are little-endian.** A flattened device tree stores integers
big-endian; this file does not. Checked across all 429 `size`/`data` pairs in
the file: 429 match when the `size` cell is read little-endian, 0 match
big-endian. Novatek wrote host-order values into an otherwise standard DTB
container, so `dtc -I dtb -O dts` prints every number byte-swapped. (Curiously
`sensor/sensor.dtb` next door *does* use proper big-endian cells, so the
convention is not even consistent within the directory.)

**`version-info` stamps each profile's binary struct layout**, and differs by
profile class:

| profile class | `version-info` bytes |
|---|---|
| `iq` and `iq_ldc` | `00 05 00 01` |
| `iq_cap` | `00 02 00 01` |
| `ae` | `00 01 00 01` |
| `awb` | `00 00 00 01` |

The `data` blobs are packed Novatek IQ structs. Without the SDK headers you are
tuning them by careful diffing, not by reading field names.

## Structure

One root node, `/isp`, holding four sections:

```
/isp/sensor@0     IMX678   -> ae / awb / iq profile paths (+ _hdr, group1, _cap, _ldc)
/isp/sensor@1     IMX675
/isp/sensor@2     IMX675, "fisheye" tuning
/isp/ae/*         9 auto-exposure profiles
/isp/awb/*        9 auto-white-balance profiles
/isp/iq/*        13 image-quality profiles
```

The `sensor@N` nodes contain no data — only string paths binding each camera
channel to its profiles:

```dts
sensor@0 {
    ae_path      = "/isp/ae/imx678_ae_0";
    awb_path     = "/isp/awb/imx678_awb_0";
    iq_path      = "/isp/iq/imx678_iq_0";
    ae_path_hdr  = "/isp/ae/imx678_ae_0_hdr";
    awb_path_hdr = "/isp/awb/imx678_awb_0_hdr";
    iq_path_hdr  = "/isp/iq/imx678_iq_0_hdr";
    iq_ldc_path  = "/isp/iq/imx678_iq_ldc_0";
    iq_cap_path  = "/isp/iq/imx678_iq_0_cap";
    group1 { ... a second binding set ... };
};
```

The three-channel layout agrees with `SENSOR1="sen_imx678"` and
`SENSOR2`/`SENSOR3="sen_imx675_242_241x2"` in `/etc/profile_prjcfg`, and with
`sensor/sensor.dtb`, which gives channel 0 four MIPI data lanes and channels 1
and 2 two lanes each.

Suffixes: `_hdr` is the HDR/SHDR variant, `_cap` the still-capture profile,
`_ldc` lens distortion correction (present only for the IMX678 channel), and
`group1` an alternate profile set selected at runtime.

The whole file uses exactly **11 distinct property names**: the eight `*_path`
bindings plus `version-info`, `size` and `data`. Every profile is a list of
named blocks, each a `{ size, data }` pair.

## Tuning blocks

**AE** (12): `ae_expect_lum`, `ae_la_clamp`, `ae_over_exposure`,
`ae_convergence`, `ae_curve_gen_movie`, `ae_curve_gen_photo`,
`ae_meter_window`, `ae_lum_gamma`, `ae_shdr`, `ae_shdr_meter`, `ae_shdr_hbs`,
`ae_iris`

**AWB** (9): `awb_th`, `awb_lv`, `awb_ct_weight`, `awb_target`, `awb_ct_info`,
`awb_mwb`, `awb_converge`, `awb_expand_block`, `awb_luma_weight`

**IQ** (26): `iq_ob`, `iq_nr`, `iq_cfa`, `iq_va`, `iq_post_va`, `iq_tone`,
`iq_gamma`, `iq_ccm`, `iq_ccm_ext`, `iq_color`, `iq_contrast`, `iq_edge`,
`iq_3dnr`, `iq_pfr`, `iq_pfr_ext`, `iq_wdr`, `iq_wdr_enh`, `iq_defog`,
`iq_shdr`, `iq_companding`, `iq_rgbir`, `iq_rgbir_enh`, `iq_post_sharpen_1`,
`iq_post_sharpen_2`, `iq_ycurve`, `iq_cst`

## Where the bytes go

923 properties; 293,324 bytes (93% of the file) sit in `data` blobs, all of
them under `/isp`. Largest blocks, summed across all profiles:

| block | bytes |
|---|---|
| `iq_3dnr` | 80,360 |
| `iq_nr` | 46,480 |
| `iq_color` | 17,720 |
| `iq_gamma` | 15,640 |
| `iq_edge` | 15,080 |
| `iq_cfa` | 13,640 |
| `iq_tone` | 10,680 |
| `iq_companding` | 9,080 |
| `iq_shdr` | 8,800 |
| `iq_post_sharpen_1` | 6,880 |
| `iq_post_sharpen_2` | 5,880 |
| `iq_ccm` | 5,760 |
| `iq_ycurve` | 5,200 |
| `iq_wdr` | 3,880 |

Temporal and spatial noise reduction alone are 43% of the file.

The one small outlier is the LDC profile, a single block:

```dts
imx678_iq_ldc_0 {
    version-info = <0x50001>;          /* raw bytes 00 05 00 01 */
    iq_ldc {
        size = <0x1c010000>;           /* little-endian: 284 */
        data = [01000000f4010000f401000000040000ffff000012fd0000...];
    };
};
```

## Neighbours in the app partition

```
application.dtb   316,953 B   this file
motor/                        3 lens-motor drivers: AN41908, MS41949, TI8833
sensor/                       135 SDK sensor .cfg files + sensor.dtb
```

`motor/` is SDK leftovers — a dashcam has no motorised lens. `sensor/` ships
the SDK's entire driver catalogue; only `sen_imx678_530.cfg` is relevant here,
and notably there is **no `imx675` cfg at all**, even though `sensor@1` and
`sensor@2` reference `imx675_*` profiles. Either those channels' drivers are
built into `/usr/bin/cardv` or the profile names are labels rather than parts.

`sensor/sensor.dtb` defines only the three MIPI CSI interfaces (`ssenif@0..2`);
its `sensor/sen_cfg` node is empty, so sensor selection happens at runtime
rather than from that file.

## Reading it yourself

`dtc` will byte-swap the numbers (see above). A parser that dumps the tree
without reinterpreting the cells:

```python
import struct
d = open('unpacked/application.dtb', 'rb').read()
magic, total, off_struct, off_strings, _, _, _, _, size_str, size_struct = \
    struct.unpack('>10I', d[:40])
strings = d[off_strings:off_strings + size_str]
name = lambda o: strings[o:strings.index(b'\0', o)].decode()

p, depth, end = off_struct, 0, off_struct + size_struct
while p < end:
    tok, = struct.unpack('>I', d[p:p+4]); p += 4
    if tok == 1:                                   # FDT_BEGIN_NODE
        n = d[p:d.index(b'\0', p)].decode()
        p = (d.index(b'\0', p) + 4) & ~3
        print('  ' * depth + (n or '/') + ' {'); depth += 1
    elif tok == 2:                                 # FDT_END_NODE
        depth -= 1; print('  ' * depth + '};')
    elif tok == 3:                                 # FDT_PROP
        ln, noff = struct.unpack('>II', d[p:p+8]); p += 8
        val = d[p:p+ln]; p = (p + ln + 3) & ~3
        nm = name(noff)
        if nm == 'size':                           # little-endian, unlike DT spec
            txt = str(struct.unpack('<I', val)[0])
        elif val and val[-1] == 0 and all(32 <= c < 127 or c == 0 for c in val):
            txt = '"' + '","'.join(x.decode() for x in val.split(b'\0')[:-1]) + '"'
        else:
            txt = f'[{ln} bytes]'
        print('  ' * depth + f'{nm} = {txt};')
    elif tok == 9:                                 # FDT_END
        break
```

After editing, put the file back with `rebuild app` inside the container and
then `viofo-fw pack unpacked -o FWA329S.bin` on the host.
