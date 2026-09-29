//! Internet checksum helpers shared by IPv4 and ICMP.
//! RFC 1071: one's-complement sum over 16-bit network-order words.

/// Compute the 16-bit Internet checksum. Odd trailing bytes are padded with a
/// zero octet for the sum without modifying the input.
pub(super) fn checksum(bytes: &[u8]) -> u16 {
    !fold_sum(bytes)
}

/// Verify a packet that already contains its transmitted checksum.
pub(super) fn valid(bytes: &[u8]) -> bool {
    fold_sum(bytes) == 0xffff
}

fn fold_sum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut chunks = bytes.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u16::from_be_bytes([chunk[0], chunk[1]]) as u32);
    }
    if let [last] = chunks.remainder() {
        sum = sum.wrapping_add((*last as u32) << 8);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_odd_lengths_follow_ones_complement_rules() {
        assert_eq!(checksum(&[]), 0xffff);
        assert_eq!(checksum(&[0x08, 0x00, 0, 0, 0, 0, 0, 0]), 0xf7ff);
        assert_eq!(checksum(&[0x01]), 0xfeff);
    }

    #[test]
    fn inserted_checksum_validates() {
        let mut message = [0x08, 0x00, 0, 0, 0x12, 0x34, 0x00, 0x01, 0xaa];
        let value = checksum(&message);
        message[2..4].copy_from_slice(&value.to_be_bytes());
        assert!(valid(&message));
        message[8] ^= 1;
        assert!(!valid(&message));
    }
}
