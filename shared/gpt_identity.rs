//! Allocation-free GPT identity validation shared by loader and kernel.
//!
//! The caller owns block I/O. This module validates the identity-critical
//! primary/backup GPT metadata and returns the disk, ESP and optional Vibrix
//! System partition GUID tuple used by boot-device reacquisition.

pub const ESP_TYPE_GUID: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
pub const VIBRIX_SYSTEM_TYPE_GUID: [u8; 16] = [
    0x3b, 0x0f, 0x4a, 0x2e, 0x3d, 0x6a, 0x96, 0x4e, 0xb9, 0x9a, 0x55, 0x3d, 0x7c, 0x0b, 0x12, 0x01,
];

const HEADER_MIN: usize = 92;
const ENTRY_MIN: usize = 128;
const MAX_ENTRIES: usize = 128;
pub const MAX_ENTRY_ARRAY_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeaderLayout {
    pub first_usable: u64,
    pub last_usable: u64,
    pub entry_lba: u64,
    pub entry_count: u32,
    pub entry_size: usize,
    pub entry_array_bytes: usize,
    pub entry_crc: u32,
    pub disk_guid: [u8; 16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity {
    pub disk_guid: [u8; 16],
    pub esp_guid: [u8; 16],
    pub esp_first_lba: u64,
    pub esp_last_lba: u64,
    pub system_guid: Option<[u8; 16]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    SectorSize,
    HeaderLength,
    HeaderSignature,
    HeaderRevision,
    HeaderCrc,
    HeaderLocations,
    HeaderLayout,
    MetadataMismatch,
    EntryLength,
    EntryCrc,
    EntryCopiesDiffer,
    EmptyDiskGuid,
    EmptyPartitionGuid,
    DuplicatePartitionGuid,
    PartitionRange,
    OverlappingPartitions,
    MissingEsp,
    MultipleEsp,
    MultipleSystem,
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated GPT field"),
    )
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated GPT field"),
    )
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

pub fn validate_header(
    header: &[u8],
    sector_size: usize,
    expected_current: u64,
    expected_backup: u64,
) -> Result<(usize, HeaderLayout), Error> {
    if !matches!(sector_size, 512 | 4096) {
        return Err(Error::SectorSize);
    }
    if header.len() != sector_size {
        return Err(Error::HeaderLength);
    }
    if header.get(..8) != Some(b"EFI PART") {
        return Err(Error::HeaderSignature);
    }
    let size = u32_at(header, 12) as usize;
    if !(HEADER_MIN..=sector_size).contains(&size) {
        return Err(Error::HeaderLength);
    }
    if u32_at(header, 8) != 0x0001_0000 || u32_at(header, 20) != 0 {
        return Err(Error::HeaderRevision);
    }
    let mut copy = [0u8; 4096];
    copy[..size].copy_from_slice(&header[..size]);
    let expected_crc = u32_at(header, 16);
    copy[16..20].fill(0);
    if crc32(&copy[..size]) != expected_crc {
        return Err(Error::HeaderCrc);
    }
    if u64_at(header, 24) != expected_current || u64_at(header, 32) != expected_backup {
        return Err(Error::HeaderLocations);
    }
    let first_usable = u64_at(header, 40);
    let last_usable = u64_at(header, 48);
    if first_usable < 2
        || first_usable > last_usable
        || last_usable >= expected_backup.max(expected_current)
    {
        return Err(Error::HeaderLayout);
    }
    let entry_lba = u64_at(header, 72);
    let entry_count = u32_at(header, 80);
    let entry_size = u32_at(header, 84) as usize;
    if entry_count == 0
        || entry_count as usize > MAX_ENTRIES
        || entry_size < ENTRY_MIN
        || !entry_size.is_multiple_of(8)
    {
        return Err(Error::HeaderLayout);
    }
    let entry_array_bytes = (entry_count as usize)
        .checked_mul(entry_size)
        .ok_or(Error::EntryLength)?;
    if entry_array_bytes > MAX_ENTRY_ARRAY_BYTES {
        return Err(Error::EntryLength);
    }
    let disk_guid: [u8; 16] = header[56..72].try_into().expect("fixed GPT GUID");
    if disk_guid == [0; 16] {
        return Err(Error::EmptyDiskGuid);
    }
    Ok((
        size,
        HeaderLayout {
            first_usable,
            last_usable,
            entry_lba,
            entry_count,
            entry_size,
            entry_array_bytes,
            entry_crc: u32_at(header, 88),
            disk_guid,
        },
    ))
}


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Crc32 {
    state: u32,
}

impl Crc32 {
    pub const fn new() -> Self {
        Self { state: !0u32 }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u32::from(byte);
            for _ in 0..8 {
                self.state =
                    (self.state >> 1) ^ if self.state & 1 != 0 { 0xedb8_8320 } else { 0 };
            }
        }
    }

    pub const fn finish(self) -> u32 {
        !self.state
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

/// Incremental, allocation-free GPT entry-array identity scanner.
///
/// Chunks must contain complete entries and together must cover exactly the
/// entry-array byte count declared by the validated header. This allows a
/// block driver to verify a GPT without buffering the whole entry array.
pub struct EntryScanner {
    layout: HeaderLayout,
    crc: Crc32,
    consumed: usize,
    seen: [[u8; 16]; MAX_ENTRIES],
    seen_len: usize,
    ranges: [(u64, u64); MAX_ENTRIES],
    range_len: usize,
    esp: Option<([u8; 16], u64, u64)>,
    system: Option<[u8; 16]>,
}

impl EntryScanner {
    pub const fn new(layout: HeaderLayout) -> Self {
        Self {
            layout,
            crc: Crc32::new(),
            consumed: 0,
            seen: [[0; 16]; MAX_ENTRIES],
            seen_len: 0,
            ranges: [(0, 0); MAX_ENTRIES],
            range_len: 0,
            esp: None,
            system: None,
        }
    }

    pub fn consume(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if chunk.is_empty()
            || !chunk.len().is_multiple_of(self.layout.entry_size)
            || self
                .consumed
                .checked_add(chunk.len())
                .is_none_or(|end| end > self.layout.entry_array_bytes)
        {
            return Err(Error::EntryLength);
        }
        self.crc.update(chunk);
        for entry in chunk.chunks_exact(self.layout.entry_size) {
            let type_guid: [u8; 16] = entry[..16].try_into().expect("fixed GPT GUID");
            if type_guid == [0; 16] {
                continue;
            }
            let guid: [u8; 16] = entry[16..32].try_into().expect("fixed GPT GUID");
            if guid == [0; 16] {
                return Err(Error::EmptyPartitionGuid);
            }
            if self.seen[..self.seen_len].contains(&guid) {
                return Err(Error::DuplicatePartitionGuid);
            }
            self.seen[self.seen_len] = guid;
            self.seen_len += 1;

            let first = u64_at(entry, 32);
            let last = u64_at(entry, 40);
            if first < self.layout.first_usable
                || first > last
                || last > self.layout.last_usable
            {
                return Err(Error::PartitionRange);
            }
            for &(other_first, other_last) in &self.ranges[..self.range_len] {
                if first <= other_last && other_first <= last {
                    return Err(Error::OverlappingPartitions);
                }
            }
            self.ranges[self.range_len] = (first, last);
            self.range_len += 1;

            if type_guid == ESP_TYPE_GUID {
                if self.esp.replace((guid, first, last)).is_some() {
                    return Err(Error::MultipleEsp);
                }
            } else if type_guid == VIBRIX_SYSTEM_TYPE_GUID
                && self.system.replace(guid).is_some()
            {
                return Err(Error::MultipleSystem);
            }
        }
        self.consumed += chunk.len();
        Ok(())
    }

    pub fn finish(self) -> Result<Identity, Error> {
        if self.consumed != self.layout.entry_array_bytes {
            return Err(Error::EntryLength);
        }
        if self.crc.finish() != self.layout.entry_crc {
            return Err(Error::EntryCrc);
        }
        let (esp_guid, esp_first_lba, esp_last_lba) = self.esp.ok_or(Error::MissingEsp)?;
        Ok(Identity {
            disk_guid: self.layout.disk_guid,
            esp_guid,
            esp_first_lba,
            esp_last_lba,
            system_guid: self.system,
        })
    }
}

pub fn layouts_match(primary: HeaderLayout, backup: HeaderLayout) -> Result<(), Error> {
    if primary.first_usable != backup.first_usable
        || primary.last_usable != backup.last_usable
        || primary.entry_count != backup.entry_count
        || primary.entry_size != backup.entry_size
        || primary.entry_array_bytes != backup.entry_array_bytes
        || primary.entry_crc != backup.entry_crc
        || primary.disk_guid != backup.disk_guid
    {
        return Err(Error::MetadataMismatch);
    }
    Ok(())
}

pub fn identities_match(primary: Identity, backup: Identity) -> Result<Identity, Error> {
    if primary != backup {
        return Err(Error::MetadataMismatch);
    }
    Ok(primary)
}

pub fn validate_identity(
    sector_size: usize,
    last_lba: u64,
    primary_header: &[u8],
    primary_entries: &[u8],
    backup_header: &[u8],
    backup_entries: &[u8],
) -> Result<Identity, Error> {
    if last_lba < 5 {
        return Err(Error::HeaderLocations);
    }
    let primary = validate_header(primary_header, sector_size, 1, last_lba)?;
    let backup = validate_header(backup_header, sector_size, last_lba, 1)?;

    if primary.0 != backup.0
        || primary.1.first_usable != backup.1.first_usable
        || primary.1.last_usable != backup.1.last_usable
        || primary.1.entry_count != backup.1.entry_count
        || primary.1.entry_size != backup.1.entry_size
        || primary.1.entry_crc != backup.1.entry_crc
        || primary.1.disk_guid != backup.1.disk_guid
    {
        return Err(Error::MetadataMismatch);
    }

    let expected_entries = primary.1.entry_array_bytes;
    if primary_entries.len() != expected_entries || backup_entries.len() != expected_entries {
        return Err(Error::EntryLength);
    }
    if crc32(primary_entries) != primary.1.entry_crc || crc32(backup_entries) != backup.1.entry_crc
    {
        return Err(Error::EntryCrc);
    }
    if primary_entries != backup_entries {
        return Err(Error::EntryCopiesDiffer);
    }

    let primary_array_sectors = expected_entries.div_ceil(sector_size) as u64;
    if primary.1.entry_lba < 2
        || primary.1.entry_lba.saturating_add(primary_array_sectors) > primary.1.first_usable
        || backup.1.entry_lba <= backup.1.last_usable
        || backup.1.entry_lba.saturating_add(primary_array_sectors) > last_lba
    {
        return Err(Error::HeaderLayout);
    }

    let mut seen = [[0u8; 16]; MAX_ENTRIES];
    let mut seen_len = 0usize;
    let mut ranges = [(0u64, 0u64); MAX_ENTRIES];
    let mut range_len = 0usize;
    let mut esp: Option<([u8; 16], u64, u64)> = None;
    let mut system = None;

    for entry in primary_entries.chunks_exact(primary.1.entry_size) {
        let type_guid: [u8; 16] = entry[..16].try_into().expect("fixed GPT GUID");
        if type_guid == [0; 16] {
            continue;
        }
        let guid: [u8; 16] = entry[16..32].try_into().expect("fixed GPT GUID");
        if guid == [0; 16] {
            return Err(Error::EmptyPartitionGuid);
        }
        if seen[..seen_len].contains(&guid) {
            return Err(Error::DuplicatePartitionGuid);
        }
        seen[seen_len] = guid;
        seen_len += 1;

        let first = u64_at(entry, 32);
        let last = u64_at(entry, 40);
        if first < primary.1.first_usable || first > last || last > primary.1.last_usable {
            return Err(Error::PartitionRange);
        }
        for &(other_first, other_last) in &ranges[..range_len] {
            if first <= other_last && other_first <= last {
                return Err(Error::OverlappingPartitions);
            }
        }
        ranges[range_len] = (first, last);
        range_len += 1;

        if type_guid == ESP_TYPE_GUID {
            if esp.replace((guid, first, last)).is_some() {
                return Err(Error::MultipleEsp);
            }
        } else if type_guid == VIBRIX_SYSTEM_TYPE_GUID && system.replace(guid).is_some() {
            return Err(Error::MultipleSystem);
        }
    }

    let (esp_guid, esp_first_lba, esp_last_lba) = esp.ok_or(Error::MissingEsp)?;
    Ok(Identity {
        disk_guid: primary.1.disk_guid,
        esp_guid,
        esp_first_lba,
        esp_last_lba,
        system_guid: system,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put64(bytes: &mut [u8], at: usize, value: u64) {
        bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture(sector: usize) -> (std::vec::Vec<u8>, std::vec::Vec<u8>, std::vec::Vec<u8>) {
        let last_lba = 127u64;
        let mut entries = std::vec![0u8; 4 * 128];
        entries[..16].copy_from_slice(&ESP_TYPE_GUID);
        entries[16..32].copy_from_slice(&[0x22; 16]);
        put64(&mut entries, 32, 40);
        put64(&mut entries, 40, 60);
        entries[128..144].copy_from_slice(&VIBRIX_SYSTEM_TYPE_GUID);
        entries[144..160].copy_from_slice(&[0x33; 16]);
        put64(&mut entries, 160, 61);
        put64(&mut entries, 168, 90);
        let entries_crc = crc32(&entries);

        let mut primary = std::vec![0u8; sector];
        primary[..8].copy_from_slice(b"EFI PART");
        put32(&mut primary, 8, 0x0001_0000);
        put32(&mut primary, 12, 92);
        put64(&mut primary, 24, 1);
        put64(&mut primary, 32, last_lba);
        put64(&mut primary, 40, 34);
        put64(&mut primary, 48, 93);
        primary[56..72].copy_from_slice(&[0x11; 16]);
        put64(&mut primary, 72, 2);
        put32(&mut primary, 80, 4);
        put32(&mut primary, 84, 128);
        put32(&mut primary, 88, entries_crc);
        let crc = crc32(&primary[..92]);
        put32(&mut primary, 16, crc);

        let mut backup = primary.clone();
        put64(&mut backup, 24, last_lba);
        put64(&mut backup, 32, 1);
        put64(&mut backup, 72, 126);
        put32(&mut backup, 16, 0);
        let crc = crc32(&backup[..92]);
        put32(&mut backup, 16, crc);
        (primary, entries, backup)
    }

    #[test]
    fn known_crc_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn extracts_same_identity_for_512_and_4096_byte_sectors() {
        for sector in [512, 4096] {
            let (primary, entries, backup) = fixture(sector);
            let identity =
                validate_identity(sector, 127, &primary, &entries, &backup, &entries).unwrap();
            assert_eq!(identity.disk_guid, [0x11; 16]);
            assert_eq!(identity.esp_guid, [0x22; 16]);
            assert_eq!(identity.esp_first_lba, 40);
            assert_eq!(identity.esp_last_lba, 60);
            assert_eq!(identity.system_guid, Some([0x33; 16]));
        }
    }

    #[test]
    fn rejects_mismatched_copies_duplicate_guids_and_ambiguous_esp() {
        let (primary, entries, backup) = fixture(512);
        let mut mismatch = entries.clone();
        mismatch[64] ^= 1;
        assert_eq!(
            validate_identity(512, 127, &primary, &entries, &backup, &mismatch),
            Err(Error::EntryCrc)
        );

        let mut duplicate = entries.clone();
        duplicate[144..160].copy_from_slice(&[0x22; 16]);
        let crc = crc32(&duplicate);
        let mut p = primary.clone();
        let mut b = backup.clone();
        put32(&mut p, 88, crc);
        put32(&mut p, 16, 0);
        let h = crc32(&p[..92]);
        put32(&mut p, 16, h);
        put32(&mut b, 88, crc);
        put32(&mut b, 16, 0);
        let h = crc32(&b[..92]);
        put32(&mut b, 16, h);
        assert_eq!(
            validate_identity(512, 127, &p, &duplicate, &b, &duplicate),
            Err(Error::DuplicatePartitionGuid)
        );

        let mut two_esp = entries.clone();
        two_esp[128..144].copy_from_slice(&ESP_TYPE_GUID);
        let crc = crc32(&two_esp);
        put32(&mut p, 88, crc);
        put32(&mut p, 16, 0);
        let h = crc32(&p[..92]);
        put32(&mut p, 16, h);
        put32(&mut b, 88, crc);
        put32(&mut b, 16, 0);
        let h = crc32(&b[..92]);
        put32(&mut b, 16, h);
        assert_eq!(
            validate_identity(512, 127, &p, &two_esp, &b, &two_esp),
            Err(Error::MultipleEsp)
        );
    }

    #[test]
    fn streaming_scanner_matches_whole_array_validation() {
        for sector in [512, 4096] {
            let (primary, entries, backup) = fixture(sector);
            let (_, primary_layout) = validate_header(&primary, sector, 1, 127).unwrap();
            let (_, backup_layout) = validate_header(&backup, sector, 127, 1).unwrap();

            let mut primary_scan = EntryScanner::new(primary_layout);
            let mut backup_scan = EntryScanner::new(backup_layout);
            for chunk in entries.chunks(primary_layout.entry_size * 2) {
                primary_scan.consume(chunk).unwrap();
                backup_scan.consume(chunk).unwrap();
            }
            let streamed =
                identities_match(primary_scan.finish().unwrap(), backup_scan.finish().unwrap())
                    .unwrap();
            let whole =
                validate_identity(sector, 127, &primary, &entries, &backup, &entries).unwrap();
            assert_eq!(streamed, whole);
        }
    }

    #[test]
    fn streaming_scanner_rejects_truncation_and_crc_corruption() {
        let (primary, entries, _) = fixture(512);
        let (_, layout) = validate_header(&primary, 512, 1, 127).unwrap();

        let mut truncated = EntryScanner::new(layout);
        truncated.consume(&entries[..128]).unwrap();
        assert_eq!(truncated.finish(), Err(Error::EntryLength));

        let mut corrupt = entries.clone();
        corrupt[64] ^= 1;
        let mut scan = EntryScanner::new(layout);
        scan.consume(&corrupt).unwrap();
        assert_eq!(scan.finish(), Err(Error::EntryCrc));
    }

    #[test]
    fn corruption_and_missing_esp_fail_closed() {
        let (mut primary, entries, backup) = fixture(512);
        primary[40] ^= 1;
        assert_eq!(
            validate_identity(512, 127, &primary, &entries, &backup, &entries),
            Err(Error::HeaderCrc)
        );

        let (mut primary, mut entries, mut backup) = fixture(512);
        entries[..16].fill(0);
        let crc = crc32(&entries);
        for header in [&mut primary, &mut backup] {
            put32(header, 88, crc);
            put32(header, 16, 0);
            let h = crc32(&header[..92]);
            put32(header, 16, h);
        }
        assert_eq!(
            validate_identity(512, 127, &primary, &entries, &backup, &entries),
            Err(Error::MissingEsp)
        );
    }
}
