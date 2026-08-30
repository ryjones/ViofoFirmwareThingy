//! `manifest.toml` -- the human-editable description of a split firmware.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Serde helper: represent a u32 as a `"0x…"` string so the manifest stays
/// readable next to a hex editor.
pub mod hex32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("0x{v:08x}"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
        let s = String::deserialize(d)?;
        let t = s.trim();
        let v = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(h, 16),
            None => t.parse::<u32>(),
        };
        v.map_err(serde::de::Error::custom)
    }
}

pub mod hex32_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<u32>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(v) => s.serialize_str(&format!("0x{v:x}")),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
        let s = Option::<String>::deserialize(d)?;
        match s {
            None => Ok(None),
            Some(s) => {
                let t = s.trim();
                let v = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                    Some(h) => u32::from_str_radix(h, 16),
                    None => t.parse::<u32>(),
                };
                v.map(Some).map_err(serde::de::Error::custom)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub firmware: Firmware,
    #[serde(rename = "partition")]
    pub partitions: Vec<Partition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Firmware {
    /// Source image this manifest was produced from (informational).
    #[serde(default)]
    pub source: String,
    #[serde(with = "hex32")]
    pub version: u32,
    /// Offset of the partition table; also the size of the fixed header.
    pub header_size: u32,
    pub chksum_method: u32,
    /// Partition start alignment, in bytes, measured from the first partition
    /// offset (`header_size + 12 * partition_count`).
    pub align: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    pub id: u32,
    /// Informational; taken from the device tree `nvtpack/index` node.
    pub name: String,
    /// Path of the payload, relative to the manifest.
    pub file: String,
    pub container: Container,
    /// For `raw` partitions carrying a Novatek build tag: byte offset of the
    /// tag, whose corrective checksum word is re-solved on pack.
    #[serde(default, with = "hex32_opt", skip_serializing_if = "Option::is_none")]
    pub nvt_tag_offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cksm: Option<Cksm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uimage: Option<UImage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    /// `file` is the complete partition, byte for byte.
    Raw,
    /// `file` is the payload; a 0x40-byte CKSM header is prepended on pack.
    Cksm,
    /// `file` is the payload; a 0x40-byte legacy uImage header is prepended.
    Uimage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cksm {
    #[serde(with = "hex32")]
    pub version: u32,
    pub field_08: u32,
    pub data_offset: u32,
    pub field_18: u32,
    pub field_1c: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UImage {
    pub name: String,
    pub time: u32,
    #[serde(with = "hex32")]
    pub load: u32,
    #[serde(with = "hex32")]
    pub entry: u32,
    pub os: u8,
    pub arch: u8,
    #[serde(rename = "type")]
    pub img_type: u8,
    pub comp: u8,
}

impl Manifest {
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn from_toml(s: &str) -> Result<Self> {
        let m: Manifest = toml::from_str(s)?;
        if m.partitions.is_empty() {
            bail!("manifest has no partitions");
        }
        if m.firmware.align == 0 || !m.firmware.align.is_power_of_two() {
            bail!("firmware.align must be a power of two, got {}", m.firmware.align);
        }
        for p in &m.partitions {
            match p.container {
                Container::Cksm if p.cksm.is_none() => {
                    bail!("partition {} ({}): container = \"cksm\" needs a [partition.cksm] table", p.id, p.name)
                }
                Container::Uimage if p.uimage.is_none() => {
                    bail!("partition {} ({}): container = \"uimage\" needs a [partition.uimage] table", p.id, p.name)
                }
                _ => {}
            }
        }
        Ok(m)
    }
}
