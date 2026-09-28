//! VibrixFS v1 redo-journal wire codec and fail-closed transaction validator.
//!
//! Host-only conformance code for ADR 0011. It does not open files or devices,
//! issue writes, replay metadata, or claim crash recovery is implemented.

const BLOCK: usize = 4096;
const VERSION: u16 = 1;
const HEADER: u16 = 64;
const MAX_ENTRIES: usize = 64;
const MANIFEST_MAGIC: &[u8; 8] = b"VJMANF01";
const COMMIT_MAGIC: &[u8; 8] = b"VJCOMT01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub home_block: u64,
    pub data_crc32: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub transaction_id: u64,
    pub previous_generation: u64,
    pub new_generation: u64,
    pub journal_start: u64,
    pub journal_blocks: u64,
    pub entry_count: u16,
    pub descriptors: [Descriptor; MAX_ENTRIES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commit {
    pub transaction_id: u64,
    pub previous_generation: u64,
    pub new_generation: u64,
    pub entry_count: u16,
    pub manifest_crc32: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Magic,
    Version,
    Reserved,
    Checksum,
    Count,
    Generation,
    Geometry,
    Target,
    DuplicateTarget,
    Payload,
    CommitMismatch,
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}

fn crc_with_zeroed_u32(block: &[u8; BLOCK], at: usize) -> u32 {
    let mut copy = *block;
    copy[at..at + 4].fill(0);
    crc32(&copy)
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn u64_at(data: &[u8], at: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        data.get(at..at + 8)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn manifest_crc(block: &[u8; BLOCK]) -> u32 {
    crc_with_zeroed_u32(block, 40)
}

pub fn encode_manifest(
    transaction_id: u64,
    previous_generation: u64,
    new_generation: u64,
    journal_start: u64,
    journal_blocks: u64,
    entries: &[(u64, &[u8; BLOCK])],
) -> Result<[u8; BLOCK], Error> {
    if transaction_id == 0
        || entries.is_empty()
        || entries.len() > MAX_ENTRIES
        || new_generation != previous_generation.checked_add(1).ok_or(Error::Generation)?
    {
        return Err(Error::Generation);
    }
    let needed = u64::try_from(entries.len())
        .map_err(|_| Error::Count)?
        .checked_add(2)
        .ok_or(Error::Geometry)?;
    if journal_start == 0 || journal_blocks < needed {
        return Err(Error::Geometry);
    }
    let mut out = [0u8; BLOCK];
    out[..8].copy_from_slice(MANIFEST_MAGIC);
    out[8..10].copy_from_slice(&VERSION.to_le_bytes());
    out[10..12].copy_from_slice(&HEADER.to_le_bytes());
    out[12..14].copy_from_slice(
        &u16::try_from(entries.len())
            .map_err(|_| Error::Count)?
            .to_le_bytes(),
    );
    out[16..24].copy_from_slice(&transaction_id.to_le_bytes());
    out[24..32].copy_from_slice(&previous_generation.to_le_bytes());
    out[32..40].copy_from_slice(&new_generation.to_le_bytes());
    out[48..56].copy_from_slice(&journal_start.to_le_bytes());
    out[56..64].copy_from_slice(&journal_blocks.to_le_bytes());
    for (index, (home, data)) in entries.iter().enumerate() {
        let at = 64 + index * 16;
        out[at..at + 8].copy_from_slice(&home.to_le_bytes());
        out[at + 8..at + 12].copy_from_slice(&crc32(*data).to_le_bytes());
    }
    let checksum = manifest_crc(&out);
    out[40..44].copy_from_slice(&checksum.to_le_bytes());
    Ok(out)
}

pub fn parse_manifest(data: &[u8]) -> Result<Manifest, Error> {
    let block: &[u8; BLOCK] = data
        .get(..BLOCK)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    if &block[..8] != MANIFEST_MAGIC {
        return Err(Error::Magic);
    }
    if u16_at(block, 8)? != VERSION || u16_at(block, 10)? != HEADER {
        return Err(Error::Version);
    }
    if u16_at(block, 14)? != 0 || u32_at(block, 44)? != 0 {
        return Err(Error::Reserved);
    }
    let count = usize::from(u16_at(block, 12)?);
    if count == 0 || count > MAX_ENTRIES {
        return Err(Error::Count);
    }
    let used = 64usize
        .checked_add(count.checked_mul(16).ok_or(Error::Count)?)
        .ok_or(Error::Count)?;
    if block[used..].iter().any(|&byte| byte != 0) {
        return Err(Error::Reserved);
    }
    if u32_at(block, 40)? != manifest_crc(block) {
        return Err(Error::Checksum);
    }
    let mut descriptors = [Descriptor {
        home_block: 0,
        data_crc32: 0,
    }; MAX_ENTRIES];
    for (index, descriptor) in descriptors[..count].iter_mut().enumerate() {
        let at = 64 + index * 16;
        if u32_at(block, at + 12)? != 0 {
            return Err(Error::Reserved);
        }
        *descriptor = Descriptor {
            home_block: u64_at(block, at)?,
            data_crc32: u32_at(block, at + 8)?,
        };
    }
    let manifest = Manifest {
        transaction_id: u64_at(block, 16)?,
        previous_generation: u64_at(block, 24)?,
        new_generation: u64_at(block, 32)?,
        journal_start: u64_at(block, 48)?,
        journal_blocks: u64_at(block, 56)?,
        entry_count: u16_at(block, 12)?,
        descriptors,
    };
    if manifest.transaction_id == 0
        || manifest.new_generation
            != manifest
                .previous_generation
                .checked_add(1)
                .ok_or(Error::Generation)?
    {
        return Err(Error::Generation);
    }
    Ok(manifest)
}

pub fn encode_commit(manifest_block: &[u8; BLOCK]) -> Result<[u8; BLOCK], Error> {
    let manifest = parse_manifest(manifest_block)?;
    let mut out = [0u8; BLOCK];
    out[..8].copy_from_slice(COMMIT_MAGIC);
    out[8..10].copy_from_slice(&VERSION.to_le_bytes());
    out[10..12].copy_from_slice(&HEADER.to_le_bytes());
    out[12..14].copy_from_slice(&manifest.entry_count.to_le_bytes());
    out[16..24].copy_from_slice(&manifest.transaction_id.to_le_bytes());
    out[24..32].copy_from_slice(&manifest.previous_generation.to_le_bytes());
    out[32..40].copy_from_slice(&manifest.new_generation.to_le_bytes());
    out[40..44].copy_from_slice(&u32_at(manifest_block, 40)?.to_le_bytes());
    let checksum = crc_with_zeroed_u32(&out, 44);
    out[44..48].copy_from_slice(&checksum.to_le_bytes());
    Ok(out)
}

pub fn parse_commit(data: &[u8]) -> Result<Commit, Error> {
    let block: &[u8; BLOCK] = data
        .get(..BLOCK)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    if &block[..8] != COMMIT_MAGIC {
        return Err(Error::Magic);
    }
    if u16_at(block, 8)? != VERSION || u16_at(block, 10)? != HEADER {
        return Err(Error::Version);
    }
    if u16_at(block, 14)? != 0 || block[48..].iter().any(|&byte| byte != 0) {
        return Err(Error::Reserved);
    }
    let count = u16_at(block, 12)?;
    if count == 0 || usize::from(count) > MAX_ENTRIES {
        return Err(Error::Count);
    }
    if u32_at(block, 44)? != crc_with_zeroed_u32(block, 44) {
        return Err(Error::Checksum);
    }
    let commit = Commit {
        transaction_id: u64_at(block, 16)?,
        previous_generation: u64_at(block, 24)?,
        new_generation: u64_at(block, 32)?,
        entry_count: count,
        manifest_crc32: u32_at(block, 40)?,
    };
    if commit.transaction_id == 0
        || commit.new_generation
            != commit
                .previous_generation
                .checked_add(1)
                .ok_or(Error::Generation)?
    {
        return Err(Error::Generation);
    }
    Ok(commit)
}

pub fn validate_transaction(
    manifest_block: &[u8; BLOCK],
    payloads: &[[u8; BLOCK]],
    commit_block: &[u8; BLOCK],
    total_blocks: u64,
) -> Result<Manifest, Error> {
    let manifest = parse_manifest(manifest_block)?;
    let commit = parse_commit(commit_block)?;
    let count = usize::from(manifest.entry_count);
    if payloads.len() != count {
        return Err(Error::Count);
    }
    if commit.transaction_id != manifest.transaction_id
        || commit.previous_generation != manifest.previous_generation
        || commit.new_generation != manifest.new_generation
        || commit.entry_count != manifest.entry_count
        || commit.manifest_crc32 != u32_at(manifest_block, 40)?
    {
        return Err(Error::CommitMismatch);
    }
    let journal_end = manifest
        .journal_start
        .checked_add(manifest.journal_blocks)
        .ok_or(Error::Geometry)?;
    let needed_end = manifest
        .journal_start
        .checked_add(u64::from(manifest.entry_count))
        .and_then(|n| n.checked_add(2))
        .ok_or(Error::Geometry)?;
    if total_blocks < 3
        || manifest.journal_start == 0
        || journal_end > total_blocks - 1
        || needed_end > journal_end
    {
        return Err(Error::Geometry);
    }
    for index in 0..count {
        let descriptor = manifest.descriptors[index];
        if descriptor.home_block == 0
            || descriptor.home_block >= total_blocks - 1
            || (manifest.journal_start..journal_end).contains(&descriptor.home_block)
        {
            return Err(Error::Target);
        }
        if manifest.descriptors[..index]
            .iter()
            .any(|other| other.home_block == descriptor.home_block)
        {
            return Err(Error::DuplicateTarget);
        }
        if crc32(&payloads[index]) != descriptor.data_crc32 {
            return Err(Error::Payload);
        }
    }
    Ok(manifest)
}

#[cfg(not(test))]
fn main() {
    eprintln!("VibrixFS journal wire conformance helper: run with rustc --test");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(fill: u8) -> [u8; BLOCK] {
        [fill; BLOCK]
    }

    fn transaction() -> ([u8; BLOCK], [[u8; BLOCK]; 2], [u8; BLOCK]) {
        let payloads = [block(0x11), block(0x22)];
        let manifest =
            encode_manifest(7, 40, 41, 100, 8, &[(10, &payloads[0]), (11, &payloads[1])])
                .unwrap();
        let commit = encode_commit(&manifest).unwrap();
        (manifest, payloads, commit)
    }

    #[test]
    fn round_trip_committed_full_block_redo_transaction() {
        let (manifest_block, payloads, commit_block) = transaction();
        let manifest =
            validate_transaction(&manifest_block, &payloads, &commit_block, 4096).unwrap();
        assert_eq!(manifest.transaction_id, 7);
        assert_eq!(manifest.previous_generation, 40);
        assert_eq!(manifest.new_generation, 41);
        assert_eq!(manifest.entry_count, 2);
        assert_eq!(manifest.descriptors[0].home_block, 10);
        assert_eq!(manifest.descriptors[1].home_block, 11);
    }

    #[test]
    fn payload_manifest_and_commit_corruption_fail_closed() {
        let (manifest, payloads, commit) = transaction();
        let mut bad_payloads = payloads;
        bad_payloads[0][123] ^= 1;
        assert_eq!(
            validate_transaction(&manifest, &bad_payloads, &commit, 4096),
            Err(Error::Payload)
        );
        let mut bad_manifest = manifest;
        bad_manifest[64] ^= 1;
        assert_eq!(parse_manifest(&bad_manifest), Err(Error::Checksum));
        let mut bad_commit = commit;
        bad_commit[20] ^= 1;
        assert_eq!(parse_commit(&bad_commit), Err(Error::Checksum));
    }

    #[test]
    fn commit_must_bind_exact_manifest_identity_and_generation() {
        let (manifest, payloads, mut commit) = transaction();
        commit[40] ^= 1;
        let checksum = crc_with_zeroed_u32(&commit, 44);
        commit[44..48].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            validate_transaction(&manifest, &payloads, &commit, 4096),
            Err(Error::CommitMismatch)
        );
    }

    #[test]
    fn duplicate_out_of_range_and_journal_targets_are_rejected() {
        let payloads = [block(1), block(2)];
        for homes in [[9u64, 9], [0, 8], [101, 8], [4095, 8]] {
            let manifest =
                encode_manifest(1, 0, 1, 100, 8, &[(homes[0], &payloads[0]), (homes[1], &payloads[1])])
                    .unwrap();
            let commit = encode_commit(&manifest).unwrap();
            assert!(validate_transaction(&manifest, &payloads, &commit, 4096).is_err());
        }
    }

    #[test]
    fn journal_geometry_and_generation_are_checked_before_use() {
        let payload = block(3);
        assert_eq!(
            encode_manifest(1, 9, 11, 100, 4, &[(8, &payload)]),
            Err(Error::Generation)
        );
        assert_eq!(
            encode_manifest(1, 9, 10, 0, 4, &[(8, &payload)]),
            Err(Error::Geometry)
        );
        let manifest = encode_manifest(1, 9, 10, 4090, 6, &[(8, &payload)]).unwrap();
        let commit = encode_commit(&manifest).unwrap();
        assert_eq!(
            validate_transaction(&manifest, &[payload], &commit, 4096),
            Err(Error::Geometry)
        );
    }

    #[test]
    fn reserved_bytes_and_count_mismatch_are_rejected() {
        let (manifest, payloads, commit) = transaction();
        let mut bad = manifest;
        bad[15] = 1;
        let checksum = manifest_crc(&bad);
        bad[40..44].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(parse_manifest(&bad), Err(Error::Reserved));
        assert_eq!(
            validate_transaction(&manifest, &payloads[..1], &commit, 4096),
            Err(Error::Count)
        );
    }
}
