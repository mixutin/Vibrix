//! Fixed-capacity IPv4 routing and deterministic route-selection policy.
//!
//! This is a control-plane policy primitive only. It does not transmit packets,
//! program a NIC, perform neighbor discovery, or expose a userspace socket API.

pub const MAX_ROUTES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub const UNSPECIFIED: Self = Self([0, 0, 0, 0]);

    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    const fn as_u32(self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Route {
    pub network: Ipv4Address,
    pub prefix_len: u8,
    pub gateway: Option<Ipv4Address>,
    pub interface: u8,
    pub metric: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidPrefix,
    NonCanonicalNetwork,
    UnspecifiedGateway,
    Full,
}

impl Route {
    pub fn new(
        network: Ipv4Address,
        prefix_len: u8,
        gateway: Option<Ipv4Address>,
        interface: u8,
        metric: u16,
    ) -> Result<Self, Error> {
        if prefix_len > 32 {
            return Err(Error::InvalidPrefix);
        }
        let mask = prefix_mask(prefix_len);
        if network.as_u32() & !mask != 0 {
            return Err(Error::NonCanonicalNetwork);
        }
        if gateway == Some(Ipv4Address::UNSPECIFIED) {
            return Err(Error::UnspecifiedGateway);
        }
        Ok(Self {
            network,
            prefix_len,
            gateway,
            interface,
            metric,
        })
    }

    fn matches(self, address: Ipv4Address) -> bool {
        let mask = prefix_mask(self.prefix_len);
        address.as_u32() & mask == self.network.as_u32()
    }
}

const fn prefix_mask(prefix_len: u8) -> u32 {
    if prefix_len == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_len)
    }
}

pub struct Table {
    routes: [Option<Route>; MAX_ROUTES],
    len: usize,
}

impl Table {
    pub const fn new() -> Self {
        Self {
            routes: [None; MAX_ROUTES],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Insert a route or replace an existing route with the same prefix and
    /// interface. Replacement is transactional and does not consume capacity.
    pub fn insert(&mut self, route: Route) -> Result<(), Error> {
        if let Some(slot) = self.routes[..self.len].iter_mut().find(|slot| {
            slot.is_some_and(|existing| {
                existing.network == route.network
                    && existing.prefix_len == route.prefix_len
                    && existing.interface == route.interface
            })
        }) {
            *slot = Some(route);
            return Ok(());
        }
        if self.len == MAX_ROUTES {
            return Err(Error::Full);
        }
        self.routes[self.len] = Some(route);
        self.len += 1;
        Ok(())
    }

    /// Longest-prefix match, then lowest metric, then earliest table entry.
    pub fn select(&self, destination: Ipv4Address) -> Option<Route> {
        let mut best: Option<Route> = None;
        for route in self.routes[..self.len].iter().flatten().copied() {
            if !route.matches(destination) {
                continue;
            }
            best = match best {
                None => Some(route),
                Some(current)
                    if route.prefix_len > current.prefix_len
                        || (route.prefix_len == current.prefix_len
                            && route.metric < current.metric) =>
                {
                    Some(route)
                }
                Some(current) => Some(current),
            };
        }
        best
    }

    pub fn remove(&mut self, network: Ipv4Address, prefix_len: u8, interface: u8) -> Option<Route> {
        let index = self.routes[..self.len].iter().position(|slot| {
            slot.is_some_and(|route| {
                route.network == network
                    && route.prefix_len == prefix_len
                    && route.interface == interface
            })
        })?;
        let removed = self.routes[index];
        for next in index + 1..self.len {
            self.routes[next - 1] = self.routes[next];
        }
        self.len -= 1;
        self.routes[self.len] = None;
        removed
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

pub fn self_test() -> Result<(), Error> {
    let mut table = Table::new();
    table.insert(Route::new(
        Ipv4Address::UNSPECIFIED,
        0,
        Some(Ipv4Address::new(192, 0, 2, 1)),
        1,
        100,
    )?)?;
    table.insert(Route::new(Ipv4Address::new(10, 0, 0, 0), 8, None, 2, 20)?)?;
    table.insert(Route::new(
        Ipv4Address::new(10, 20, 0, 0),
        16,
        Some(Ipv4Address::new(10, 0, 0, 1)),
        2,
        30,
    )?)?;

    let selected = table
        .select(Ipv4Address::new(10, 20, 30, 40))
        .ok_or(Error::InvalidPrefix)?;
    if selected.prefix_len != 16 || selected.interface != 2 {
        return Err(Error::InvalidPrefix);
    }
    let default = table
        .select(Ipv4Address::new(203, 0, 113, 9))
        .ok_or(Error::InvalidPrefix)?;
    if default.prefix_len != 0 || default.gateway != Some(Ipv4Address::new(192, 0, 2, 1)) {
        return Err(Error::InvalidPrefix);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(
        network: Ipv4Address,
        prefix_len: u8,
        gateway: Option<Ipv4Address>,
        interface: u8,
        metric: u16,
    ) -> Route {
        Route::new(network, prefix_len, gateway, interface, metric).unwrap()
    }

    #[test]
    fn rejects_noncanonical_prefixes_and_invalid_gateways() {
        assert_eq!(
            Route::new(Ipv4Address::new(10, 0, 0, 1), 24, None, 1, 0),
            Err(Error::NonCanonicalNetwork)
        );
        assert_eq!(
            Route::new(Ipv4Address::UNSPECIFIED, 33, None, 1, 0),
            Err(Error::InvalidPrefix)
        );
        assert_eq!(
            Route::new(
                Ipv4Address::UNSPECIFIED,
                0,
                Some(Ipv4Address::UNSPECIFIED),
                1,
                0,
            ),
            Err(Error::UnspecifiedGateway)
        );
    }

    #[test]
    fn longest_prefix_beats_metric_and_default_route() {
        let mut table = Table::new();
        table
            .insert(route(
                Ipv4Address::UNSPECIFIED,
                0,
                Some(Ipv4Address::new(192, 0, 2, 1)),
                1,
                1,
            ))
            .unwrap();
        table
            .insert(route(Ipv4Address::new(10, 0, 0, 0), 8, None, 2, 100))
            .unwrap();
        table
            .insert(route(
                Ipv4Address::new(10, 20, 0, 0),
                16,
                Some(Ipv4Address::new(10, 0, 0, 1)),
                3,
                500,
            ))
            .unwrap();

        let selected = table.select(Ipv4Address::new(10, 20, 1, 2)).unwrap();
        assert_eq!(selected.prefix_len, 16);
        assert_eq!(selected.interface, 3);
    }

    #[test]
    fn lower_metric_breaks_equal_prefix_ties() {
        let mut table = Table::new();
        table
            .insert(route(Ipv4Address::new(198, 51, 100, 0), 24, None, 1, 50))
            .unwrap();
        table
            .insert(route(Ipv4Address::new(198, 51, 100, 0), 24, None, 2, 10))
            .unwrap();
        assert_eq!(
            table
                .select(Ipv4Address::new(198, 51, 100, 99))
                .unwrap()
                .interface,
            2
        );
    }

    #[test]
    fn exact_prefix_and_interface_insert_replaces_without_growth() {
        let mut table = Table::new();
        table
            .insert(route(Ipv4Address::new(203, 0, 113, 0), 24, None, 4, 50))
            .unwrap();
        table
            .insert(route(
                Ipv4Address::new(203, 0, 113, 0),
                24,
                Some(Ipv4Address::new(192, 0, 2, 9)),
                4,
                5,
            ))
            .unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(
            table
                .select(Ipv4Address::new(203, 0, 113, 2))
                .unwrap()
                .metric,
            5
        );
    }

    #[test]
    fn capacity_failure_preserves_existing_routes() {
        let mut table = Table::new();
        for host in 0..MAX_ROUTES {
            table
                .insert(route(
                    Ipv4Address::new(10, host as u8, 0, 0),
                    16,
                    None,
                    host as u8,
                    host as u16,
                ))
                .unwrap();
        }
        assert_eq!(
            table.insert(route(Ipv4Address::new(172, 16, 0, 0), 16, None, 99, 0)),
            Err(Error::Full)
        );
        assert_eq!(table.len(), MAX_ROUTES);
        assert!(table.select(Ipv4Address::new(10, 3, 1, 1)).is_some());
    }

    #[test]
    fn removal_compacts_table_without_changing_selection() {
        let mut table = Table::new();
        let a = route(Ipv4Address::new(10, 0, 0, 0), 8, None, 1, 0);
        let b = route(Ipv4Address::new(192, 168, 0, 0), 16, None, 2, 0);
        table.insert(a).unwrap();
        table.insert(b).unwrap();
        assert_eq!(table.remove(a.network, a.prefix_len, a.interface), Some(a));
        assert_eq!(table.len(), 1);
        assert_eq!(table.select(Ipv4Address::new(192, 168, 1, 2)), Some(b));
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
