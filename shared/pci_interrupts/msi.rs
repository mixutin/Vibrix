//! Single-message xAPIC MSI programming with fail-closed ordering.

use super::pci_interrupts::{Capabilities, Msi, discover};

/// A serialized configuration transport for exactly one PCI function.
/// Implementations must use the requested access width, not a DWORD
/// read-modify-write for WORD writes (adjacent PCI status bits can be W1C).
/// Failed writes may have reached hardware; failure is not proof of rollback.
pub trait ConfigSpace {
    fn read32(&mut self, offset: u8) -> Option<u32>;
    fn write16(&mut self, offset: u8, value: u16) -> bool;
    fn write32(&mut self, offset: u8, value: u32) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupError {
    ReservedVector,
    BroadcastDestination,
    MissingCapability,
    StaleCapability,
    AlreadyEnabled,
    UnsupportedExtension,
    ReadFailed,
    WriteFailed,
    ReadbackMismatch,
}

/// Fixed, physical, edge-triggered xAPIC delivery. No interrupt remapping,
/// logical destinations, x2APIC IDs or automatic vector allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message {
    address: u32,
    data: u16,
}

impl Message {
    pub fn new(destination: u8, vector: u8) -> Result<Self, SetupError> {
        if !(0x50..0xf0).contains(&vector) {
            return Err(SetupError::ReservedVector);
        }
        if destination == 0xff {
            return Err(SetupError::BroadcastDestination);
        }
        Ok(Self {
            address: 0xfee0_0000 | (u32::from(destination) << 12),
            data: u16::from(vector),
        })
    }

    pub const fn address(self) -> u32 {
        self.address
    }

    pub const fn data(self) -> u16 {
        self.data
    }
}

/// Program one MSI, enabling it only after address/data readback succeeds.
/// MSI and MSI-X must both initially be disabled. No command register, BAR,
/// bus-master bit, interrupt-pending register or DMA register is written.
///
/// # Safety
/// The caller exclusively owns this stable PCI function and serializes config
/// accesses. The device's interrupt sources are quiesced and legacy INTx is
/// disabled. The destination LAPIC is in xAPIC mode with interrupt remapping
/// disabled. The caller owns the vector, has installed a permanent handler,
/// and keeps all handler/device state live until a verified disable and drain.
/// On ANY error retain those resources: best-effort disable cannot establish
/// quiescence if the transport/device failed. This routine is not a hotplug,
/// general driver-binding, DMA-isolation or SMP synchronization mechanism.
pub unsafe fn enable_single(
    io: &mut impl ConfigSpace,
    expected: Capabilities,
    message: Message,
) -> Result<(), SetupError> {
    let current = discover(&mut |offset| io.read32(offset))
        .map_err(|_| SetupError::StaleCapability)?;
    if current != expected {
        return Err(SetupError::StaleCapability);
    }
    let msi = current.msi.ok_or(SetupError::MissingCapability)?;
    if msi.control() & 1 != 0 || current.msix.is_some_and(|cap| cap.control() & 0x8000 != 0) {
        return Err(SetupError::AlreadyEnabled);
    }
    if msi.control() & !0x1ff != 0 {
        return Err(SetupError::UnsupportedExtension);
    }
    let disabled = msi.control() & !0x71;
    let data_offset = msi.offset() + if msi.address_64_bit() { 12 } else { 8 };
    let mask_offset = data_offset + 4;
    let supported = u32::MAX >> (32 - u32::from(msi.message_capacity()));
    let original_mask = if msi.per_vector_masking() {
        io.read32(mask_offset).ok_or(SetupError::ReadFailed)?
    } else {
        0
    };
    let result = (|| {
        write16_checked(io, msi.offset() + 2, disabled)?;
        if msi.per_vector_masking() {
            write32_checked(io, mask_offset, original_mask | supported)?;
        }
        write32_checked(io, msi.offset() + 4, message.address())?;
        if msi.address_64_bit() {
            write32_checked(io, msi.offset() + 8, 0)?;
        }
        write16_checked(io, data_offset, message.data())?;
        if msi.per_vector_masking() {
            write32_checked(io, mask_offset, (original_mask | supported) & !1)?;
        }
        write16_checked(io, msi.offset() + 2, disabled | 1)
    })();
    if result.is_err() {
        // Retain resources even if these attempts succeed: an interrupt may
        // already be in flight. Never reactivate an old route on failure.
        let _ = io.write16(msi.offset() + 2, disabled);
        if msi.per_vector_masking() {
            let _ = io.write32(mask_offset, original_mask | supported);
        }
        let _ = io.read32(msi.offset());
    }
    result
}

/// Disable MSI and verify the enable bit is clear. This does not drain an
/// already delivered/in-flight interrupt or release vector/handler ownership.
///
/// # Safety
/// Same exclusive function/config transport as `enable_single`. The handler
/// and device mappings remain live, and the source is quiesced by its owner.
pub unsafe fn disable_single(io: &mut impl ConfigSpace, msi: Msi) -> Result<(), SetupError> {
    let header = io.read32(msi.offset()).ok_or(SetupError::ReadFailed)?;
    if header as u8 != 5 || ((header >> 16) as u16 & !0x71) != (msi.control() & !0x71) {
        return Err(SetupError::StaleCapability);
    }
    write16_checked(io, msi.offset() + 2, (header >> 16) as u16 & !1)
}

pub(crate) fn write16_checked(
    io: &mut impl ConfigSpace,
    offset: u8,
    value: u16,
) -> Result<(), SetupError> {
    if !io.write16(offset, value) {
        return Err(SetupError::WriteFailed);
    }
    let word = io.read32(offset & !3).ok_or(SetupError::ReadFailed)?;
    if (word >> (u32::from(offset & 2) * 8)) as u16 != value {
        return Err(SetupError::ReadbackMismatch);
    }
    Ok(())
}

pub(crate) fn write32_checked(
    io: &mut impl ConfigSpace,
    offset: u8,
    value: u32,
) -> Result<(), SetupError> {
    if !io.write32(offset, value) {
        return Err(SetupError::WriteFailed);
    }
    if io.read32(offset).ok_or(SetupError::ReadFailed)? != value {
        return Err(SetupError::ReadbackMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        words: [u32; 64],
        writes: [(u8, u8, u32); 24],
        count: usize,
        fail_at: Option<usize>,
        discard_at: Option<usize>,
    }

    impl Fake {
        fn new(control: u16) -> Self {
            let mut words = [0u32; 64];
            words[0] = 0x11e8_1234;
            words[1] = 0xab10_0400;
            words[13] = 0x40;
            words[16] = (u32::from(control) << 16) | 5;
            Self {
                words,
                writes: [(0, 0, 0); 24],
                count: 0,
                fail_at: None,
                discard_at: None,
            }
        }

        fn capabilities(&mut self) -> Capabilities {
            discover(&mut |offset| self.read32(offset)).unwrap()
        }

        fn put(&mut self, width: u8, offset: u8, value: u32) -> bool {
            self.writes[self.count] = (width, offset, value);
            self.count += 1;
            if self.fail_at == Some(self.count) {
                return false;
            }
            if self.discard_at == Some(self.count) {
                return true;
            }
            let index = usize::from(offset / 4);
            if width == 32 {
                assert_eq!(offset & 3, 0);
                self.words[index] = value;
            } else {
                assert_eq!(offset & 1, 0);
                let shift = u32::from(offset & 2) * 8;
                self.words[index] = (self.words[index] & !(0xffff << shift)) | (value << shift);
            }
            true
        }
    }

    impl ConfigSpace for Fake {
        fn read32(&mut self, offset: u8) -> Option<u32> {
            assert_eq!(offset & 3, 0);
            Some(self.words[usize::from(offset / 4)])
        }

        fn write16(&mut self, offset: u8, value: u16) -> bool {
            self.put(16, offset, u32::from(value))
        }

        fn write32(&mut self, offset: u8, value: u32) -> bool {
            self.put(32, offset, value)
        }
    }

    #[test]
    fn xapic_message_excludes_core_vectors_and_broadcast() {
        for vector in 0..=255u8 {
            assert_eq!(Message::new(0, vector).is_ok(), (0x50..0xf0).contains(&vector));
        }
        assert_eq!(Message::new(255, 0x50), Err(SetupError::BroadcastDestination));
        let message = Message::new(7, 0x51).unwrap();
        assert_eq!(message.address(), 0xfee0_7000);
        assert_eq!(message.data(), 0x51);
    }

    #[test]
    fn all_four_layouts_enable_last_and_preserve_unrelated_registers() {
        for flags in [0, 0x80, 0x100, 0x180] {
            let mut io = Fake::new(flags | 0xa);
            let expected = io.capabilities();
            let before = io.words[1];
            let message = Message::new(3, 0x50).unwrap();
            // SAFETY: an in-memory fake has no hardware or interrupt effects.
            unsafe { enable_single(&mut io, expected, message) }.unwrap();
            assert_eq!(io.words[1], before);
            assert_eq!(io.words[16] as u16, 5);
            assert_eq!(io.words[16] >> 16, u32::from(flags | 0xa | 1));
            assert_eq!(io.writes[0], (16, 0x42, u32::from(flags | 0xa)));
            assert_eq!(io.writes[io.count - 1], (16, 0x42, u32::from(flags | 0xa | 1)));
            assert!(io.writes[..io.count].iter().all(|entry| entry.1 >= 0x42));
            if flags & 0x100 != 0 {
                let mask = if flags & 0x80 != 0 { 0x50 } else { 0x4c };
                assert_eq!(io.words[mask / 4], 0xffff_fffe);
            }
            // SAFETY: same inert configuration fake, no live resources.
            unsafe { disable_single(&mut io, expected.msi.unwrap()) }.unwrap();
            assert_eq!(io.words[16] & (1 << 16), 0);
        }
    }

    #[test]
    fn stale_active_and_extended_capabilities_do_not_write() {
        for control in [1, 0x200] {
            let mut io = Fake::new(control);
            let expected = io.capabilities();
            let message = Message::new(0, 0x50).unwrap();
            // SAFETY: inert fake.
            assert!(unsafe { enable_single(&mut io, expected, message) }.is_err());
            assert_eq!(io.count, 0);
        }
        let mut io = Fake::new(0);
        let expected = io.capabilities();
        io.words[16] |= 0x80 << 16;
        // SAFETY: inert fake.
        assert_eq!(
            unsafe { enable_single(&mut io, expected, Message::new(0, 0x50).unwrap()) },
            Err(SetupError::StaleCapability)
        );
        assert_eq!(io.count, 0);
    }

    #[test]
    fn active_msix_is_not_silently_taken_over() {
        let mut io = Fake::new(0);
        io.words[16] |= 0x50 << 8;
        io.words[20] = (0x8000 << 16) | 0x11;
        let expected = io.capabilities();
        // SAFETY: inert fake.
        assert_eq!(
            unsafe { enable_single(&mut io, expected, Message::new(0, 0x50).unwrap()) },
            Err(SetupError::AlreadyEnabled)
        );
        assert_eq!(io.count, 0);
    }

    #[test]
    fn every_write_failure_leaves_enable_clear_in_the_fake() {
        for failure in 1..=7 {
            let mut io = Fake::new(0x180);
            let expected = io.capabilities();
            io.fail_at = Some(failure);
            // SAFETY: inert fake; failed write is deliberately injected.
            assert_eq!(
                unsafe { enable_single(&mut io, expected, Message::new(0, 0x50).unwrap()) },
                Err(SetupError::WriteFailed)
            );
            assert_eq!(io.words[16] & (1 << 16), 0);
            assert_ne!(io.words[20] & 1, 0);
        }
    }

    #[test]
    fn ignored_address_and_enable_writes_are_detected() {
        for discarded in [2, 4] {
            let mut io = Fake::new(0);
            let expected = io.capabilities();
            io.discard_at = Some(discarded);
            // SAFETY: inert fake; hardware readback failure is simulated.
            assert_eq!(
                unsafe { enable_single(&mut io, expected, Message::new(0, 0x50).unwrap()) },
                Err(SetupError::ReadbackMismatch)
            );
            assert_eq!(io.words[16] & (1 << 16), 0);
        }
    }
}
