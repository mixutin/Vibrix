//! Bounded resolver configuration and optional local DNS A-record cache.

pub const MAX_SERVERS: usize = 3;
pub const CACHE_CAPACITY: usize = 8;
pub const NAME_BYTES: usize = 63;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4(pub [u8; 4]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NoServers,
    TooManyServers,
    InvalidServer,
    Name,
    ZeroTtl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolverConfig {
    servers: [Ipv4; MAX_SERVERS],
    len: u8,
    cache_enabled: bool,
}

impl ResolverConfig {
    pub fn new(servers: &[Ipv4], cache_enabled: bool) -> Result<Self, Error> {
        if servers.is_empty() {
            return Err(Error::NoServers);
        }
        if servers.len() > MAX_SERVERS {
            return Err(Error::TooManyServers);
        }
        if servers.iter().any(|server| server.0 == [0, 0, 0, 0]) {
            return Err(Error::InvalidServer);
        }
        let mut stored = [Ipv4([0, 0, 0, 0]); MAX_SERVERS];
        stored[..servers.len()].copy_from_slice(servers);
        Ok(Self {
            servers: stored,
            len: servers.len() as u8,
            cache_enabled,
        })
    }

    pub fn servers(&self) -> &[Ipv4] {
        &self.servers[..usize::from(self.len)]
    }

    pub const fn cache_enabled(&self) -> bool {
        self.cache_enabled
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Name {
    bytes: [u8; NAME_BYTES],
    len: u8,
}

impl Name {
    fn new(value: &[u8]) -> Result<Self, Error> {
        if value.is_empty()
            || value.len() > NAME_BYTES
            || !value.iter().copied().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-')
            })
        {
            return Err(Error::Name);
        }
        let mut bytes = [0; NAME_BYTES];
        bytes[..value.len()].copy_from_slice(value);
        Ok(Self {
            bytes,
            len: value.len() as u8,
        })
    }

    fn equals(&self, value: &[u8]) -> bool {
        &self.bytes[..usize::from(self.len)] == value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entry {
    name: Name,
    address: Ipv4,
    expires_at: u64,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cache {
    entries: [Option<Entry>; CACHE_CAPACITY],
    next_generation: u64,
}

impl Cache {
    pub const fn new() -> Self {
        Self {
            entries: [None; CACHE_CAPACITY],
            next_generation: 1,
        }
    }

    pub fn lookup(&mut self, name: &[u8], now: u64) -> Result<Option<Ipv4>, Error> {
        let _ = Name::new(name)?;
        for slot in &mut self.entries {
            if let Some(entry) = slot {
                if entry.expires_at <= now {
                    *slot = None;
                    continue;
                }
                if entry.name.equals(name) {
                    return Ok(Some(entry.address));
                }
            }
        }
        Ok(None)
    }

    pub fn insert(
        &mut self,
        name: &[u8],
        address: Ipv4,
        ttl_seconds: u32,
        now: u64,
    ) -> Result<(), Error> {
        let name = Name::new(name)?;
        if ttl_seconds == 0 {
            return Err(Error::ZeroTtl);
        }
        let expires_at = now.saturating_add(u64::from(ttl_seconds));
        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);

        if let Some(slot) = self
            .entries
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|entry| entry.name == name))
        {
            *slot = Some(Entry {
                name,
                address,
                expires_at,
                generation,
            });
            return Ok(());
        }

        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(Entry {
                name,
                address,
                expires_at,
                generation,
            });
            return Ok(());
        }

        let oldest = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, slot)| slot.map(|entry| entry.generation).unwrap_or(u64::MAX))
            .map(|(index, _)| index)
            .expect("non-empty fixed cache");
        self.entries[oldest] = Some(Entry {
            name,
            address,
            expires_at,
            generation,
        });
        Ok(())
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let config = ResolverConfig::new(&[Ipv4([192, 0, 2, 53])], true)?;
    if !config.cache_enabled() || config.servers().len() != 1 {
        return Err(Error::NoServers);
    }
    let mut cache = Cache::new();
    cache.insert(b"example.test", Ipv4([203, 0, 113, 7]), 30, 100)?;
    if cache.lookup(b"example.test", 110)? != Some(Ipv4([203, 0, 113, 7])) {
        return Err(Error::Name);
    }
    if cache.lookup(b"example.test", 130)?.is_some() {
        return Err(Error::ZeroTtl);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_configuration_is_bounded_and_explicit() {
        assert_eq!(ResolverConfig::new(&[], false), Err(Error::NoServers));
        assert_eq!(
            ResolverConfig::new(&[Ipv4([0, 0, 0, 0])], false),
            Err(Error::InvalidServer)
        );
        let config = ResolverConfig::new(
            &[Ipv4([1, 1, 1, 1]), Ipv4([9, 9, 9, 9])],
            true,
        )
        .unwrap();
        assert_eq!(config.servers().len(), 2);
        assert!(config.cache_enabled());
    }

    #[test]
    fn cache_honors_ttl_replacement_and_expiry() {
        let mut cache = Cache::new();
        cache
            .insert(b"a.test", Ipv4([192, 0, 2, 1]), 10, 100)
            .unwrap();
        assert_eq!(
            cache.lookup(b"a.test", 109).unwrap(),
            Some(Ipv4([192, 0, 2, 1]))
        );
        cache
            .insert(b"a.test", Ipv4([192, 0, 2, 2]), 20, 105)
            .unwrap();
        assert_eq!(
            cache.lookup(b"a.test", 110).unwrap(),
            Some(Ipv4([192, 0, 2, 2]))
        );
        assert_eq!(cache.lookup(b"a.test", 125).unwrap(), None);
    }

    #[test]
    fn zero_ttl_and_invalid_names_fail_closed() {
        let mut cache = Cache::new();
        assert_eq!(
            cache.insert(b"bad name", Ipv4([1, 2, 3, 4]), 1, 0),
            Err(Error::Name)
        );
        assert_eq!(
            cache.insert(b"ok.test", Ipv4([1, 2, 3, 4]), 0, 0),
            Err(Error::ZeroTtl)
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
