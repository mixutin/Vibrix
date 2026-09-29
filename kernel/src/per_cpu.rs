//! Publish-once per-CPU identity storage for the pre-SMP kernel.
//!
//! Firmware CPU enumeration remains the source of truth for identities. This
//! module copies that validated inventory into kernel-owned immutable storage
//! exactly once, then binds the currently executing BSP by APIC ID. It does not
//! start APs, schedule across CPUs, provide mutable CPU-local state, or claim
//! synchronization beyond the single-BSP bootstrap.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::cpu_topology::{Availability, MAX_PROCESSORS, Topology};

const UNBOUND: usize = usize::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuSlot {
    pub firmware_uid: u32,
    pub apic_id: u32,
    pub availability: Availability,
    pub x2apic: bool,
}

const EMPTY_SLOT: CpuSlot = CpuSlot {
    firmware_uid: 0,
    apic_id: 0,
    availability: Availability::Disabled,
    x2apic: false,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Table {
    slots: [CpuSlot; MAX_PROCESSORS],
    length: usize,
}

impl Table {
    const EMPTY: Self = Self {
        slots: [EMPTY_SLOT; MAX_PROCESSORS],
        length: 0,
    };

    fn from_topology(topology: &Topology) -> Self {
        let mut table = Self::EMPTY;
        for processor in topology.processors() {
            table.slots[table.length] = CpuSlot {
                firmware_uid: processor.firmware_uid,
                apic_id: processor.apic_id,
                availability: processor.availability,
                x2apic: processor.x2apic,
            };
            table.length += 1;
        }
        table
    }

    fn slots(&self) -> &[CpuSlot] {
        &self.slots[..self.length]
    }

    fn enabled_index(&self, apic_id: u32) -> Option<usize> {
        self.slots()
            .iter()
            .position(|slot| slot.apic_id == apic_id && slot.availability == Availability::Enabled)
    }
}

struct PublishedTable(UnsafeCell<Table>);

// SAFETY: TABLE is written once by the sole BSP before publication. READY is
// Release-stored only after the complete Table value is written. No API mutates
// TABLE after publication, so Acquire readers observe immutable bytes.
unsafe impl Sync for PublishedTable {}

static TABLE: PublishedTable = PublishedTable(UnsafeCell::new(Table::EMPTY));
static READY: AtomicBool = AtomicBool::new(false);
static BSP_SLOT: AtomicUsize = AtomicUsize::new(UNBOUND);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    AlreadyInitialized,
    NotInitialized,
    BspNotEnabled,
    BspAlreadyBound,
}

/// Publish one kernel-owned copy of the validated firmware processor inventory.
///
/// # Safety
///
/// Call exactly once on the boot CPU before AP startup exists and before any
/// other CPU can execute Vibrix code. The topology must already have passed the
/// production MADT parser. No later code may mutate TABLE.
pub unsafe fn initialize_from_topology(topology: &Topology) -> Result<usize, Error> {
    if READY.load(Ordering::Acquire) {
        return Err(Error::AlreadyInitialized);
    }

    let table = Table::from_topology(topology);
    // SAFETY: caller guarantees sole-BSP one-time initialization and READY is
    // still false, so no safe reader can observe the backing object yet.
    unsafe { TABLE.0.get().write(table) };
    READY.store(true, Ordering::Release);
    Ok(table.length)
}

fn published() -> Option<&'static Table> {
    if !READY.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: Release/Acquire publication above makes the complete one-time
    // write visible, and the table is immutable forever after READY becomes true.
    Some(unsafe { &*TABLE.0.get() })
}

/// Bind the current bootstrap processor to its immutable firmware slot.
///
/// This does not make any AP runnable. It only records which already-published
/// enabled slot corresponds to the CPU that passed the LAPIC runtime probe.
pub fn bind_bsp(apic_id: u32) -> Result<CpuSlot, Error> {
    let table = published().ok_or(Error::NotInitialized)?;
    let index = table.enabled_index(apic_id).ok_or(Error::BspNotEnabled)?;
    BSP_SLOT
        .compare_exchange(UNBOUND, index, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| Error::BspAlreadyBound)?;
    Ok(table.slots[index])
}

pub fn slots() -> &'static [CpuSlot] {
    published().map_or(&[], Table::slots)
}

pub fn bsp() -> Option<CpuSlot> {
    let index = BSP_SLOT.load(Ordering::Acquire);
    if index == UNBOUND {
        return None;
    }
    published()?.slots().get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checksum(bytes: &mut [u8]) {
        bytes[9] = 0;
        bytes[9] = 0u8.wrapping_sub(bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
    }

    fn topology(entries: &[u8]) -> Topology {
        let mut bytes = std::vec![0; 44];
        bytes[..4].copy_from_slice(b"APIC");
        bytes[8] = 5;
        bytes.extend_from_slice(entries);
        let length = bytes.len() as u32;
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        checksum(&mut bytes);
        Topology::from_madt(&bytes).unwrap()
    }

    fn lapic(uid: u8, id: u8, flags: u8) -> [u8; 8] {
        [0, 8, uid, id, flags, 0, 0, 0]
    }

    #[test]
    fn immutable_table_preserves_firmware_identity_and_availability() {
        let mut entries = lapic(1, 4, 1).to_vec();
        entries.extend_from_slice(&lapic(2, 7, 2));
        entries.extend_from_slice(&lapic(3, 9, 0));
        let table = Table::from_topology(&topology(&entries));

        assert_eq!(table.slots().len(), 3);
        assert_eq!(
            table.slots()[0],
            CpuSlot {
                firmware_uid: 1,
                apic_id: 4,
                availability: Availability::Enabled,
                x2apic: false,
            }
        );
        assert_eq!(table.slots()[1].availability, Availability::OnlineCapable);
        assert_eq!(table.slots()[2].availability, Availability::Disabled);
    }

    #[test]
    fn bsp_binding_candidate_requires_an_enabled_apic_id() {
        let mut entries = lapic(1, 4, 1).to_vec();
        entries.extend_from_slice(&lapic(2, 7, 2));
        entries.extend_from_slice(&lapic(3, 9, 0));
        let table = Table::from_topology(&topology(&entries));

        assert_eq!(table.enabled_index(4), Some(0));
        assert_eq!(table.enabled_index(7), None);
        assert_eq!(table.enabled_index(9), None);
        assert_eq!(table.enabled_index(99), None);
    }

    #[test]
    fn table_capacity_matches_cpu_inventory_capacity() {
        assert_eq!(Table::EMPTY.slots.len(), MAX_PROCESSORS);
    }
}
