#![no_std]

#[cfg(test)]
extern crate std;

pub const MAGIC: [u8; 4] = *b"VPK1";
pub const FORMAT_VERSION: u16 = 1;
pub const MAX_NAME: usize = 31;
pub const MAX_DEPENDENCIES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Name {
    bytes: [u8; MAX_NAME],
    len: u8,
}

impl Name {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MAX_NAME {
            return Err(Error::InvalidName);
        }
        if !bytes.iter().copied().all(valid_name_byte) {
            return Err(Error::InvalidName);
        }
        let mut out = [0; MAX_NAME];
        out[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: out,
            len: bytes.len() as u8,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

const fn valid_name_byte(byte: u8) -> bool {
    matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.')
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dependency {
    pub name: Name,
    pub minimum: Version,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Manifest {
    pub name: Name,
    pub version: Version,
    pub payload_len: u64,
    pub payload_sha256: [u8; 32],
    dependencies: [Option<Dependency>; MAX_DEPENDENCIES],
    dependency_count: u8,
}

impl Manifest {
    pub fn dependencies(&self) -> impl Iterator<Item = Dependency> + '_ {
        self.dependencies[..usize::from(self.dependency_count)]
            .iter()
            .flatten()
            .copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledPackage {
    pub name: Name,
    pub version: Version,
    pub payload_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    BadMagic,
    UnsupportedVersion,
    InvalidName,
    TooManyDependencies,
    DuplicateDependency,
    TrailingBytes,
    DatabaseFull,
    DuplicatePackage,
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, Error> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(4)? != MAGIC.as_slice() {
        return Err(Error::BadMagic);
    }
    if cursor.u16()? != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let name_len = usize::from(cursor.u8()?);
    let dependency_count = usize::from(cursor.u8()?);
    if dependency_count > MAX_DEPENDENCIES {
        return Err(Error::TooManyDependencies);
    }
    let version = Version::new(cursor.u16()?, cursor.u16()?, cursor.u16()?);
    let payload_len = cursor.u64()?;
    let mut payload_sha256 = [0; 32];
    payload_sha256.copy_from_slice(cursor.take(32)?);
    let name = Name::parse(cursor.take(name_len)?)?;

    let mut dependencies: [Option<Dependency>; MAX_DEPENDENCIES] = [None; MAX_DEPENDENCIES];
    for index in 0..dependency_count {
        let dep_name_len = usize::from(cursor.u8()?);
        let dep_name = Name::parse(cursor.take(dep_name_len)?)?;
        let minimum = Version::new(cursor.u16()?, cursor.u16()?, cursor.u16()?);
        if dependencies[..index]
            .iter()
            .flatten()
            .any(|existing| existing.name == dep_name)
        {
            return Err(Error::DuplicateDependency);
        }
        dependencies[index] = Some(Dependency {
            name: dep_name,
            minimum,
        });
    }
    if cursor.remaining() != 0 {
        return Err(Error::TrailingBytes);
    }

    Ok(Manifest {
        name,
        version,
        payload_len,
        payload_sha256,
        dependencies,
        dependency_count: dependency_count as u8,
    })
}

pub struct Database<const N: usize> {
    entries: [Option<InstalledPackage>; N],
    len: usize,
}

impl<const N: usize> Default for Database<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Database<N> {
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, name: Name) -> Option<InstalledPackage> {
        self.entries[..self.len]
            .iter()
            .flatten()
            .copied()
            .find(|entry| entry.name == name)
    }

    pub fn insert(&mut self, package: InstalledPackage) -> Result<(), Error> {
        if self.get(package.name).is_some() {
            return Err(Error::DuplicatePackage);
        }
        if self.len == N {
            return Err(Error::DatabaseFull);
        }
        self.entries[self.len] = Some(package);
        self.len += 1;
        Ok(())
    }

    pub fn dependencies_satisfied(&self, manifest: &Manifest) -> bool {
        manifest.dependencies().all(|dependency| {
            self.get(dependency.name)
                .is_some_and(|installed| installed.version >= dependency.minimum)
        })
    }

    pub fn missing_dependency_mask(&self, manifest: &Manifest) -> u8 {
        let mut mask = 0u8;
        for (index, dependency) in manifest.dependencies().enumerate() {
            let satisfied = self
                .get(dependency.name)
                .is_some_and(|installed| installed.version >= dependency.minimum);
            if !satisfied {
                mask |= 1 << index;
            }
        }
        mask
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.checked_add(len).ok_or(Error::Truncated)?;
        let slice = self.bytes.get(self.offset..end).ok_or(Error::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let raw = self.take(2)?;
        Ok(u16::from_le_bytes([raw[0], raw[1]]))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let raw = self.take(8)?;
        Ok(u64::from_le_bytes([
            raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    fn manifest_bytes(name: &[u8], version: Version, deps: &[(&[u8], Version)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.push(name.len() as u8);
        out.push(deps.len() as u8);
        out.extend_from_slice(&version.major.to_le_bytes());
        out.extend_from_slice(&version.minor.to_le_bytes());
        out.extend_from_slice(&version.patch.to_le_bytes());
        out.extend_from_slice(&4096u64.to_le_bytes());
        out.extend_from_slice(&[0x5a; 32]);
        out.extend_from_slice(name);
        for (dep_name, dep_version) in deps {
            out.push(dep_name.len() as u8);
            out.extend_from_slice(dep_name);
            out.extend_from_slice(&dep_version.major.to_le_bytes());
            out.extend_from_slice(&dep_version.minor.to_le_bytes());
            out.extend_from_slice(&dep_version.patch.to_le_bytes());
        }
        out
    }

    #[test]
    fn parses_bounded_manifest_and_dependencies() {
        let bytes = manifest_bytes(
            b"editor",
            Version::new(1, 2, 3),
            &[
                (b"libui", Version::new(2, 0, 0)),
                (b"term", Version::new(1, 4, 0)),
            ],
        );
        let manifest = parse_manifest(&bytes).unwrap();
        assert_eq!(manifest.name.as_bytes(), b"editor");
        assert_eq!(manifest.version, Version::new(1, 2, 3));
        assert_eq!(manifest.payload_len, 4096);
        assert_eq!(manifest.dependencies().count(), 2);
    }

    #[test]
    fn rejects_malformed_or_ambiguous_metadata() {
        let mut bad_magic = manifest_bytes(b"pkg", Version::new(1, 0, 0), &[]);
        bad_magic[0] = 0;
        assert_eq!(parse_manifest(&bad_magic), Err(Error::BadMagic));

        let invalid_name = manifest_bytes(b"Bad Name", Version::new(1, 0, 0), &[]);
        assert_eq!(parse_manifest(&invalid_name), Err(Error::InvalidName));

        let duplicate = manifest_bytes(
            b"pkg",
            Version::new(1, 0, 0),
            &[
                (b"dep", Version::new(1, 0, 0)),
                (b"dep", Version::new(2, 0, 0)),
            ],
        );
        assert_eq!(parse_manifest(&duplicate), Err(Error::DuplicateDependency));

        let mut trailing = manifest_bytes(b"pkg", Version::new(1, 0, 0), &[]);
        trailing.push(0);
        assert_eq!(parse_manifest(&trailing), Err(Error::TrailingBytes));
    }

    #[test]
    fn database_is_unique_bounded_and_version_aware() {
        let dep = Name::parse(b"libui").unwrap();
        let mut database = Database::<1>::new();
        database
            .insert(InstalledPackage {
                name: dep,
                version: Version::new(2, 1, 0),
                payload_sha256: [1; 32],
            })
            .unwrap();
        assert_eq!(database.len(), 1);
        assert_eq!(
            database.insert(InstalledPackage {
                name: dep,
                version: Version::new(3, 0, 0),
                payload_sha256: [2; 32],
            }),
            Err(Error::DuplicatePackage)
        );
        assert_eq!(
            database.insert(InstalledPackage {
                name: Name::parse(b"other").unwrap(),
                version: Version::new(1, 0, 0),
                payload_sha256: [3; 32],
            }),
            Err(Error::DatabaseFull)
        );
    }

    #[test]
    fn dependency_resolution_reports_missing_or_old_entries() {
        let bytes = manifest_bytes(
            b"editor",
            Version::new(1, 0, 0),
            &[
                (b"libui", Version::new(2, 0, 0)),
                (b"term", Version::new(1, 4, 0)),
            ],
        );
        let manifest = parse_manifest(&bytes).unwrap();
        let mut database = Database::<4>::new();
        database
            .insert(InstalledPackage {
                name: Name::parse(b"libui").unwrap(),
                version: Version::new(2, 0, 0),
                payload_sha256: [0; 32],
            })
            .unwrap();
        database
            .insert(InstalledPackage {
                name: Name::parse(b"term").unwrap(),
                version: Version::new(1, 3, 9),
                payload_sha256: [0; 32],
            })
            .unwrap();
        assert!(!database.dependencies_satisfied(&manifest));
        assert_eq!(database.missing_dependency_mask(&manifest), 0b10);
    }
}
