#![no_std]

//! Vibrix package metadata/database foundation.
//!
//! VPKG v1 is a fixed-size, allocation-free package manifest. It deliberately
//! defines only package identity, a numeric MAJOR.MINOR.PATCH version, payload
//! size/hash and bounded minimum-version dependencies. Repository signatures,
//! archives, installation transactions and package-manager UX are separate
//! milestones.

pub const WIRE_BYTES: usize = 512;
pub const NAME_BYTES: usize = 32;
pub const MAX_DEPENDENCIES: usize = 8;
pub const DATABASE_CAPACITY: usize = 32;
const MAGIC: [u8; 8] = *b"VPKGv001";
const DEPENDENCY_BYTES: usize = 48;
const DEPENDENCY_BASE: usize = 100;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub const fn satisfies_minimum(self, minimum: Self) -> bool {
        if self.major != minimum.major {
            return self.major > minimum.major;
        }
        if self.minor != minimum.minor {
            return self.minor > minimum.minor;
        }
        self.patch >= minimum.patch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Name {
    len: u8,
    bytes: [u8; NAME_BYTES],
}

impl Name {
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0; NAME_BYTES],
    };

    pub fn new(value: &[u8]) -> Result<Self, Error> {
        if value.is_empty() || value.len() > NAME_BYTES {
            return Err(Error::Name);
        }
        if !value.iter().copied().all(valid_name_byte) {
            return Err(Error::Name);
        }
        let mut result = Self::EMPTY;
        result.len = value.len() as u8;
        result.bytes[..value.len()].copy_from_slice(value);
        Ok(result)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

fn valid_name_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
}

pub const CAPABILITY_WIRE_BYTES: usize = 64;
const CAPABILITY_MAGIC: [u8; 8] = *b"VCAPv001";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities(u64);

impl Capabilities {
    pub const FILESYSTEM_READ: Self = Self(1 << 0);
    pub const FILESYSTEM_WRITE: Self = Self(1 << 1);
    pub const NETWORK: Self = Self(1 << 2);
    pub const DEVICE: Self = Self(1 << 3);
    pub const PROCESS_CONTROL: Self = Self(1 << 4);
    pub const ALL: Self = Self(
        Self::FILESYSTEM_READ.0
            | Self::FILESYSTEM_WRITE.0
            | Self::NETWORK.0
            | Self::DEVICE.0
            | Self::PROCESS_CONTROL.0,
    );

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn from_bits(bits: u64) -> Result<Self, Error> {
        if bits & !Self::ALL.0 != 0 {
            return Err(Error::UnknownCapabilities);
        }
        Ok(Self(bits))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDeclaration {
    pub package: Name,
    pub capabilities: Capabilities,
}

impl CapabilityDeclaration {
    pub const fn new(package: Name, capabilities: Capabilities) -> Self {
        Self {
            package,
            capabilities,
        }
    }

    pub fn encode(self) -> [u8; CAPABILITY_WIRE_BYTES] {
        let mut wire = [0u8; CAPABILITY_WIRE_BYTES];
        wire[..8].copy_from_slice(&CAPABILITY_MAGIC);
        wire[8] = self.package.len;
        put_u64(&mut wire[16..24], self.capabilities.bits());
        wire[24..56].copy_from_slice(&self.package.bytes);
        wire
    }

    pub fn decode(wire: &[u8]) -> Result<Self, Error> {
        if wire.len() != CAPABILITY_WIRE_BYTES || wire[..8] != CAPABILITY_MAGIC {
            return Err(Error::Format);
        }
        if wire[9..16].iter().any(|&byte| byte != 0)
            || wire[56..].iter().any(|&byte| byte != 0)
        {
            return Err(Error::Reserved);
        }
        let package = decode_name(wire[8], &wire[24..56])?;
        let capabilities = Capabilities::from_bits(get_u64(&wire[16..24]))?;
        Ok(Self {
            package,
            capabilities,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dependency {
    pub name: Name,
    pub minimum: Version,
}

impl Dependency {
    pub const EMPTY: Self = Self {
        name: Name::EMPTY,
        minimum: Version::new(0, 0, 0),
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Manifest {
    pub name: Name,
    pub version: Version,
    pub payload_size: u64,
    pub payload_hash: [u8; 32],
    dependencies: [Dependency; MAX_DEPENDENCIES],
    dependency_count: u8,
}

impl Manifest {
    pub fn new(
        name: Name,
        version: Version,
        payload_size: u64,
        payload_hash: [u8; 32],
        dependencies: &[Dependency],
    ) -> Result<Self, Error> {
        if dependencies.len() > MAX_DEPENDENCIES {
            return Err(Error::TooManyDependencies);
        }
        if payload_size == 0 {
            return Err(Error::Payload);
        }
        let mut stored = [Dependency::EMPTY; MAX_DEPENDENCIES];
        for (index, dependency) in dependencies.iter().copied().enumerate() {
            if dependency.name == name {
                return Err(Error::SelfDependency);
            }
            if dependencies[..index]
                .iter()
                .any(|earlier| earlier.name == dependency.name)
            {
                return Err(Error::DuplicateDependency);
            }
            stored[index] = dependency;
        }
        Ok(Self {
            name,
            version,
            payload_size,
            payload_hash,
            dependencies: stored,
            dependency_count: dependencies.len() as u8,
        })
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies[..usize::from(self.dependency_count)]
    }

    pub fn encode(&self) -> [u8; WIRE_BYTES] {
        let mut wire = [0u8; WIRE_BYTES];
        wire[..8].copy_from_slice(&MAGIC);
        wire[8] = self.name.len;
        wire[9] = self.dependency_count;
        put_u32(&mut wire[16..20], self.version.major);
        put_u32(&mut wire[20..24], self.version.minor);
        put_u32(&mut wire[24..28], self.version.patch);
        put_u64(&mut wire[28..36], self.payload_size);
        wire[36..68].copy_from_slice(&self.payload_hash);
        wire[68..100].copy_from_slice(&self.name.bytes);
        for (index, dependency) in self.dependencies().iter().enumerate() {
            let base = DEPENDENCY_BASE + index * DEPENDENCY_BYTES;
            wire[base] = dependency.name.len;
            put_u32(&mut wire[base + 4..base + 8], dependency.minimum.major);
            put_u32(&mut wire[base + 8..base + 12], dependency.minimum.minor);
            put_u32(&mut wire[base + 12..base + 16], dependency.minimum.patch);
            wire[base + 16..base + 48].copy_from_slice(&dependency.name.bytes);
        }
        wire
    }

    pub fn decode(wire: &[u8]) -> Result<Self, Error> {
        if wire.len() != WIRE_BYTES || wire[..8] != MAGIC {
            return Err(Error::Format);
        }
        if wire[10..16].iter().any(|&byte| byte != 0) || wire[484..].iter().any(|&byte| byte != 0) {
            return Err(Error::Reserved);
        }
        let dependency_count = usize::from(wire[9]);
        if dependency_count > MAX_DEPENDENCIES {
            return Err(Error::TooManyDependencies);
        }
        let name = decode_name(wire[8], &wire[68..100])?;
        let version = Version::new(
            get_u32(&wire[16..20]),
            get_u32(&wire[20..24]),
            get_u32(&wire[24..28]),
        );
        let payload_size = get_u64(&wire[28..36]);
        if payload_size == 0 {
            return Err(Error::Payload);
        }
        let mut payload_hash = [0u8; 32];
        payload_hash.copy_from_slice(&wire[36..68]);

        let mut dependencies = [Dependency::EMPTY; MAX_DEPENDENCIES];
        for (index, dependency) in dependencies.iter_mut().enumerate().take(dependency_count) {
            let base = DEPENDENCY_BASE + index * DEPENDENCY_BYTES;
            if wire[base + 1..base + 4].iter().any(|&byte| byte != 0) {
                return Err(Error::Reserved);
            }
            *dependency = Dependency {
                name: decode_name(wire[base], &wire[base + 16..base + 48])?,
                minimum: Version::new(
                    get_u32(&wire[base + 4..base + 8]),
                    get_u32(&wire[base + 8..base + 12]),
                    get_u32(&wire[base + 12..base + 16]),
                ),
            };
        }
        for index in dependency_count..MAX_DEPENDENCIES {
            let base = DEPENDENCY_BASE + index * DEPENDENCY_BYTES;
            if wire[base..base + DEPENDENCY_BYTES]
                .iter()
                .any(|&byte| byte != 0)
            {
                return Err(Error::Reserved);
            }
        }
        Self::new(
            name,
            version,
            payload_size,
            payload_hash,
            &dependencies[..dependency_count],
        )
    }
}

fn decode_name(len: u8, bytes: &[u8]) -> Result<Name, Error> {
    let len = usize::from(len);
    if len == 0 || len > NAME_BYTES || bytes.len() != NAME_BYTES {
        return Err(Error::Name);
    }
    if bytes[len..].iter().any(|&byte| byte != 0) {
        return Err(Error::Reserved);
    }
    Name::new(&bytes[..len])
}

fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&value.to_le_bytes());
}

fn get_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes(input.try_into().expect("fixed u32 wire field"))
}

fn put_u64(output: &mut [u8], value: u64) {
    output.copy_from_slice(&value.to_le_bytes());
}

fn get_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes(input.try_into().expect("fixed u64 wire field"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Format,
    Reserved,
    Name,
    Payload,
    TooManyDependencies,
    DuplicateDependency,
    SelfDependency,
    AlreadyInstalled,
    MissingDependency,
    DependencyVersion,
    RequiredByInstalled,
    DatabaseFull,
    NotInstalled,
    UnknownCapabilities,
}

pub struct Database {
    packages: [Option<Manifest>; DATABASE_CAPACITY],
    used: usize,
}

impl Database {
    pub const fn new() -> Self {
        Self {
            packages: [None; DATABASE_CAPACITY],
            used: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.used
    }

    pub const fn is_empty(&self) -> bool {
        self.used == 0
    }

    pub fn get(&self, name: Name) -> Option<&Manifest> {
        self.packages[..self.used]
            .iter()
            .flatten()
            .find(|package| package.name == name)
    }

    pub fn validate_dependencies(&self, manifest: &Manifest) -> Result<(), Error> {
        for dependency in manifest.dependencies() {
            let installed = self.get(dependency.name).ok_or(Error::MissingDependency)?;
            if !installed.version.satisfies_minimum(dependency.minimum) {
                return Err(Error::DependencyVersion);
            }
        }
        Ok(())
    }

    pub fn install(&mut self, manifest: Manifest) -> Result<(), Error> {
        if self.get(manifest.name).is_some() {
            return Err(Error::AlreadyInstalled);
        }
        if self.used == self.packages.len() {
            return Err(Error::DatabaseFull);
        }
        self.validate_dependencies(&manifest)?;
        self.packages[self.used] = Some(manifest);
        self.used += 1;
        Ok(())
    }

    pub fn remove(&mut self, name: Name) -> Result<Manifest, Error> {
        let index = self.packages[..self.used]
            .iter()
            .position(|slot| slot.is_some_and(|package| package.name == name))
            .ok_or(Error::NotInstalled)?;
        if self.packages[..self.used].iter().flatten().any(|package| {
            package.name != name && package.dependencies().iter().any(|dep| dep.name == name)
        }) {
            return Err(Error::RequiredByInstalled);
        }
        let removed = self.packages[index].take().ok_or(Error::NotInstalled)?;
        for cursor in index..self.used - 1 {
            self.packages[cursor] = self.packages[cursor + 1];
        }
        self.used -= 1;
        self.packages[self.used] = None;
        Ok(removed)
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &[u8]) -> Name {
        Name::new(value).unwrap()
    }

    fn manifest(name_bytes: &[u8], version: Version, dependencies: &[Dependency]) -> Manifest {
        Manifest::new(name(name_bytes), version, 1234, [0xa5; 32], dependencies).unwrap()
    }

    #[test]
    fn wire_round_trip_is_canonical_and_reserved_bytes_fail_closed() {
        let dep = Dependency {
            name: name(b"core"),
            minimum: Version::new(1, 2, 3),
        };
        let source = manifest(b"shell", Version::new(2, 0, 1), &[dep]);
        let wire = source.encode();
        assert_eq!(&wire[..8], b"VPKGv001");
        assert_eq!(wire.len(), WIRE_BYTES);
        assert_eq!(Manifest::decode(&wire), Ok(source));

        let mut corrupt = wire;
        corrupt[10] = 1;
        assert_eq!(Manifest::decode(&corrupt), Err(Error::Reserved));
        let mut corrupt = wire;
        corrupt[500] = 1;
        assert_eq!(Manifest::decode(&corrupt), Err(Error::Reserved));
    }

    #[test]
    fn names_and_dependency_lists_are_strict() {
        for invalid in [
            &b""[..],
            &b"Upper"[..],
            &b"space name"[..],
            &[b'a'; NAME_BYTES + 1][..],
        ] {
            assert_eq!(Name::new(invalid), Err(Error::Name));
        }
        let self_dep = Dependency {
            name: name(b"same"),
            minimum: Version::new(0, 0, 1),
        };
        assert_eq!(
            Manifest::new(
                name(b"same"),
                Version::new(1, 0, 0),
                1,
                [0; 32],
                &[self_dep]
            ),
            Err(Error::SelfDependency)
        );
        assert_eq!(
            Manifest::new(
                name(b"app"),
                Version::new(1, 0, 0),
                1,
                [0; 32],
                &[self_dep, self_dep],
            ),
            Err(Error::DuplicateDependency)
        );
    }

    #[test]
    fn capability_declarations_are_canonical_and_reject_unknown_authority() {
        let caps = Capabilities::FILESYSTEM_READ
            .union(Capabilities::NETWORK)
            .union(Capabilities::DEVICE);
        let declaration = CapabilityDeclaration::new(name(b"browser"), caps);
        let wire = declaration.encode();
        assert_eq!(&wire[..8], b"VCAPv001");
        assert_eq!(CapabilityDeclaration::decode(&wire), Ok(declaration));
        assert!(declaration
            .capabilities
            .contains(Capabilities::FILESYSTEM_READ));
        assert!(!declaration
            .capabilities
            .contains(Capabilities::FILESYSTEM_WRITE));

        let mut unknown = wire;
        unknown[23] = 0x80;
        assert_eq!(
            CapabilityDeclaration::decode(&unknown),
            Err(Error::UnknownCapabilities)
        );

        let mut reserved = wire;
        reserved[9] = 1;
        assert_eq!(
            CapabilityDeclaration::decode(&reserved),
            Err(Error::Reserved)
        );
        let mut trailing = wire;
        trailing[63] = 1;
        assert_eq!(
            CapabilityDeclaration::decode(&trailing),
            Err(Error::Reserved)
        );
    }

    #[test]
    fn empty_capability_declaration_is_explicit_and_round_trips() {
        let declaration =
            CapabilityDeclaration::new(name(b"calculator"), Capabilities::empty());
        assert_eq!(
            CapabilityDeclaration::decode(&declaration.encode()),
            Ok(declaration)
        );
        assert_eq!(declaration.capabilities.bits(), 0);
    }

    #[test]
    fn version_minimum_comparison_is_numeric() {
        assert!(Version::new(1, 10, 0).satisfies_minimum(Version::new(1, 9, 9)));
        assert!(Version::new(2, 0, 0).satisfies_minimum(Version::new(1, 99, 99)));
        assert!(!Version::new(1, 2, 2).satisfies_minimum(Version::new(1, 2, 3)));
    }

    #[test]
    fn database_install_is_dependency_checked_and_transactional() {
        let core = manifest(b"core", Version::new(1, 4, 0), &[]);
        let requirement = Dependency {
            name: core.name,
            minimum: Version::new(1, 3, 0),
        };
        let app = manifest(b"app", Version::new(1, 0, 0), &[requirement]);
        let mut db = Database::new();

        assert_eq!(db.install(app), Err(Error::MissingDependency));
        assert_eq!(db.len(), 0);
        db.install(core).unwrap();
        db.install(app).unwrap();
        assert_eq!(db.len(), 2);
        assert_eq!(db.install(app), Err(Error::AlreadyInstalled));
    }

    #[test]
    fn insufficient_dependency_version_is_rejected_without_mutation() {
        let core = manifest(b"core", Version::new(1, 2, 9), &[]);
        let app = manifest(
            b"app",
            Version::new(1, 0, 0),
            &[Dependency {
                name: core.name,
                minimum: Version::new(1, 3, 0),
            }],
        );
        let mut db = Database::new();
        db.install(core).unwrap();
        assert_eq!(db.install(app), Err(Error::DependencyVersion));
        assert_eq!(db.len(), 1);
    }

    #[test]
    fn reverse_dependencies_block_removal_until_dependents_are_removed() {
        let core = manifest(b"core", Version::new(1, 0, 0), &[]);
        let app = manifest(
            b"app",
            Version::new(1, 0, 0),
            &[Dependency {
                name: core.name,
                minimum: Version::new(1, 0, 0),
            }],
        );
        let mut db = Database::new();
        db.install(core).unwrap();
        db.install(app).unwrap();
        assert_eq!(db.remove(core.name), Err(Error::RequiredByInstalled));
        assert_eq!(db.remove(app.name), Ok(app));
        assert_eq!(db.remove(core.name), Ok(core));
        assert!(db.is_empty());
    }
}
