//! Pure x86-64 IRQ routing policy shared by APIC setup and host tests.
//!
//! Hardware programming stays in the APIC/PIT modules. This module validates
//! vectors, GSI coverage and I/O-APIC redirection words without privileged I/O.

use core::sync::atomic::{AtomicU64, Ordering};

pub const TIMER_VECTOR: u8 = 0x40;
pub const SPURIOUS_VECTOR: u8 = 0xff;
pub const LEGACY_TIMER_IRQ: u8 = 0;

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

pub(crate) fn record_timer_tick() {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
}

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
        if vector < 32 || vector == SPURIOUS_VECTOR {
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

    pub fn with_signal(mut self, active_low: bool, level_triggered: bool) -> Self {
        self.active_low = active_low;
        self.level_triggered = level_triggered;
        self
    }

    pub fn redirection_entry(
        self,
        max_redirection_entry: u8,
        ioapic_gsi_base: u32,
    ) -> Result<(u8, u64), RouteError> {
        let index = self
            .gsi
            .checked_sub(ioapic_gsi_base)
            .ok_or(RouteError::GsiOutOfRange)?;
        if index > u32::from(max_redirection_entry) {
            return Err(RouteError::GsiOutOfRange);
        }
        // Fixed physical delivery, unmasked. Polarity/trigger bits are set
        // only from validated MADT/ISA semantics.
        let mut entry = u64::from(self.vector);
        if self.active_low {
            entry |= 1 << 13;
        }
        if self.level_triggered {
            entry |= 1 << 15;
        }
        entry |= u64::from(self.destination_apic_id) << 56;
        Ok((index as u8, entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_tick_counter_advances() {
        let before = timer_ticks();
        record_timer_tick();
        assert_eq!(timer_ticks(), before + 1);
    }

    #[test]
    fn timer_vector_is_outside_exceptions_and_spurious_slot() {
        assert!(TIMER_VECTOR >= 32);
        assert_ne!(TIMER_VECTOR, SPURIOUS_VECTOR);
        assert_eq!(Route::new(0, 14, 0), Err(RouteError::VectorReserved));
        assert_eq!(
            Route::new(0, SPURIOUS_VECTOR, 0),
            Err(RouteError::VectorReserved)
        );
    }

    #[test]
    fn builds_edge_high_fixed_delivery_entry() {
        let route = Route::new(2, TIMER_VECTOR, 7).unwrap();
        let (index, entry) = route.redirection_entry(23, 0).unwrap();
        assert_eq!(index, 2);
        assert_eq!(entry & 0xff, u64::from(TIMER_VECTOR));
        assert_eq!(entry >> 56, 7);
        assert_eq!(entry & ((1 << 13) | (1 << 15) | (1 << 16)), 0);
    }

    #[test]
    fn supports_validated_override_polarity_and_trigger() {
        let route = Route::new(9, TIMER_VECTOR, 2)
            .unwrap()
            .with_signal(true, true);
        let (_, entry) = route.redirection_entry(23, 0).unwrap();
        assert_ne!(entry & (1 << 13), 0);
        assert_ne!(entry & (1 << 15), 0);
    }

    #[test]
    fn rejects_gsi_outside_selected_ioapic() {
        let route = Route::new(24, TIMER_VECTOR, 0).unwrap();
        assert_eq!(
            route.redirection_entry(23, 0),
            Err(RouteError::GsiOutOfRange)
        );
        assert_eq!(
            route.redirection_entry(23, 25),
            Err(RouteError::GsiOutOfRange)
        );
    }
}
