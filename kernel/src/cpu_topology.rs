//! Bounded x86 processor inventory from ACPI MADT, not AP startup.
//! Primary reference: ACPI 6.5 sections 5.2.12.2 and 5.2.12.12.

pub const MAX_PROCESSORS: usize = 64;
const HEADER_BYTES: usize = 44;
const MAX_TABLE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Signature,
    Length,
    Checksum,
    Flags,
    Entry,
    Reserved,
    DuplicateApic,
    DuplicateUid,
    Capacity,
    NoEnabledCpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability {
    Disabled,
    Enabled,
    OnlineCapable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Processor {
    pub firmware_uid: u32,
    pub apic_id: u32,
    pub availability: Availability,
    pub x2apic: bool,
}

const EMPTY: Processor = Processor {
    firmware_uid: 0,
    apic_id: 0,
    availability: Availability::Disabled,
    x2apic: false,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Topology {
    processors: [Processor; MAX_PROCESSORS],
    length: usize,
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset.checked_add(4).ok_or(Error::Length)?;
    let bytes = bytes.get(offset..end).ok_or(Error::Length)?;
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| Error::Length)?,
    ))
}

fn availability(flags: u32) -> Result<Availability, Error> {
    match flags {
        0 => Ok(Availability::Disabled),
        1 => Ok(Availability::Enabled),
        2 => Ok(Availability::OnlineCapable),
        _ => Err(Error::Flags),
    }
}

impl Topology {
    /// Input is a mapped byte slice, never a firmware physical pointer.
    /// An error discards the whole inventory; no partial result escapes.
    pub fn from_madt(input: &[u8]) -> Result<Self, Error> {
        if input.get(..4) != Some(b"APIC") {
            return Err(Error::Signature);
        }
        let length = usize::try_from(word(input, 4)?).map_err(|_| Error::Length)?;
        if !(HEADER_BYTES..=MAX_TABLE_BYTES).contains(&length) {
            return Err(Error::Length);
        }
        let bytes = input.get(..length).ok_or(Error::Length)?;
        if bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
            return Err(Error::Checksum);
        }
        if word(bytes, 40)? & !1 != 0 {
            return Err(Error::Flags);
        }
        let mut topology = Self {
            processors: [EMPTY; MAX_PROCESSORS],
            length: 0,
        };
        let mut offset = HEADER_BYTES;
        while offset < bytes.len() {
            let header = bytes.get(offset..offset + 2).ok_or(Error::Entry)?;
            let length = usize::from(header[1]);
            if length < 2 {
                return Err(Error::Entry);
            }
            let end = offset.checked_add(length).ok_or(Error::Entry)?;
            let entry = bytes.get(offset..end).ok_or(Error::Entry)?;
            let processor = match entry[0] {
                0 => {
                    if length != 8 || entry[3] == u8::MAX {
                        return Err(Error::Entry);
                    }
                    Some(Processor {
                        firmware_uid: u32::from(entry[2]),
                        apic_id: u32::from(entry[3]),
                        availability: availability(word(entry, 4)?)?,
                        x2apic: false,
                    })
                }
                9 => {
                    if length != 16 {
                        return Err(Error::Entry);
                    }
                    if entry[2..4] != [0, 0] {
                        return Err(Error::Reserved);
                    }
                    let apic_id = word(entry, 4)?;
                    if apic_id == u32::MAX {
                        return Err(Error::Entry);
                    }
                    Some(Processor {
                        firmware_uid: word(entry, 12)?,
                        apic_id,
                        availability: availability(word(entry, 8)?)?,
                        x2apic: true,
                    })
                }
                _ => None,
            };
            if let Some(processor) = processor {
                topology.insert(processor)?;
            }
            offset = end;
        }
        if topology.enabled_count() == 0 {
            return Err(Error::NoEnabledCpu);
        }
        Ok(topology)
    }

    fn insert(&mut self, processor: Processor) -> Result<(), Error> {
        for previous in self.processors() {
            if previous.apic_id == processor.apic_id {
                return Err(Error::DuplicateApic);
            }
            if previous.firmware_uid == processor.firmware_uid {
                return Err(Error::DuplicateUid);
            }
        }
        if self.length == MAX_PROCESSORS {
            return Err(Error::Capacity);
        }
        self.processors[self.length] = processor;
        self.length += 1;
        Ok(())
    }

    pub fn processors(&self) -> &[Processor] {
        &self.processors[..self.length]
    }

    pub fn enabled_count(&self) -> usize {
        self.processors()
            .iter()
            .filter(|cpu| cpu.availability == Availability::Enabled)
            .count()
    }

    pub fn online_capable_count(&self) -> usize {
        self.processors()
            .iter()
            .filter(|cpu| cpu.availability == Availability::OnlineCapable)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: &[u8]) -> std::vec::Vec<u8> {
        let mut bytes = std::vec![0; HEADER_BYTES];
        bytes[..4].copy_from_slice(b"APIC");
        bytes[8] = 5;
        bytes.extend_from_slice(entries);
        let length = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        checksum(&mut bytes);
        bytes
    }

    fn checksum(bytes: &mut [u8]) {
        bytes[9] = 0;
        bytes[9] = 0u8.wrapping_sub(bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
    }

    fn lapic(uid: u8, id: u8, flags: u8) -> [u8; 8] {
        [0, 8, uid, id, flags, 0, 0, 0]
    }

    fn x2apic(uid: u32, id: u32, flags: u32) -> [u8; 16] {
        let mut entry = [0u8; 16];
        entry[..2].copy_from_slice(&[9, 16]);
        entry[4..8].copy_from_slice(&id.to_le_bytes());
        entry[8..12].copy_from_slice(&flags.to_le_bytes());
        entry[12..16].copy_from_slice(&uid.to_le_bytes());
        entry
    }

    #[test]
    fn mixed_entries_preserve_identity_and_availability() {
        let mut entries = lapic(3, 7, 1).to_vec();
        entries.extend_from_slice(&x2apic(300, 0x1234, 2));
        entries.extend_from_slice(&lapic(9, 11, 0));
        entries.extend_from_slice(&[0x7f, 3, 0]);
        let topology = Topology::from_madt(&table(&entries)).unwrap();
        assert_eq!(topology.enabled_count(), 1);
        assert_eq!(topology.online_capable_count(), 1);
        assert_eq!(topology.processors().len(), 3);
        assert_eq!(topology.processors()[1].apic_id, 0x1234);
        assert_eq!(topology.processors()[1].firmware_uid, 300);
        assert!(topology.processors()[1].x2apic);
    }

    #[test]
    fn rejects_truncation_and_checksum() {
        let bytes = table(&lapic(0, 0, 1));
        for length in 0..bytes.len() {
            assert!(Topology::from_madt(&bytes[..length]).is_err());
        }
        let mut corrupt = bytes.clone();
        corrupt[20] ^= 1;
        assert_eq!(Topology::from_madt(&corrupt), Err(Error::Checksum));
        assert_eq!(Topology::from_madt(&table(&[])), Err(Error::NoEnabledCpu));
    }

    #[test]
    fn rejects_malformed_records_reserved_and_flags() {
        for entry in [std::vec![0, 0], std::vec![0, 2], std::vec![9, 2]] {
            assert_eq!(Topology::from_madt(&table(&entry)), Err(Error::Entry));
        }
        for flags in [3, 4, 0xff] {
            assert_eq!(
                Topology::from_madt(&table(&lapic(0, 0, flags))),
                Err(Error::Flags)
            );
        }
        let mut entry = x2apic(0, 0, 1);
        entry[2] = 1;
        assert_eq!(Topology::from_madt(&table(&entry)), Err(Error::Reserved));
    }

    #[test]
    fn duplicate_ids_fail_closed_across_record_types() {
        let mut entries = lapic(1, 2, 1).to_vec();
        entries.extend_from_slice(&x2apic(3, 2, 1));
        assert_eq!(
            Topology::from_madt(&table(&entries)),
            Err(Error::DuplicateApic)
        );
        entries.truncate(8);
        entries.extend_from_slice(&x2apic(1, 3, 1));
        assert_eq!(Topology::from_madt(&table(&entries)), Err(Error::DuplicateUid));
    }

    #[test]
    fn inventory_capacity_is_bounded() {
        let mut entries = std::vec::Vec::new();
        for id in 0..MAX_PROCESSORS {
            entries.extend_from_slice(&x2apic(id as u32, id as u32, 1));
        }
        assert_eq!(
            Topology::from_madt(&table(&entries)).unwrap().enabled_count(),
            64
        );
        entries.extend_from_slice(&x2apic(64, 64, 1));
        assert_eq!(Topology::from_madt(&table(&entries)), Err(Error::Capacity));
    }
}
