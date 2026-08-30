//! Novatek firmware checksum.
//!
//! Recovered from `nvt_ivot_fw_update_utils.c` in the shipped U-Boot
//! (function at VA 0x7E011EF8 in the A329S `uboot` partition):
//!
//! ```c
//! uint32_t nvt_chksum(void *buf, uint32_t len) {
//!     uint32_t sum = 0;
//!     uint16_t *p = buf;
//!     for (uint32_t i = 0; i < (len >> 1); i++)
//!         sum += p[i] + i;          /* note: the *index* is added too */
//!     return sum & 0xFFFF;
//! }
//! ```
//!
//! Every caller checks `nvt_chksum(...) == 0`, so a blob is "valid" when the
//! whole region sums to zero. Producers achieve that by parking a corrective
//! 16-bit word somewhere inside the region (see [`solve`]).

/// Novatek 16-bit checksum over `data` (`sum of u16[i] + i`, truncated).
pub fn nvt_chksum(data: &[u8]) -> u16 {
    let n = data.len() / 2;
    let mut sum: u32 = 0;
    for (i, w) in data[..n * 2].chunks_exact(2).enumerate() {
        sum = sum.wrapping_add(u16::from_le_bytes([w[0], w[1]]) as u32);
        sum = sum.wrapping_add(i as u32);
    }
    (sum & 0xFFFF) as u16
}

/// A region is accepted by the bootloader when its checksum is zero.
pub fn is_valid(data: &[u8]) -> bool {
    nvt_chksum(data) == 0
}

/// Value to store in the 16-bit corrective slot at `slot` (a byte offset into
/// `data`, which must be 2-byte aligned) so that `nvt_chksum(data) == 0`.
///
/// The slot is zeroed first, so this is idempotent.
pub fn solve(data: &mut [u8], slot: usize) -> u16 {
    data[slot] = 0;
    data[slot + 1] = 0;
    let v = nvt_chksum(data).wrapping_neg();
    data[slot..slot + 2].copy_from_slice(&v.to_le_bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_makes_region_valid() {
        let mut buf: Vec<u8> = (0u32..1000).map(|i| (i * 7 % 251) as u8).collect();
        assert!(!is_valid(&buf));
        solve(&mut buf, 8);
        assert!(is_valid(&buf));
    }

    #[test]
    fn index_term_is_included() {
        // Two zero halfwords: sum = (0+0) + (0+1) = 1.
        assert_eq!(nvt_chksum(&[0, 0, 0, 0]), 1);
    }
}
