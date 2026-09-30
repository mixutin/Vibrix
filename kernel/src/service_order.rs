//! Fixed-capacity ordered boot/service dependency policy.
//!
//! This module validates a bounded service graph and produces a deterministic
//! topological start order. It is a policy primitive only; process launching,
//! supervision, restart and persistent enable/disable state belong to the
//! service manager milestone.

pub const MAX_SERVICES: usize = 16;
pub const MAX_DEPENDENCIES: usize = 8;
pub const SERVICE_NAME_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidName,
    TooManyDependencies,
    Capacity,
    DuplicateService,
    DuplicateDependency,
    SelfDependency,
    MissingDependency,
    Cycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceName {
    bytes: [u8; SERVICE_NAME_BYTES],
    len: u8,
}

impl ServiceName {
    pub const EMPTY: Self = Self {
        bytes: [0; SERVICE_NAME_BYTES],
        len: 0,
    };

    pub fn new(name: &[u8]) -> Result<Self, Error> {
        if name.is_empty()
            || name.len() > SERVICE_NAME_BYTES
            || !name.iter().copied().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
        {
            return Err(Error::InvalidName);
        }
        let mut result = Self::EMPTY;
        result.bytes[..name.len()].copy_from_slice(name);
        result.len = name.len() as u8;
        Ok(result)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Service {
    pub name: ServiceName,
    dependencies: [ServiceName; MAX_DEPENDENCIES],
    dependency_count: u8,
}

impl Service {
    pub fn new(name: ServiceName, dependencies: &[ServiceName]) -> Result<Self, Error> {
        if dependencies.len() > MAX_DEPENDENCIES {
            return Err(Error::TooManyDependencies);
        }
        let mut stored = [ServiceName::EMPTY; MAX_DEPENDENCIES];
        for (index, dependency) in dependencies.iter().copied().enumerate() {
            if dependency == name {
                return Err(Error::SelfDependency);
            }
            if dependencies[..index].contains(&dependency) {
                return Err(Error::DuplicateDependency);
            }
            stored[index] = dependency;
        }
        Ok(Self {
            name,
            dependencies: stored,
            dependency_count: dependencies.len() as u8,
        })
    }

    pub fn dependencies(&self) -> &[ServiceName] {
        &self.dependencies[..usize::from(self.dependency_count)]
    }
}

pub struct Graph {
    services: [Option<Service>; MAX_SERVICES],
    len: usize,
}

impl Graph {
    pub const fn new() -> Self {
        Self {
            services: [None; MAX_SERVICES],
            len: 0,
        }
    }

    pub fn add(&mut self, service: Service) -> Result<(), Error> {
        if self.len == MAX_SERVICES {
            return Err(Error::Capacity);
        }
        if self.services[..self.len]
            .iter()
            .flatten()
            .any(|existing| existing.name == service.name)
        {
            return Err(Error::DuplicateService);
        }
        self.services[self.len] = Some(service);
        self.len += 1;
        Ok(())
    }

    fn index_of(&self, name: ServiceName) -> Option<usize> {
        self.services[..self.len]
            .iter()
            .position(|slot| slot.is_some_and(|service| service.name == name))
    }

    pub fn start_order(&self) -> Result<([ServiceName; MAX_SERVICES], usize), Error> {
        let mut indegree = [0u8; MAX_SERVICES];
        for (index, service) in self.services[..self.len].iter().flatten().enumerate() {
            for dependency in service.dependencies() {
                if self.index_of(*dependency).is_none() {
                    return Err(Error::MissingDependency);
                }
                indegree[index] = indegree[index].saturating_add(1);
            }
        }

        let mut emitted = [false; MAX_SERVICES];
        let mut order = [ServiceName::EMPTY; MAX_SERVICES];
        let mut count = 0usize;

        while count < self.len {
            let next = (0..self.len).find(|&index| !emitted[index] && indegree[index] == 0);
            let Some(index) = next else {
                return Err(Error::Cycle);
            };
            let service = self.services[index].expect("occupied service slot");
            emitted[index] = true;
            order[count] = service.name;
            count += 1;

            for (dependent_index, dependent) in
                self.services[..self.len].iter().flatten().enumerate()
            {
                if !emitted[dependent_index] && dependent.dependencies().contains(&service.name) {
                    indegree[dependent_index] -= 1;
                }
            }
        }
        Ok((order, count))
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let storage = ServiceName::new(b"storage")?;
    let network = ServiceName::new(b"network")?;
    let ssh = ServiceName::new(b"ssh")?;
    let mut graph = Graph::new();
    graph.add(Service::new(storage, &[])?)?;
    graph.add(Service::new(network, &[storage])?)?;
    graph.add(Service::new(ssh, &[network])?)?;
    let (order, count) = graph.start_order()?;
    if count != 3 || order[..count] != [storage, network, ssh] {
        return Err(Error::Cycle);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(name: &[u8]) -> ServiceName {
        ServiceName::new(name).unwrap()
    }

    #[test]
    fn dependency_order_is_deterministic() {
        let a = n(b"a");
        let b = n(b"b");
        let c = n(b"c");
        let d = n(b"d");
        let mut graph = Graph::new();
        graph.add(Service::new(a, &[]).unwrap()).unwrap();
        graph.add(Service::new(b, &[a]).unwrap()).unwrap();
        graph.add(Service::new(c, &[a]).unwrap()).unwrap();
        graph.add(Service::new(d, &[b, c]).unwrap()).unwrap();
        let (order, count) = graph.start_order().unwrap();
        assert_eq!(&order[..count], &[a, b, c, d]);
    }

    #[test]
    fn missing_and_cyclic_dependencies_fail_closed() {
        let a = n(b"a");
        let b = n(b"b");
        let missing = n(b"missing");
        let mut graph = Graph::new();
        graph.add(Service::new(a, &[missing]).unwrap()).unwrap();
        assert_eq!(graph.start_order(), Err(Error::MissingDependency));

        let mut graph = Graph::new();
        graph.add(Service::new(a, &[b]).unwrap()).unwrap();
        graph.add(Service::new(b, &[a]).unwrap()).unwrap();
        assert_eq!(graph.start_order(), Err(Error::Cycle));
    }

    #[test]
    fn duplicate_and_self_edges_are_rejected() {
        let a = n(b"a");
        let b = n(b"b");
        assert_eq!(Service::new(a, &[a]), Err(Error::SelfDependency));
        assert_eq!(Service::new(a, &[b, b]), Err(Error::DuplicateDependency));
        let mut graph = Graph::new();
        graph.add(Service::new(a, &[]).unwrap()).unwrap();
        assert_eq!(
            graph.add(Service::new(a, &[]).unwrap()),
            Err(Error::DuplicateService)
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
