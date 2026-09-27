//! Bounded, allocation-free ACPI table parsing, independent of address mapping.
//! Source: ACPI 6.5 §5.2 and PCI Firmware Specification §4 (MCFG).
//! Every input is an already mapped/readable byte slice. Returned addresses
//! are physical integers; this module NEVER dereferences them.

const RSDP_V1_LEN: usize = 20;
const RSDP_V2_LEN: usize = 36;
const SDT_HEADER_LEN: usize = 36;
const MAX_TABLE_LEN: usize = 1024 * 1024;
const MCFG_HEADER_LEN: usize = 44;
const MCFG_ENTRY_LEN: usize = 16;
const ECAM_BUS_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiError {
    Truncated,
    InvalidSignature,
    InvalidLength,
    Checksum,
    InvalidRootAddress,
    InvalidEntry,
    InvalidAllocation,
    AddressOverflow,
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, AcpiError> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or(AcpiError::Truncated)?
        .try_into()
        .map_err(|_| AcpiError::Truncated)?;
    Ok(u16::from_le_bytes(bytes))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, AcpiError> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or(AcpiError::Truncated)?
        .try_into()
        .map_err(|_| AcpiError::Truncated)?;
    Ok(u32::from_le_bytes(bytes))
}

fn u64_at(data: &[u8], offset: usize) -> Result<u64, AcpiError> {
    let bytes: [u8; 8] = data
        .get(offset..offset + 8)
        .ok_or(AcpiError::Truncated)?
        .try_into()
        .map_err(|_| AcpiError::Truncated)?;
    Ok(u64::from_le_bytes(bytes))
}

fn checksum_ok(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte)) == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rsdp {
    pub revision: u8,
    pub rsdt_physical: u32,
    pub xsdt_physical: Option<u64>,
    pub byte_len: usize,
}

/// Parse RSDP v1 or extended ACPI 2.0+ RSDP. Caller must supply the *whole*
/// reported length before using this result; 36 bytes alone may be insufficient.
pub fn parse_rsdp(bytes: &[u8]) -> Result<Rsdp, AcpiError> {
    let prefix = bytes.get(..RSDP_V1_LEN).ok_or(AcpiError::Truncated)?;
    if &prefix[..8] != b"RSD PTR " {
        return Err(AcpiError::InvalidSignature);
    }
    if !checksum_ok(prefix) {
        return Err(AcpiError::Checksum);
    }
    let revision = prefix[15];
    let rsdt_physical = u32_at(prefix, 16)?;
    if revision < 2 {
        if rsdt_physical == 0 {
            return Err(AcpiError::InvalidRootAddress);
        }
        return Ok(Rsdp {
            revision,
            rsdt_physical,
            xsdt_physical: None,
            byte_len: RSDP_V1_LEN,
        });
    }
    let length = usize::try_from(u32_at(bytes, 20)?).map_err(|_| AcpiError::InvalidLength)?;
    if !(RSDP_V2_LEN..=4096).contains(&length) {
        return Err(AcpiError::InvalidLength);
    }
    let full = bytes.get(..length).ok_or(AcpiError::Truncated)?;
    if !checksum_ok(full) {
        return Err(AcpiError::Checksum);
    }
    let xsdt_physical = u64_at(full, 24)?;
    if xsdt_physical == 0 && rsdt_physical == 0 {
        return Err(AcpiError::InvalidRootAddress);
    }
    Ok(Rsdp {
        revision,
        rsdt_physical,
        xsdt_physical: (xsdt_physical != 0).then_some(xsdt_physical),
        byte_len: length,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sdt<'a> {
    pub signature: [u8; 4],
    pub revision: u8,
    bytes: &'a [u8],
}

impl<'a> Sdt<'a> {
    /// Requires mapped bytes covering the entire firmware-reported table.
    /// Never trust an unchecked SDT length to create a slice from a raw pointer.
    pub fn parse(input: &'a [u8]) -> Result<Self, AcpiError> {
        let header = input.get(..SDT_HEADER_LEN).ok_or(AcpiError::Truncated)?;
        let total = usize::try_from(u32_at(header, 4)?).map_err(|_| AcpiError::InvalidLength)?;
        if !(SDT_HEADER_LEN..=MAX_TABLE_LEN).contains(&total) {
            return Err(AcpiError::InvalidLength);
        }
        let bytes = input.get(..total).ok_or(AcpiError::Truncated)?;
        if !checksum_ok(bytes) {
            return Err(AcpiError::Checksum);
        }
        let signature = header[..4].try_into().map_err(|_| AcpiError::Truncated)?;
        Ok(Self {
            signature,
            revision: header[8],
            bytes,
        })
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Table addresses come from a checked XSDT/RSDT payload, not Rust pointers.
    pub fn root_entries(&self) -> Result<RootEntries<'a>, AcpiError> {
        let stride = match &self.signature {
            b"XSDT" => 8,
            b"RSDT" => 4,
            _ => return Err(AcpiError::InvalidSignature),
        };
        let entries = &self.bytes[SDT_HEADER_LEN..];
        if entries.is_empty() || !entries.len().is_multiple_of(stride) {
            return Err(AcpiError::InvalidEntry);
        }
        Ok(RootEntries { entries, stride })
    }

    pub fn mcfg_entries(&self) -> Result<McfgEntries<'a>, AcpiError> {
        if &self.signature != b"MCFG" {
            return Err(AcpiError::InvalidSignature);
        }
        let bytes = self.bytes;
        if bytes.len() < MCFG_HEADER_LEN || bytes[SDT_HEADER_LEN..MCFG_HEADER_LEN] != [0; 8] {
            return Err(AcpiError::InvalidLength);
        }
        let entries = &bytes[MCFG_HEADER_LEN..];
        if entries.is_empty() || !entries.len().is_multiple_of(MCFG_ENTRY_LEN) {
            return Err(AcpiError::InvalidEntry);
        }
        let result = McfgEntries { entries };
        for (i, entry) in result.iter().enumerate() {
            let entry = entry?;
            for earlier in result.iter().take(i) {
                let earlier = earlier?;
                if earlier.segment == entry.segment
                    && earlier.bus_start <= entry.bus_end
                    && entry.bus_start <= earlier.bus_end
                {
                    return Err(AcpiError::InvalidAllocation);
                }
            }
        }
        Ok(result)
    }
}

pub struct RootEntries<'a> {
    entries: &'a [u8],
    stride: usize,
}

impl RootEntries<'_> {
    pub fn len(&self) -> usize {
        self.entries.len() / self.stride
    }

    pub fn address(&self, index: usize) -> Result<u64, AcpiError> {
        let offset = index.checked_mul(self.stride).ok_or(AcpiError::InvalidEntry)?;
        let address = if self.stride == 8 {
            u64_at(self.entries, offset)?
        } else {
            u64::from(u32_at(self.entries, offset)?)
        };
        if address == 0 {
            return Err(AcpiError::InvalidRootAddress);
        }
        Ok(address)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McfgEntry {
    pub ecam_base: u64,
    pub segment: u16,
    pub bus_start: u8,
    pub bus_end: u8,
}

impl McfgEntry {
    /// Compute an *unmapped physical address* for PCIe ECAM config space.
    /// The caller must map and validate the MMIO range before reading it.
    pub fn config_physical(
        self,
        bus: u8,
        device: u8,
        function: u8,
        register: u16,
    ) -> Result<u64, AcpiError> {
        if !(self.bus_start..=self.bus_end).contains(&bus)
            || device >= 32
            || function >= 8
            || register >= 4096
        {
            return Err(AcpiError::InvalidAllocation);
        }
        let offset = (u64::from(bus - self.bus_start) << 20)
            | (u64::from(device) << 15)
            | (u64::from(function) << 12)
            | u64::from(register);
        self.ecam_base.checked_add(offset).ok_or(AcpiError::AddressOverflow)
    }
}

pub struct McfgEntries<'a> {
    entries: &'a [u8],
}

impl McfgEntries<'_> {
    pub fn len(&self) -> usize {
        self.entries.len() / MCFG_ENTRY_LEN
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<McfgEntry, AcpiError>> + '_ {
        self.entries.chunks_exact(MCFG_ENTRY_LEN).map(|bytes| {
            let base = u64_at(bytes, 0)?;
            let segment = u16_at(bytes, 8)?;
            let start = bytes[10];
            let end = bytes[11];
            if base == 0
                || !base.is_multiple_of(ECAM_BUS_BYTES)
                || start > end
                || bytes[12..16] != [0; 4]
                || base
                    .checked_add((u64::from(end - start) + 1) * ECAM_BUS_BYTES)
                    .is_none()
            {
                return Err(AcpiError::InvalidAllocation);
            }
            Ok(McfgEntry {
                ecam_base: base,
                segment,
                bus_start: start,
                bus_end: end,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix_sum(bytes: &mut [u8], offset: usize, region_len: usize) {
        bytes[offset] = 0;
        let sum = bytes[..region_len]
            .iter()
            .fold(0u8, |value, &b| value.wrapping_add(b));
        bytes[offset] = 0u8.wrapping_sub(sum);
    }

    fn rsdp() -> Vec<u8> {
        let mut data = vec![0u8; 36];
        data[..8].copy_from_slice(b"RSD PTR ");
        data[15] = 2;
        data[16..20].copy_from_slice(&0x1000u32.to_le_bytes());
        data[20..24].copy_from_slice(&36u32.to_le_bytes());
        data[24..32].copy_from_slice(&0x1_0000_2000u64.to_le_bytes());
        fix_sum(&mut data, 8, 20);
        fix_sum(&mut data, 32, 36);
        data
    }

    fn table(signature: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 36 + payload.len()];
        bytes[..4].copy_from_slice(signature);
        let size = u32::try_from(bytes.len()).unwrap();
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        bytes[8] = 1;
        bytes[36..].copy_from_slice(payload);
        fix_sum(&mut bytes, 9, usize::try_from(size).unwrap());
        bytes
    }

    #[test]
    fn parses_rsdp_both_revisions_without_confusing_physical_with_virtual() {
        let bytes = rsdp();
        assert_eq!(
            parse_rsdp(&bytes).unwrap(),
            Rsdp {
                revision: 2,
                rsdt_physical: 0x1000,
                xsdt_physical: Some(0x1_0000_2000),
                byte_len: 36,
            }
        );
        let mut old = bytes[..20].to_vec();
        old[15] = 0;
        fix_sum(&mut old, 8, 20);
        assert_eq!(parse_rsdp(&old).unwrap().xsdt_physical, None);
    }

    #[test]
    fn rejects_truncated_bad_signature_and_two_separate_rsdp_checksums() {
        assert_eq!(parse_rsdp(&[0; 19]), Err(AcpiError::Truncated));
        let mut bytes = rsdp();
        bytes[0] = b'X';
        assert_eq!(parse_rsdp(&bytes), Err(AcpiError::InvalidSignature));
        let mut bytes = rsdp();
        bytes[9] ^= 1;
        assert_eq!(parse_rsdp(&bytes), Err(AcpiError::Checksum));
        let mut bytes = rsdp();
        bytes[33] ^= 1;
        assert_eq!(parse_rsdp(&bytes), Err(AcpiError::Checksum));
        let mut bytes = rsdp();
        bytes[20..24].copy_from_slice(&4097u32.to_le_bytes());
        assert_eq!(parse_rsdp(&bytes), Err(AcpiError::InvalidLength));
    }

    #[test]
    fn xsdt_and_rsdt_are_stride_checked_and_nonzero() {
        let raw = table(b"XSDT", &0x1_0000_2000u64.to_le_bytes());
        let parsed = Sdt::parse(&raw).unwrap();
        let entries = parsed.root_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries.address(0), Ok(0x1_0000_2000));
        assert_eq!(entries.address(1), Err(AcpiError::Truncated));
        let rsdt = table(b"RSDT", &0x2000u32.to_le_bytes());
        assert_eq!(
            Sdt::parse(&rsdt).unwrap().root_entries().unwrap().address(0),
            Ok(0x2000)
        );
        let raw = table(b"XSDT", &[0; 3]);
        assert!(matches!(
            Sdt::parse(&raw).unwrap().root_entries(),
            Err(AcpiError::InvalidEntry)
        ));
    }

    #[test]
    fn sdt_rejects_short_claimed_length_bad_crc_and_truncation() {
        let raw = table(b"XSDT", &8u64.to_le_bytes());
        let mut corrupt = raw.clone();
        corrupt[9] ^= 0x40;
        assert!(matches!(Sdt::parse(&corrupt), Err(AcpiError::Checksum)));
        let mut corrupt = raw.clone();
        corrupt[4..8].copy_from_slice(&35u32.to_le_bytes());
        assert!(matches!(Sdt::parse(&corrupt), Err(AcpiError::InvalidLength)));
        let mut corrupt = raw.clone();
        corrupt[4..8].copy_from_slice(&((MAX_TABLE_LEN + 1) as u32).to_le_bytes());
        assert!(matches!(Sdt::parse(&corrupt), Err(AcpiError::InvalidLength)));
        assert!(matches!(Sdt::parse(&raw[..38]), Err(AcpiError::Truncated)));
    }

    fn mcfg_entry(base: u64, segment: u16, start: u8, end: u8) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&base.to_le_bytes());
        bytes[8..10].copy_from_slice(&segment.to_le_bytes());
        bytes[10] = start;
        bytes[11] = end;
        bytes
    }

    #[test]
    fn mcfg_ranges_and_checked_ecam_config_address() {
        let mut payload = vec![0u8; 8];
        payload.extend_from_slice(&mcfg_entry(0xe000_0000, 0, 0x20, 0x2f));
        let raw = table(b"MCFG", &payload);
        let ranges = Sdt::parse(&raw).unwrap().mcfg_entries().unwrap();
        assert_eq!(ranges.len(), 1);
        let entry = ranges.iter().next().unwrap().unwrap();
        assert_eq!(entry.config_physical(0x20, 0, 0, 0), Ok(0xe000_0000));
        assert_eq!(
            entry.config_physical(0x21, 1, 2, 0xabc),
            Ok(0xe000_0000 + (1 << 20) + (1 << 15) + (2 << 12) + 0xabc)
        );
        assert_eq!(
            entry.config_physical(0x1f, 0, 0, 0),
            Err(AcpiError::InvalidAllocation)
        );
        assert_eq!(
            entry.config_physical(0x20, 32, 0, 0),
            Err(AcpiError::InvalidAllocation)
        );
        assert_eq!(
            entry.config_physical(0x20, 0, 8, 0),
            Err(AcpiError::InvalidAllocation)
        );
        assert_eq!(
            entry.config_physical(0x20, 0, 0, 4096),
            Err(AcpiError::InvalidAllocation)
        );
    }

    #[test]
    fn mcfg_rejects_bad_alignment_overflow_reserved_and_bus_overlap() {
        for entry in [
            mcfg_entry(0xe000_1000, 0, 0, 1),
            mcfg_entry(u64::MAX & !(ECAM_BUS_BYTES - 1), 0, 0, 255),
            mcfg_entry(0xe000_0000, 0, 2, 1),
        ] {
            let mut payload = vec![0u8; 8];
            payload.extend_from_slice(&entry);
            let raw = table(b"MCFG", &payload);
            assert!(matches!(
                Sdt::parse(&raw).unwrap().mcfg_entries(),
                Err(AcpiError::InvalidAllocation)
            ));
        }
        let mut payload = vec![0u8; 8];
        payload.extend_from_slice(&mcfg_entry(0xe000_0000, 0, 0, 5));
        payload.extend_from_slice(&mcfg_entry(0xf000_0000, 0, 5, 8));
        let raw = table(b"MCFG", &payload);
        assert!(matches!(
            Sdt::parse(&raw).unwrap().mcfg_entries(),
            Err(AcpiError::InvalidAllocation)
        ));
        let mut payload = vec![0u8; 8];
        payload[0] = 1;
        payload.extend_from_slice(&mcfg_entry(0xe000_0000, 0, 0, 1));
        let raw = table(b"MCFG", &payload);
        assert!(matches!(
            Sdt::parse(&raw).unwrap().mcfg_entries(),
            Err(AcpiError::InvalidLength)
        ));
    }
}
