//! Pure x86-64 IRQ routing policy shared by APIC setup and host tests.
//!
//! Hardware programming stays in the APIC module. This module makes vector,
//! GSI and I/O-APIC redirection validation testable without privileged I/O.

pub const TIMER_VECTOR: u8 = 0x40;
pub const KEYBOARD_VECTOR: u8 = 0x41;
pub const LEGACY_TIMER_IRQ: u32 = 0;
pub const LEGACY_KEYBOARD_IRQ: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteError {
    VectorReserved,
    GsiOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub gsi: u32,
    pub vector: u8,
    pub destination_apic_id: u8,
    pub active_low: bool,
    pub level_triggered: bool,
}

impl Route {
    pub fn new(gsi: u32, vector: u8, destination_apic_id: u8) -> Result<Self, RouteError> {
        if vector < 32 {
            return Err(RouteError::VectorReserved);
        }
        Ok(Self {
            gsi,
            vector,
            destination_apic_id,
            active_low: false,
            level_triggered: false,
        })
    }

    pub fn redirection_entry(self, max_redirection_entry: u8, ioapic_gsi_base: u32) -> Result<(u8, u64), RouteError> {
        let index = self.gsi.checked_sub(ioapic_gsi_base).ok_or(RouteError::GsiOutOfRange)?;
        if index > u32::from(max_redirection_entry) {
            return Err(RouteError::GsiOutOfRange);
        }
        let mut entry = u64::from(self.vector);
        if self.active_low { entry |= 1 << 13; }
        if self.level_triggered { entry |= 1 << 15; }
        entry |= u64::from(self.destination_apic_id) << 56;
        Ok((index as u8, entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_and_keyboard_vectors_do_not_overlap_exceptions() {
        assert!(TIMER_VECTOR >= 32);
        assert!(KEYBOARD_VECTOR >= 32);
        assert_ne!(TIMER_VECTOR, KEYBOARD_VECTOR);
    }

    #[test]
    fn builds_edge_high_fixed_delivery_entry() {
        let route = Route::new(1, KEYBOARD_VECTOR, 7).unwrap();
        let (index, entry) = route.redirection_entry(23, 0).unwrap();
        assert_eq!(index, 1);
        assert_eq!(entry & 0xff, u64::from(KEYBOARD_VECTOR));
        assert_eq!(entry >> 56, 7);
        assert_eq!(entry & ((1 << 13) | (1 << 15)), 0);
    }

    #[test]
    fn supports_madt_override_polarity_and_trigger() {
        let mut route = Route::new(9, KEYBOARD_VECTOR, 2).unwrap();
        route.active_low = true;
        route.level_triggered = true;
        let (_, entry) = route.redirection_entry(23, 0).unwrap();
        assert_ne!(entry & (1 << 13), 0);
        assert_ne!(entry & (1 << 15), 0);
    }

    #[test]
    fn rejects_reserved_vector_and_gsi_outside_ioapic() {
        assert_eq!(Route::new(1, 14, 0), Err(RouteError::VectorReserved));
        let route = Route::new(24, TIMER_VECTOR, 0).unwrap();
        assert_eq!(route.redirection_entry(23, 0), Err(RouteError::GsiOutOfRange));
        assert_eq!(route.redirection_entry(23, 25), Err(RouteError::GsiOutOfRange));
    }
}
