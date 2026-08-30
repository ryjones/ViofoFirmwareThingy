//! `viofo-fw` -- split and rebuild Novatek NVTPACK dashcam firmware.
//!
//! Written against the VIOFO A329S `FWA329S.bin` (Novatek NA51102, "CarDV").
//! Everything the tool knows about the format was recovered from the image
//! itself and from the U-Boot partition it carries; see README.md.

mod checksum;
mod format;
mod manifest;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use format::*;
use manifest::{Container, Manifest, Partition};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "viofo-fw", version, about = "Unpack/repack Novatek NVTPACK firmware (VIOFO A329S)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the partition table and container headers.
    Info { firmware: PathBuf },
    /// Check every Novatek checksum in the image.
    Verify { firmware: PathBuf },
    /// Split a firmware image into per-partition files plus a manifest.
    Unpack {
        firmware: PathBuf,
        /// Output directory (created if absent).
        #[arg(short, long, default_value = "unpacked")]
        out: PathBuf,
        /// Keep container headers in the partition files instead of stripping
        /// them into the manifest.
        #[arg(long)]
        raw: bool,
        /// Overwrite a non-empty output directory.
        #[arg(short, long)]
        force: bool,
    },
    /// Rebuild a firmware image from a manifest directory.
    Pack {
        /// Directory containing manifest.toml (or the manifest file itself).
        dir: PathBuf,
        #[arg(short, long, default_value = "firmware.bin")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Info { firmware } => cmd_info(&firmware),
        Cmd::Verify { firmware } => cmd_verify(&firmware),
        Cmd::Unpack { firmware, out, raw, force } => cmd_unpack(&firmware, &out, raw, force),
        Cmd::Pack { dir, out } => cmd_pack(&dir, &out),
    }
}

// ---------------------------------------------------------------------------

fn describe(part: &[u8]) -> String {
    if let Some(c) = CksmHeader::parse(part) {
        format!(
            "CKSM v0x{:08x}{} payload 0x{:x} @0x{:x}",
            c.version,
            if c.version == CKSM_VERSION { "" } else { " (unknown)" },
            c.data_length,
            c.data_offset
        )
    } else if let Some(u) = UImageHeader::parse(part) {
        format!(
            "uImage \"{}\" os={} arch={} type={} comp={} load=0x{:x}",
            u.name, u.os, u.arch, u.img_type, u.comp, u.load
        )
    } else if part.len() >= 4 && part[..4] == [0xD0, 0x0D, 0xFE, 0xED] {
        let total = u32::from_be_bytes(part[4..8].try_into().unwrap());
        format!("device tree blob, totalsize 0x{total:x}")
    } else if let Some(t) = find_nvt_tag(part) {
        format!(
            "raw, nvt tag \"{}\" ver \"{}\" date \"{}\" @0x{:x}",
            t.tag, t.version, t.date, t.offset
        )
    } else {
        "raw".to_string()
    }
}

fn cmd_info(path: &Path) -> Result<()> {
    let data = read_file(path)?;
    let fw = FirmwareHeader::parse(&data)?;
    println!("{}", path.display());
    println!("  GUID          {}", hex(&data[..16]));
    println!(
        "  version       0x{:08x}{}",
        fw.version,
        if fw.version == FW_VERSION { "" } else { "  (unknown; tool was written against 0x16071515)" }
    );
    println!("  header size   0x{:x}", fw.header_size);
    println!("  partitions    {}", fw.parts.len());
    println!(
        "  total size    0x{:x} ({} bytes; file is {})",
        fw.total_size,
        fw.total_size,
        data.len()
    );
    println!("  chksum        method {} value 0x{:08x}", fw.chksum_method, fw.chksum_value);
    println!();
    println!("  {:<3} {:<12} {:<10} {:<10} {:<8} {}", "id", "name", "offset", "size", "cksum", "contents");
    for p in &fw.parts {
        let blob = &data[p.offset as usize..(p.offset + p.size) as usize];
        let ck = checksum::nvt_chksum(blob);
        println!(
            "  {:<3} {:<12} 0x{:08x} 0x{:08x} {:<8} {}",
            p.id,
            partition_name(p.id),
            p.offset,
            p.size,
            if ck == 0 { "ok".into() } else { format!("0x{ck:04x}") },
            describe(blob)
        );
    }
    Ok(())
}

fn cmd_verify(path: &Path) -> Result<()> {
    let data = read_file(path)?;
    let fw = FirmwareHeader::parse(&data)?;
    let mut bad = 0;

    if fw.total_size != data.len() as u64 {
        println!("FAIL  header total_size 0x{:x} != file size 0x{:x}", fw.total_size, data.len());
        bad += 1;
    } else {
        println!("ok    header total_size 0x{:x}", fw.total_size);
    }

    let file_ck = checksum::nvt_chksum(&data);
    if file_ck == 0 {
        println!("ok    whole-image checksum");
    } else {
        println!("FAIL  whole-image checksum residue 0x{file_ck:04x} (must be 0)");
        bad += 1;
    }

    for p in &fw.parts {
        let blob = &data[p.offset as usize..(p.offset + p.size) as usize];
        let name = partition_name(p.id);
        let ck = checksum::nvt_chksum(blob);
        // fdt and kernel ship without a Novatek corrective word: the kernel is
        // covered by the uImage CRCs instead, and the dtb by nothing at all.
        let has_slot = CksmHeader::parse(blob).is_some() || find_nvt_tag(blob).is_some();
        match (ck == 0, has_slot) {
            (true, _) => println!("ok    partition {} ({}) checksum", p.id, name),
            (false, false) => println!(
                "note  partition {} ({}) has no Novatek checksum slot (residue 0x{ck:04x}) -- as shipped",
                p.id, name
            ),
            (false, true) => {
                println!("FAIL  partition {} ({}) checksum residue 0x{ck:04x}", p.id, name);
                bad += 1;
            }
        }
        if let Some(u) = UImageHeader::parse(blob) {
            let payload = &blob[UIMAGE_HDR_LEN..];
            let dcrc = u32::from_be_bytes(blob[0x18..0x1C].try_into().unwrap());
            let want = crc32fast::hash(payload);
            if dcrc == want {
                println!("ok    partition {} ({}) uImage data CRC", p.id, name);
            } else {
                println!("FAIL  partition {} ({}) uImage data CRC 0x{dcrc:08x} != 0x{want:08x}", p.id, name);
                bad += 1;
            }
            let mut h = blob[..UIMAGE_HDR_LEN].to_vec();
            let hcrc = u32::from_be_bytes(h[0x04..0x08].try_into().unwrap());
            h[0x04..0x08].fill(0);
            let want = crc32fast::hash(&h);
            if hcrc == want {
                println!("ok    partition {} ({}) uImage header CRC", p.id, name);
            } else {
                println!("FAIL  partition {} ({}) uImage header CRC 0x{hcrc:08x} != 0x{want:08x}", p.id, name);
                bad += 1;
            }
            let _ = u;
        }
    }

    if bad > 0 {
        bail!("{bad} check(s) failed");
    }
    println!("\nall checks passed");
    Ok(())
}

// ---------------------------------------------------------------------------

/// File extension to give a partition's payload, so the split tree is obvious
/// to whatever you point at it next.
fn payload_ext(id: u32, blob: &[u8], raw: bool) -> &'static str {
    if raw {
        return "bin";
    }
    if CksmHeader::parse(blob).is_some() {
        return "ubi";
    }
    if UImageHeader::parse(blob).is_some() {
        return "lzma";
    }
    if blob.len() >= 4 && blob[..4] == [0xD0, 0x0D, 0xFE, 0xED] {
        return "dtb";
    }
    let _ = id;
    "bin"
}

fn cmd_unpack(path: &Path, out: &Path, raw: bool, force: bool) -> Result<()> {
    let data = read_file(path)?;
    let fw = FirmwareHeader::parse(&data)?;

    if out.exists() {
        let empty = std::fs::read_dir(out)?.next().is_none();
        if !empty && !force {
            bail!("{} is not empty (pass --force to overwrite)", out.display());
        }
    }
    std::fs::create_dir_all(out)?;

    // Derive the alignment actually used by this image, so a round trip that
    // changes nothing reproduces the file byte for byte.
    let align = derive_align(&fw).unwrap_or(DEFAULT_ALIGN);

    let mut parts = Vec::new();
    for p in &fw.parts {
        let blob = &data[p.offset as usize..(p.offset + p.size) as usize];
        let name = partition_name(p.id);
        let ext = payload_ext(p.id, blob, raw);
        let file = format!("{:02}-{}.{}", p.id, name, ext);

        let mut entry = Partition {
            id: p.id,
            name: name.clone(),
            file: file.clone(),
            container: Container::Raw,
            nvt_tag_offset: None,
            cksm: None,
            uimage: None,
        };
        let payload: &[u8] = if raw {
            if let Some(t) = find_nvt_tag(blob) {
                entry.nvt_tag_offset = Some(t.offset as u32);
            }
            blob
        } else if let Some(c) = CksmHeader::parse(blob) {
            entry.container = Container::Cksm;
            entry.cksm = Some(manifest::Cksm {
                version: c.version,
                field_08: c.field_08,
                data_offset: c.data_offset,
                field_18: c.field_18,
                field_1c: c.field_1c,
            });
            let s = c.data_offset as usize;
            let e = s + c.data_length as usize;
            if e > blob.len() {
                bail!("partition {} ({name}): CKSM payload runs past the partition", p.id);
            }
            &blob[s..e]
        } else if let Some(u) = UImageHeader::parse(blob) {
            entry.container = Container::Uimage;
            entry.uimage = Some(manifest::UImage {
                name: u.name.clone(),
                time: u.time,
                load: u.load,
                entry: u.entry,
                os: u.os,
                arch: u.arch,
                img_type: u.img_type,
                comp: u.comp,
            });
            let len = u32::from_be_bytes(blob[0x0C..0x10].try_into().unwrap()) as usize;
            let e = UIMAGE_HDR_LEN + len;
            if e > blob.len() {
                bail!("partition {} ({name}): uImage payload runs past the partition", p.id);
            }
            &blob[UIMAGE_HDR_LEN..e]
        } else {
            if let Some(t) = find_nvt_tag(blob) {
                entry.nvt_tag_offset = Some(t.offset as u32);
            }
            blob
        };

        std::fs::write(out.join(&file), payload)
            .with_context(|| format!("writing {}", out.join(&file).display()))?;
        println!(
            "  {:<3} {:<12} -> {:<22} {:>10} bytes  {}",
            p.id,
            name,
            file,
            payload.len(),
            describe(blob)
        );
        parts.push(entry);
    }

    let m = Manifest {
        firmware: manifest::Firmware {
            source: path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            version: fw.version,
            header_size: fw.header_size,
            chksum_method: fw.chksum_method,
            align,
        },
        partitions: parts,
    };
    let toml = m.to_toml()?;
    let mpath = out.join("manifest.toml");
    std::fs::write(&mpath, header_comment() + &toml)?;
    println!("\nwrote {}", mpath.display());
    Ok(())
}

fn header_comment() -> String {
    "\
# viofo-fw manifest -- rebuild with:  viofo-fw pack <this directory> -o new.bin
#
# `container` says how the partition is wrapped when packing:
#   raw    -- the file is the whole partition; if nvt_tag_offset is set, the
#             Novatek build tag's size and checksum words are re-solved.
#   cksm   -- a 0x40-byte CKSM header is prepended and its checksum solved.
#   uimage -- a 0x40-byte legacy uImage header is prepended, CRCs recomputed.
#
# Partition offsets, sizes, the image total size and the whole-image checksum
# are all recomputed on pack; they are deliberately not stored here.
"
    .to_string()
}

/// Recover the partition alignment used by an existing image.
fn derive_align(fw: &FirmwareHeader) -> Option<u64> {
    let first = fw.first_partition_offset();
    let mut a = DEFAULT_ALIGN;
    loop {
        let ok = fw.parts.iter().all(|p| p.offset >= first && (p.offset - first) % a == 0);
        if ok {
            return Some(a);
        }
        a /= 2;
        if a < 2 {
            return Some(1);
        }
    }
}

// ---------------------------------------------------------------------------

fn cmd_pack(dir: &Path, out: &Path) -> Result<()> {
    let (mpath, base) = if dir.is_dir() {
        (dir.join("manifest.toml"), dir.to_path_buf())
    } else {
        (dir.to_path_buf(), dir.parent().unwrap_or(Path::new(".")).to_path_buf())
    };
    let text = std::fs::read_to_string(&mpath)
        .with_context(|| format!("reading {}", mpath.display()))?;
    let m = Manifest::from_toml(&text)?;

    let header_size = m.firmware.header_size as usize;
    let count = m.partitions.len();
    let first_off = header_size as u64 + count as u64 * 12;

    let mut image: Vec<u8> = vec![0u8; first_off as usize];
    let mut table: Vec<(u32, u64, u64)> = Vec::new();

    for p in &m.partitions {
        let payload = read_file(&base.join(&p.file))?;
        let mut blob = match p.container {
            Container::Raw => payload,
            Container::Cksm => {
                let c = p.cksm.as_ref().unwrap();
                CksmHeader {
                    version: c.version,
                    field_08: c.field_08,
                    data_offset: c.data_offset,
                    data_length: payload.len() as u32,
                    field_18: c.field_18,
                    field_1c: c.field_1c,
                }
                .build(&payload)
            }
            Container::Uimage => {
                let u = p.uimage.as_ref().unwrap();
                UImageHeader {
                    time: u.time,
                    load: u.load,
                    entry: u.entry,
                    os: u.os,
                    arch: u.arch,
                    img_type: u.img_type,
                    comp: u.comp,
                    name: u.name.clone(),
                }
                .build(&payload)
            }
        };

        if p.container == Container::Raw {
            if let Some(t) = p.nvt_tag_offset {
                fix_nvt_tag(&mut blob, t as usize).with_context(|| {
                    format!("partition {} ({}) build tag", p.id, p.name)
                })?;
            }
        }

        // Pad to the next aligned start, measured from the first partition.
        let cur = image.len() as u64;
        let rel = cur - first_off;
        let padded = first_off + rel.div_ceil(m.firmware.align) * m.firmware.align;
        image.resize(padded as usize, 0);

        let off = image.len() as u64;
        if off + blob.len() as u64 > u32::MAX as u64 {
            bail!("partition {} ({}) would push the image past 4 GiB", p.id, p.name);
        }
        image.extend_from_slice(&blob);
        println!(
            "  {:<3} {:<12} <- {:<22} 0x{:08x} + 0x{:08x}",
            p.id,
            p.name,
            p.file,
            off,
            blob.len()
        );
        table.push((p.id, off, blob.len() as u64));
    }

    // Fixed header.
    image[..16].copy_from_slice(&FW_GUID);
    image[0x10..0x14].copy_from_slice(&m.firmware.version.to_le_bytes());
    image[0x14..0x18].copy_from_slice(&(header_size as u32).to_le_bytes());
    image[0x18..0x1C].copy_from_slice(&(count as u32).to_le_bytes());
    let total = image.len() as u32;
    image[0x1C..0x20].copy_from_slice(&total.to_le_bytes());
    image[0x20..0x24].copy_from_slice(&m.firmware.chksum_method.to_le_bytes());

    // Partition table.
    for (i, (id, off, size)) in table.iter().enumerate() {
        let e = header_size + i * 12;
        image[e..e + 4].copy_from_slice(&(*off as u32).to_le_bytes());
        image[e + 4..e + 8].copy_from_slice(&(*size as u32).to_le_bytes());
        image[e + 8..e + 12].copy_from_slice(&id.to_le_bytes());
    }

    let ck = checksum::solve(&mut image, FW_CHKSUM_OFF);
    debug_assert!(checksum::is_valid(&image));

    std::fs::write(out, &image).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "\nwrote {} ({} bytes, image checksum word 0x{ck:04x})",
        out.display(),
        image.len()
    );
    Ok(())
}
