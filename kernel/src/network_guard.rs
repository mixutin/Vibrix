//! Bounded IPv4 ingress hardening before deeper protocol parsing.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooLarge,
    Fragmented,
    UnspecifiedSource,
    LoopbackSource,
    MulticastSource,
    BroadcastSource,
    LocalAddressSpoof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Policy {
    pub local_address: [u8; 4],
    pub max_packet_bytes: u16,
    pub allow_unspecified_source: bool,
}

impl Policy {
    pub const fn new(local_address: [u8; 4], max_packet_bytes: u16) -> Self {
        Self {
            local_address,
            max_packet_bytes,
            allow_unspecified_source: false,
        }
    }

    pub fn validate(
        &self,
        source: [u8; 4],
        total_length: u16,
        fragment_offset: u16,
        more_fragments: bool,
    ) -> Result<(), Error> {
        if total_length > self.max_packet_bytes {
            return Err(Error::TooLarge);
        }
        if fragment_offset != 0 || more_fragments {
            return Err(Error::Fragmented);
        }
        if source == [0, 0, 0, 0] && !self.allow_unspecified_source {
            return Err(Error::UnspecifiedSource);
        }
        if source[0] == 127 {
            return Err(Error::LoopbackSource);
        }
        if (224..=239).contains(&source[0]) {
            return Err(Error::MulticastSource);
        }
        if source == [255, 255, 255, 255] {
            return Err(Error::BroadcastSource);
        }
        if source == self.local_address && self.local_address != [0, 0, 0, 0] {
            return Err(Error::LocalAddressSpoof);
        }
        Ok(())
    }
}

pub fn self_test() -> Result<(), Error> {
    let policy = Policy::new([192, 0, 2, 10], 1500);
    policy.validate([198, 51, 100, 20], 576, 0, false)?;
    if policy.validate([127, 0, 0, 1], 64, 0, false) != Err(Error::LoopbackSource) {
        return Err(Error::LoopbackSource);
    }
    if policy.validate([198, 51, 100, 20], 576, 1, false) != Err(Error::Fragmented) {
        return Err(Error::Fragmented);
    }
    if policy.validate([198, 51, 100, 20], 1600, 0, false) != Err(Error::TooLarge) {
        return Err(Error::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_unfragmented_packet_within_limit_is_accepted() {
        let policy = Policy::new([10, 0, 0, 2], 1500);
        assert_eq!(policy.validate([10, 0, 0, 1], 512, 0, false), Ok(()));
    }

    #[test]
    fn fragments_and_oversized_packets_fail_before_deeper_processing() {
        let policy = Policy::new([10, 0, 0, 2], 1500);
        assert_eq!(
            policy.validate([10, 0, 0, 1], 1501, 0, false),
            Err(Error::TooLarge)
        );
        assert_eq!(
            policy.validate([10, 0, 0, 1], 512, 1, false),
            Err(Error::Fragmented)
        );
        assert_eq!(
            policy.validate([10, 0, 0, 1], 512, 0, true),
            Err(Error::Fragmented)
        );
    }

    #[test]
    fn obvious_spoofed_source_classes_are_rejected() {
        let policy = Policy::new([192, 0, 2, 10], 1500);
        assert_eq!(
            policy.validate([0, 0, 0, 0], 64, 0, false),
            Err(Error::UnspecifiedSource)
        );
        assert_eq!(
            policy.validate([127, 1, 2, 3], 64, 0, false),
            Err(Error::LoopbackSource)
        );
        assert_eq!(
            policy.validate([224, 0, 0, 1], 64, 0, false),
            Err(Error::MulticastSource)
        );
        assert_eq!(
            policy.validate([255, 255, 255, 255], 64, 0, false),
            Err(Error::BroadcastSource)
        );
        assert_eq!(
            policy.validate([192, 0, 2, 10], 64, 0, false),
            Err(Error::LocalAddressSpoof)
        );
    }

    #[test]
    fn dhcp_style_unspecified_source_requires_explicit_opt_in() {
        let mut policy = Policy::new([0, 0, 0, 0], 1500);
        assert_eq!(
            policy.validate([0, 0, 0, 0], 300, 0, false),
            Err(Error::UnspecifiedSource)
        );
        policy.allow_unspecified_source = true;
        assert_eq!(policy.validate([0, 0, 0, 0], 300, 0, false), Ok(()));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
