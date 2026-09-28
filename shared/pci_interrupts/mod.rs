//! Bounded PCI interrupt capability discovery; no device writes or allocation.
//!
//! The caller serializes aligned configuration reads for one present function.
//! Unknown capabilities are skipped, not interpreted. Successful discovery is
//! metadata only: it neither owns an interrupt vector nor authorizes BAR access.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    ReadFailed,
    DeviceAbsent,
    UnsupportedHeader,
    InvalidPointer,
    OverlapOrCycle,
    Truncated,
    Duplicate,
    InvalidControl,
    InvalidBir,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Msi {
    pub(crate) offset: u8,
    pub(crate) control: u16,
}

impl Msi {
    pub const fn offset(self) -> u8 {
        self.offset
    }

    pub const fn control(self) -> u16 {
        self.control
    }

    pub const fn address_64_bit(self) -> bool {
        self.control & 0x80 != 0
    }

    pub const fn per_vector_masking(self) -> bool {
        self.control & 0x100 != 0
    }

    pub const fn message_capacity(self) -> u8 {
        1 << ((self.control >> 1) & 7)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarRegion {
    pub(crate) bir: u8,
    pub(crate) offset: u32,
}

impl BarRegion {
    pub const fn bir(self) -> u8 {
        self.bir
    }

    pub const fn offset(self) -> u32 {
        self.offset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Msix {
    pub(crate) offset: u8,
    pub(crate) control: u16,
    pub(crate) table: BarRegion,
    pub(crate) pending: BarRegion,
}

impl Msix {
    pub const fn offset(self) -> u8 {
        self.offset
    }

    pub const fn control(self) -> u16 {
        self.control
    }

    pub const fn vectors(self) -> u16 {
        (self.control & 0x7ff) + 1
    }

    pub const fn table(self) -> BarRegion {
        self.table
    }

    pub const fn pending(self) -> BarRegion {
        self.pending
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    pub count: u8,
    pub msi: Option<Msi>,
    pub msix: Option<Msix>,
}

/// Inspect conventional (256-byte) configuration space of a type-0/1 function.
///
/// Reads only aligned DWORDs. The occupied-slot bitmap bounds traversal to 48
/// headers, detects cycles, and prevents headers from aliasing known payloads.
/// Unknown capability payload lengths cannot be inferred from their headers.
/// No partial result is returned for an invalid chain or failed read.
pub fn discover(read: &mut impl FnMut(u8) -> Option<u32>) -> Result<Capabilities, Error> {
    let vendor = read(0).ok_or(Error::ReadFailed)? as u16;
    if vendor == 0 || vendor == 0xffff {
        return Err(Error::DeviceAbsent);
    }
    let kind = ((read(0x0c).ok_or(Error::ReadFailed)? >> 16) as u8) & 0x7f;
    let bar_slots = match kind {
        0 => 6,
        1 => 2,
        _ => return Err(Error::UnsupportedHeader),
    };
    let mut result = Capabilities::default();
    if read(4).ok_or(Error::ReadFailed)? & (1 << 20) == 0 {
        return Ok(result);
    }
    let mut next = read(0x34).ok_or(Error::ReadFailed)? as u8;
    let mut occupied = 0u64;
    while next != 0 {
        if next < 0x40 || next & 3 != 0 {
            return Err(Error::InvalidPointer);
        }
        if occupied & (1u64 << (next / 4)) != 0 {
            return Err(Error::OverlapOrCycle);
        }
        let header = read(next).ok_or(Error::ReadFailed)?;
        let id = header as u8;
        let control = (header >> 16) as u16;
        let words = match id {
            5 => {
                let capable = (control >> 1) & 7;
                let enabled = (control >> 4) & 7;
                if capable > 5 || enabled > capable {
                    return Err(Error::InvalidControl);
                }
                if control & 0x100 != 0 {
                    if control & 0x80 != 0 { 6 } else { 5 }
                } else if control & 0x80 != 0 {
                    4
                } else {
                    3
                }
            }
            0x11 => 3,
            _ => 1,
        };
        reserve(&mut occupied, next, words)?;
        match id {
            5 => {
                if result.msi.is_some() {
                    return Err(Error::Duplicate);
                }
                result.msi = Some(Msi {
                    offset: next,
                    control,
                });
            }
            0x11 => {
                if result.msix.is_some() {
                    return Err(Error::Duplicate);
                }
                let table = region(read(next + 4).ok_or(Error::ReadFailed)?, bar_slots)?;
                let pending = region(read(next + 8).ok_or(Error::ReadFailed)?, bar_slots)?;
                result.msix = Some(Msix {
                    offset: next,
                    control,
                    table,
                    pending,
                });
            }
            _ => {}
        }
        result.count += 1;
        next = (header >> 8) as u8;
    }
    Ok(result)
}

fn reserve(occupied: &mut u64, offset: u8, words: u8) -> Result<(), Error> {
    let start = offset / 4;
    if u16::from(start) + u16::from(words) > 64 {
        return Err(Error::Truncated);
    }
    let mask = ((1u64 << words) - 1) << start;
    if *occupied & mask != 0 {
        return Err(Error::OverlapOrCycle);
    }
    *occupied |= mask;
    Ok(())
}

fn region(word: u32, bar_slots: u8) -> Result<BarRegion, Error> {
    let bir = (word & 7) as u8;
    if bir >= bar_slots {
        return Err(Error::InvalidBir);
    }
    Ok(BarRegion {
        bir,
        offset: word & !7,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> [u32; 64] {
        let mut words = [0u32; 64];
        words[0] = 0x11e8_1234;
        words[1] = 1 << 20;
        words[0x34 / 4] = 0x40;
        words
    }

    fn parse(words: &[u32; 64]) -> Result<Capabilities, Error> {
        discover(&mut |offset| {
            assert_eq!(offset & 3, 0);
            Some(words[usize::from(offset) / 4])
        })
    }

    #[test]
    fn no_capabilities_does_not_read_the_pointer() {
        let result = discover(&mut |offset| match offset {
            0 => Some(0x1234_5678),
            4 | 0x0c => Some(0),
            _ => panic!("unexpected configuration read"),
        });
        assert_eq!(result, Ok(Capabilities::default()));
    }

    #[test]
    fn absent_failed_and_unsupported_headers_are_rejected() {
        assert_eq!(discover(&mut |_| None), Err(Error::ReadFailed));
        for vendor in [0, 0xffff] {
            assert_eq!(discover(&mut |_| Some(vendor)), Err(Error::DeviceAbsent));
        }
        let mut words = config();
        words[3] = 2 << 16;
        assert_eq!(parse(&words), Err(Error::UnsupportedHeader));
    }

    #[test]
    fn every_pointer_is_bounded_and_aligned_before_access() {
        for pointer in 1..=255u8 {
            let mut words = config();
            words[13] = u32::from(pointer);
            let result = parse(&words);
            if pointer < 0x40 || pointer & 3 != 0 {
                assert_eq!(result, Err(Error::InvalidPointer));
            } else {
                assert_eq!(result.unwrap().count, 1);
            }
        }
    }

    #[test]
    fn msi_layouts_and_capacity_are_decoded() {
        for flags in [0, 0x80, 0x100, 0x180] {
            for power in 0..=5 {
                let mut words = config();
                let control = flags | (power << 1);
                words[16] = (control << 16) | 5;
                let msi = parse(&words).unwrap().msi.unwrap();
                assert_eq!(msi.offset(), 0x40);
                assert_eq!(msi.message_capacity(), 1 << power);
                assert_eq!(msi.address_64_bit(), flags & 0x80 != 0);
                assert_eq!(msi.per_vector_masking(), flags & 0x100 != 0);
            }
        }
    }

    #[test]
    fn rejects_invalid_msi_control_and_truncated_payloads() {
        for control in [6 << 1, 7 << 1, 1 << 4] {
            let mut words = config();
            words[16] = (control << 16) | 5;
            assert_eq!(parse(&words), Err(Error::InvalidControl));
        }
        for (flags, bytes) in [(0, 12), (0x80, 16), (0x100, 20), (0x180, 24)] {
            let mut words = config();
            let offset = 256 - bytes;
            words[13] = offset;
            words[offset as usize / 4] = (flags << 16) | 5;
            assert!(parse(&words).is_ok());
            words[13] = offset + 4;
            words[(offset + 4) as usize / 4] = (flags << 16) | 5;
            assert_eq!(parse(&words), Err(Error::Truncated));
        }
    }

    #[test]
    fn cycles_duplicates_and_overlapping_known_payloads_fail() {
        let mut words = config();
        words[16] = 0x4001;
        assert_eq!(parse(&words), Err(Error::OverlapOrCycle));
        words[16] = 0x5005;
        words[20] = 5;
        assert_eq!(parse(&words), Err(Error::Duplicate));
        words[16] = 0x4405;
        assert_eq!(parse(&words), Err(Error::OverlapOrCycle));
        words[16] = 0x6001;
        words[24] = 0x3c01;
        assert_eq!(parse(&words), Err(Error::InvalidPointer));
        words[16] = 0x6001;
        words[24] = 0x5c01;
        words[23] = (0x180 << 16) | 5;
        assert_eq!(parse(&words), Err(Error::OverlapOrCycle));
    }

    #[test]
    fn visits_all_48_unknown_headers_once() {
        let mut words = config();
        for (index, word) in words.iter_mut().enumerate().skip(16) {
            *word = 1 | if index == 63 {
                0
            } else {
                ((index + 1) as u32 * 4) << 8
            };
        }
        assert_eq!(parse(&words).unwrap().count, 48);
    }

    #[test]
    fn msix_decodes_regions_and_maximum_vector_count() {
        let mut words = config();
        words[16] = (0xc7ff << 16) | 0x11;
        words[17] = 0x1005;
        words[18] = 0x9000;
        let msix = parse(&words).unwrap().msix.unwrap();
        assert_eq!(msix.vectors(), 2048);
        assert_eq!(
            msix.table(),
            BarRegion {
                bir: 5,
                offset: 0x1000
            }
        );
        assert_eq!(
            msix.pending(),
            BarRegion {
                bir: 0,
                offset: 0x9000
            }
        );
        words[3] = 0x80 << 16;
        assert!(parse(&words).is_ok());
        words[3] = 1 << 16;
        assert_eq!(parse(&words), Err(Error::InvalidBir));
        words[3] = 0;
        words[17] = 6;
        assert_eq!(parse(&words), Err(Error::InvalidBir));
    }

    #[test]
    fn msi_and_msix_can_coexist_without_enabling_either() {
        let mut words = config();
        words[16] = 0x5005;
        words[20] = 0x11;
        words[21] = 0;
        words[22] = 0x1000;
        let before = words;
        let found = parse(&words).unwrap();
        assert_eq!(found.count, 2);
        assert!(found.msi.is_some());
        assert!(found.msix.is_some());
        assert_eq!(words, before);
    }

    #[test]
    fn missing_payload_read_returns_no_partial_result() {
        let mut words = config();
        words[16] = 0x11;
        assert_eq!(
            discover(&mut |offset| {
                if offset == 0x48 {
                    None
                } else {
                    Some(words[offset as usize / 4])
                }
            }),
            Err(Error::ReadFailed)
        );
    }
}
