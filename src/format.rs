//! On-disk layout of a Novatek NVTPACK firmware image (`NVTPACK_FW_HDR2`).
//!
//! ```text
//! 0x00  16  GUID          D6012E07-10BC-4F91-B28A-352F82261A50
//!                         (bytes 072E01D6 BC10 914F B28A352F82261A50)
//! 0x10  u32 version       0x16071515
//! 0x14  u32 header_size   0x80 -- also the offset of the partition table
//! 0x18  u32 part_count
//! 0x1C  u32 total_size    == file size
//! 0x20  u32 chksum_method 0
//! 0x24  u32 chksum_value  low 16 bits are the corrective word
//! 0x28  ..  zero padding up to header_size
//! 0x80      partition table: part_count * { u32 offset; u32 size; u32 id }
//! ```
//!
//! The whole file must satisfy `nvt_chksum(file) == 0`.

use anyhow::{bail, Context, Result};

pub const FW_GUID: [u8; 16] = [
    0x07, 0x2E, 0x01, 0xD6, 0xBC, 0x10, 0x91, 0x4F, 0xB2, 0x8A, 0x35, 0x2F, 0x82, 0x26, 0x1A, 0x50,
];
pub const FW_VERSION: u32 = 0x1607_1515;
/// Offset of the 32-bit corrective checksum word in the firmware header.
pub const FW_CHKSUM_OFF: usize = 0x24;
/// Partitions start on a 0x40 boundary measured from the first partition offset.
pub const DEFAULT_ALIGN: u64 = 0x40;

/// Partition id -> name, taken from the `nvtpack/index` node of the shipped
/// device tree (`ver = "NVTPACK_FW_INI_16072017"`).
pub const PARTITION_NAMES: &[(u32, &str)] = &[
    (0, "loader"),
    (1, "fdt"),
    (2, "fdt.restore"),
    (3, "atf"),
    (4, "uboot"),
    (5, "uenv"),
    (6, "kernel"),
    (7, "rootfs"),
    (8, "pstore"),
    (9, "app"),
    (10, "par"),
];

pub fn partition_name(id: u32) -> String {
    PARTITION_NAMES
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, n)| (*n).to_string())
        .unwrap_or_else(|| format!("id{id}"))
}

fn rd32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

#[derive(Debug, Clone)]
pub struct PartitionEntry {
    pub id: u32,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct FirmwareHeader {
    pub version: u32,
    pub header_size: u32,
    pub chksum_method: u32,
    pub chksum_value: u32,
    pub total_size: u64,
    pub parts: Vec<PartitionEntry>,
}

impl FirmwareHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 0x80 {
            bail!("file too small to be an NVTPACK image ({} bytes)", data.len());
        }
        if data[..16] != FW_GUID {
            bail!(
                "bad NVTPACK GUID: expected {}, found {}",
                hex(&FW_GUID),
                hex(&data[..16])
            );
        }
        let header_size = rd32(data, 0x14);
        let part_count = rd32(data, 0x18);
        let total_size = rd32(data, 0x1C) as u64;
        let tbl = header_size as usize;
        let need = tbl + part_count as usize * 12;
        if data.len() < need {
            bail!("partition table truncated (need {need} bytes, have {})", data.len());
        }
        let mut parts = Vec::with_capacity(part_count as usize);
        for i in 0..part_count as usize {
            let e = tbl + i * 12;
            let offset = rd32(data, e) as u64;
            let size = rd32(data, e + 4) as u64;
            let id = rd32(data, e + 8);
            if offset + size > data.len() as u64 {
                bail!(
                    "partition {i} (id {id}) runs past end of file: 0x{:x}+0x{:x} > 0x{:x}",
                    offset,
                    size,
                    data.len()
                );
            }
            parts.push(PartitionEntry { id, offset, size });
        }
        Ok(Self {
            version: rd32(data, 0x10),
            header_size,
            chksum_method: rd32(data, 0x20),
            chksum_value: rd32(data, FW_CHKSUM_OFF),
            total_size,
            parts,
        })
    }

    /// Offset of the first partition: right after the partition table.
    pub fn first_partition_offset(&self) -> u64 {
        self.header_size as u64 + self.parts.len() as u64 * 12
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

// ---------------------------------------------------------------------------
// CKSM container (rootfs / app partitions)
// ---------------------------------------------------------------------------

/// ```text
/// 0x00 "CKSM"
/// 0x04 u32 version 0x16040719
/// 0x08 u32 (0)
/// 0x0C u32 corrective checksum word
/// 0x10 u32 data offset (0x40)
/// 0x14 u32 data length
/// 0x18 u32 (0)   -- U-Boot verifies over [0x10]+[0x14]+[0x18] bytes
/// 0x1C u32 (9)
/// 0x20 .. 0x3F zero
/// ```
pub const CKSM_MAGIC: &[u8; 4] = b"CKSM";
pub const CKSM_VERSION: u32 = 0x1604_0719;
pub const CKSM_HDR_LEN: usize = 0x40;
pub const CKSM_CHKSUM_OFF: usize = 0x0C;

#[derive(Debug, Clone)]
pub struct CksmHeader {
    pub version: u32,
    pub field_08: u32,
    pub data_offset: u32,
    pub data_length: u32,
    pub field_18: u32,
    pub field_1c: u32,
}

impl CksmHeader {
    pub fn parse(part: &[u8]) -> Option<Self> {
        if part.len() < CKSM_HDR_LEN || &part[..4] != CKSM_MAGIC {
            return None;
        }
        Some(Self {
            version: rd32(part, 0x04),
            field_08: rd32(part, 0x08),
            data_offset: rd32(part, 0x10),
            data_length: rd32(part, 0x14),
            field_18: rd32(part, 0x18),
            field_1c: rd32(part, 0x1C),
        })
    }

    pub fn build(&self, payload: &[u8]) -> Vec<u8> {
        let off = self.data_offset as usize;
        let mut out = vec![0u8; off + payload.len()];
        out[..4].copy_from_slice(CKSM_MAGIC);
        out[0x04..0x08].copy_from_slice(&self.version.to_le_bytes());
        out[0x08..0x0C].copy_from_slice(&self.field_08.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&self.data_offset.to_le_bytes());
        out[0x14..0x18].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        out[0x18..0x1C].copy_from_slice(&self.field_18.to_le_bytes());
        out[0x1C..0x20].copy_from_slice(&self.field_1c.to_le_bytes());
        out[off..].copy_from_slice(payload);
        crate::checksum::solve(&mut out, CKSM_CHKSUM_OFF);
        out
    }
}

// ---------------------------------------------------------------------------
// U-Boot legacy uImage container (kernel partition)
// ---------------------------------------------------------------------------

pub const UIMAGE_MAGIC: u32 = 0x2705_1956;
pub const UIMAGE_HDR_LEN: usize = 0x40;

#[derive(Debug, Clone)]
pub struct UImageHeader {
    pub time: u32,
    pub load: u32,
    pub entry: u32,
    pub os: u8,
    pub arch: u8,
    pub img_type: u8,
    pub comp: u8,
    pub name: String,
}

impl UImageHeader {
    pub fn parse(part: &[u8]) -> Option<Self> {
        if part.len() < UIMAGE_HDR_LEN {
            return None;
        }
        if u32::from_be_bytes(part[0..4].try_into().unwrap()) != UIMAGE_MAGIC {
            return None;
        }
        let name_bytes = &part[0x20..0x40];
        let end = name_bytes.iter().position(|&c| c == 0).unwrap_or(32);
        Some(Self {
            time: u32::from_be_bytes(part[0x08..0x0C].try_into().unwrap()),
            load: u32::from_be_bytes(part[0x10..0x14].try_into().unwrap()),
            entry: u32::from_be_bytes(part[0x14..0x18].try_into().unwrap()),
            os: part[0x1C],
            arch: part[0x1D],
            img_type: part[0x1E],
            comp: part[0x1F],
            name: String::from_utf8_lossy(&name_bytes[..end]).into_owned(),
        })
    }

    pub fn build(&self, payload: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; UIMAGE_HDR_LEN];
        h[0x00..0x04].copy_from_slice(&UIMAGE_MAGIC.to_be_bytes());
        h[0x08..0x0C].copy_from_slice(&self.time.to_be_bytes());
        h[0x0C..0x10].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        h[0x10..0x14].copy_from_slice(&self.load.to_be_bytes());
        h[0x14..0x18].copy_from_slice(&self.entry.to_be_bytes());
        h[0x18..0x1C].copy_from_slice(&crc32fast::hash(payload).to_be_bytes());
        h[0x1C] = self.os;
        h[0x1D] = self.arch;
        h[0x1E] = self.img_type;
        h[0x1F] = self.comp;
        let n = self.name.as_bytes();
        let n = &n[..n.len().min(31)];
        h[0x20..0x20 + n.len()].copy_from_slice(n);
        // header CRC is computed with the hcrc field itself zeroed
        let hcrc = crc32fast::hash(&h);
        h[0x04..0x08].copy_from_slice(&hcrc.to_be_bytes());
        let mut out = h;
        out.extend_from_slice(payload);
        out
    }
}

// ---------------------------------------------------------------------------
// Novatek build tag (atf / uboot partitions)
// ---------------------------------------------------------------------------

/// ```text
/// +0x00 char tag[8]      "bl51102\0" / "ub51102 "
/// +0x08 char version[8]
/// +0x10 char date[8]
/// +0x18 u32  partition size
/// +0x1C u16  magic 0xAA55
/// +0x1E u16  corrective checksum word
/// ```
pub const NVT_TAG_MAGIC: u16 = 0xAA55;
pub const NVT_TAG_LEN: usize = 0x20;
/// Byte offset of the corrective word, relative to the start of the tag.
pub const NVT_TAG_CHKSUM_OFF: usize = 0x1E;

#[derive(Debug, Clone)]
pub struct NvtTag {
    pub offset: usize,
    pub tag: String,
    pub version: String,
    pub date: String,
}

/// Locate the build tag inside a raw partition by looking for the 0xAA55 magic
/// preceded by a size field that matches the partition length.
pub fn find_nvt_tag(part: &[u8]) -> Option<NvtTag> {
    let want = part.len() as u32;
    let mut off = 0usize;
    while off + NVT_TAG_LEN <= part.len() {
        if rd32(part, off + 0x18) == want
            && u16::from_le_bytes(part[off + 0x1C..off + 0x1E].try_into().unwrap())
                == NVT_TAG_MAGIC
        {
            let s = |a: usize, b: usize| {
                let f = &part[off + a..off + b];
                let e = f.iter().position(|&c| c == 0).unwrap_or(f.len());
                String::from_utf8_lossy(&f[..e]).trim().to_string()
            };
            return Some(NvtTag {
                offset: off,
                tag: s(0x00, 0x08),
                version: s(0x08, 0x10),
                date: s(0x10, 0x18),
            });
        }
        off += 4;
    }
    None
}

/// Rewrite the size field and re-solve the corrective word of a build tag.
pub fn fix_nvt_tag(part: &mut [u8], tag_off: usize) -> Result<u16> {
    if tag_off + NVT_TAG_LEN > part.len() {
        bail!("nvt_tag_offset 0x{tag_off:x} is outside the partition");
    }
    let size = part.len() as u32;
    part[tag_off + 0x18..tag_off + 0x1C].copy_from_slice(&size.to_le_bytes());
    part[tag_off + 0x1C..tag_off + 0x1E].copy_from_slice(&NVT_TAG_MAGIC.to_le_bytes());
    Ok(crate::checksum::solve(part, tag_off + NVT_TAG_CHKSUM_OFF))
}

pub fn read_file(path: &std::path::Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cksm_roundtrip_and_checksum() {
        let h = CksmHeader {
            version: CKSM_VERSION,
            field_08: 0,
            data_offset: 0x40,
            data_length: 0,
            field_18: 0,
            field_1c: 9,
        };
        let payload: Vec<u8> = (0u32..4096).map(|i| (i % 253) as u8).collect();
        let part = h.build(&payload);
        assert_eq!(part.len(), CKSM_HDR_LEN + payload.len());
        assert!(crate::checksum::is_valid(&part), "U-Boot requires a zero residue");
        let p = CksmHeader::parse(&part).expect("re-parse");
        assert_eq!(p.data_length as usize, payload.len());
        assert_eq!(p.field_1c, 9);
        assert_eq!(&part[CKSM_HDR_LEN..], &payload[..]);
    }

    #[test]
    fn uimage_roundtrip_and_crcs() {
        let h = UImageHeader {
            time: 0x1234_5678,
            load: 0,
            entry: 0,
            os: 5,
            arch: 22,
            img_type: 2,
            comp: 0,
            name: "Linux-5.10.168".into(),
        };
        let payload: Vec<u8> = (0u32..5000).map(|i| (i * 31 % 251) as u8).collect();
        let part = h.build(&payload);
        let p = UImageHeader::parse(&part).expect("re-parse");
        assert_eq!(p.name, "Linux-5.10.168");
        assert_eq!(p.time, 0x1234_5678);
        // data CRC
        assert_eq!(
            u32::from_be_bytes(part[0x18..0x1C].try_into().unwrap()),
            crc32fast::hash(&payload)
        );
        // header CRC, computed over the header with the field zeroed
        let mut hdr = part[..UIMAGE_HDR_LEN].to_vec();
        let stored = u32::from_be_bytes(hdr[4..8].try_into().unwrap());
        hdr[4..8].fill(0);
        assert_eq!(stored, crc32fast::hash(&hdr));
    }

    #[test]
    fn nvt_tag_is_found_and_fixed() {
        let mut part = vec![0u8; 0x800];
        let off = 0x350;
        part[off..off + 8].copy_from_slice(b"ub51102 ");
        let len = part.len() as u32;
        part[off + 0x18..off + 0x1C].copy_from_slice(&len.to_le_bytes());
        part[off + 0x1C..off + 0x1E].copy_from_slice(&NVT_TAG_MAGIC.to_le_bytes());
        let t = find_nvt_tag(&part).expect("tag located by size + magic");
        assert_eq!(t.offset, off);
        assert_eq!(t.tag, "ub51102");
        fix_nvt_tag(&mut part, off).unwrap();
        assert!(crate::checksum::is_valid(&part));
        // Idempotent: solving again must not drift.
        let before = part.clone();
        fix_nvt_tag(&mut part, off).unwrap();
        assert_eq!(before, part);
    }
}
