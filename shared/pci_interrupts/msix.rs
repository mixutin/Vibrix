//! Checked MSI-X table geometry and masked single-entry programming.

use super::pci_interrupts::{Capabilities, Msix, discover};
use super::pci_msi::{ConfigSpace, Message, SetupError, write16_checked};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MsixError {
    InvalidBar,
    MissingBar,
    OutOfBounds,
    Overlap,
    StaleCapability,
    AlreadyEnabled,
    ReadFailed,
    WriteFailed,
    ReadbackMismatch,
    Config(SetupError),
}

/// Metadata from a separately validated memory BAR. Constructing this value
/// does not size a BAR, authorize a mapping, or establish device ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryBar {
    base: u64,
    bytes: u64,
}

impl MemoryBar {
    pub fn new(base: u64, bytes: u64) -> Result<Self, MsixError> {
        if base == 0 || bytes < 16 || !bytes.is_power_of_two() || !base.is_multiple_of(bytes) {
            return Err(MsixError::InvalidBar);
        }
        base.checked_add(bytes).ok_or(MsixError::InvalidBar)?;
        Ok(Self { base, bytes })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layout {
    capability: Msix,
    table_physical: u64,
    pending_physical: u64,
    table_bytes: u32,
    pending_bytes: u32,
}

impl Layout {
    /// `bars` contains only low slots of validated MEMORY BARs. IO BARs and
    /// upper halves of 64-bit BARs must be None. Extents are supplied by the
    /// driver/resource owner, never inferred from the assigned base alone.
    pub fn new(cap: Msix, bars: &[Option<MemoryBar>; 6]) -> Result<Self, MsixError> {
        let table = bars[usize::from(cap.table().bir())].ok_or(MsixError::MissingBar)?;
        let pending = bars[usize::from(cap.pending().bir())].ok_or(MsixError::MissingBar)?;
        let table_bytes = u32::from(cap.vectors()) * 16;
        let pending_bytes = u32::from(cap.vectors()).div_ceil(64) * 8;
        let table_offset = u64::from(cap.table().offset());
        let pending_offset = u64::from(cap.pending().offset());
        if table_offset + u64::from(table_bytes) > table.bytes
            || pending_offset + u64::from(pending_bytes) > pending.bytes
        {
            return Err(MsixError::OutOfBounds);
        }
        let table_physical = table.base + table_offset;
        let pending_physical = pending.base + pending_offset;
        if table_physical < pending_physical + u64::from(pending_bytes)
            && pending_physical < table_physical + u64::from(table_bytes)
        {
            return Err(MsixError::Overlap);
        }
        Ok(Self {
            capability: cap,
            table_physical,
            pending_physical,
            table_bytes,
            pending_bytes,
        })
    }

    pub const fn table_physical(self) -> u64 {
        self.table_physical
    }

    pub const fn pending_physical(self) -> u64 {
        self.pending_physical
    }

    pub const fn table_bytes(self) -> u32 {
        self.table_bytes
    }

    pub const fn pending_bytes(self) -> u32 {
        self.pending_bytes
    }
}

/// Offsets are relative to the validated table, not its BAR. The transport
/// owns the complete mapped table and performs ordered aligned DWORD MMIO.
/// Reads used for verification must flush prior posted writes to this device.
pub trait TableAccess {
    fn read32(&mut self, offset: u32) -> Option<u32>;
    fn write32(&mut self, offset: u32, value: u32) -> bool;
}

/// Mask the function and every entry before publishing one message. All
/// unused entries remain masked; the function mask is cleared LAST. The
/// pending-bit array is never modified.
///
/// # Safety
/// The caller owns this function, vector, stable configuration and the exact
/// validated UC table mapping described by `layout`. Source interrupts and
/// legacy INTx are quiesced. A permanent handler and physical xAPIC target
/// are ready with interrupt remapping disabled. All resources remain live on
/// errors and until device-specific quiescence/drain after verified disable.
/// No other CPU, IRQ handler, driver or hotplug operation may change this
/// function, configuration, BARs or table during setup.
pub unsafe fn enable_entry(
    io: &mut impl ConfigSpace,
    table: &mut impl TableAccess,
    expected: Capabilities,
    layout: Layout,
    index: u16,
    message: Message,
) -> Result<(), MsixError> {
    let cap = expected.msix.ok_or(MsixError::StaleCapability)?;
    if cap != layout.capability || index >= cap.vectors() {
        return Err(MsixError::OutOfBounds);
    }
    let current =
        discover(&mut |offset| io.read32(offset)).map_err(|_| MsixError::StaleCapability)?;
    if current != expected {
        return Err(MsixError::StaleCapability);
    }
    if cap.control() & 0x8000 != 0 || current.msi.is_some_and(|msi| msi.control() & 1 != 0) {
        return Err(MsixError::AlreadyEnabled);
    }
    let masked = (cap.control() | 0x4000) & !0x8000;
    let entry = u32::from(index) * 16;
    let result = (|| {
        write16_checked(io, cap.offset() + 2, masked).map_err(MsixError::Config)?;
        for vector in 0..u32::from(cap.vectors()) {
            let control = vector * 16 + 12;
            let old = table.read32(control).ok_or(MsixError::ReadFailed)?;
            table_write(table, control, old | 1)?;
        }
        table_write(table, entry, message.address())?;
        table_write(table, entry + 4, 0)?;
        table_write(table, entry + 8, u32::from(message.data()))?;
        write16_checked(io, cap.offset() + 2, masked | 0x8000).map_err(MsixError::Config)?;
        let control = table.read32(entry + 12).ok_or(MsixError::ReadFailed)?;
        table_write(table, entry + 12, control & !1)?;
        write16_checked(io, cap.offset() + 2, (masked | 0x8000) & !0x4000)
            .map_err(MsixError::Config)
    })();
    if result.is_err() {
        // Do not claim rollback: these writes themselves can fail, and a
        // delivered interrupt may still require the retained handler/state.
        let _ = io.write16(cap.offset() + 2, masked);
        let _ = io.read32(cap.offset());
        if let Some(control) = table.read32(entry + 12) {
            let _ = table.write32(entry + 12, control | 1);
            let _ = table.read32(entry + 12);
        }
    }
    result
}

/// Mask and disable the function, without claiming interrupt-pipeline drain.
///
/// # Safety
/// The caller retains exclusive ownership of this same function and keeps
/// its handler/mappings live while quiescing and draining its sources.
pub unsafe fn disable(io: &mut impl ConfigSpace, cap: Msix) -> Result<(), MsixError> {
    let header = io.read32(cap.offset()).ok_or(MsixError::ReadFailed)?;
    if header as u8 != 0x11 || ((header >> 16) as u16 & 0x7ff) != cap.vectors() - 1 {
        return Err(MsixError::StaleCapability);
    }
    let control = ((header >> 16) as u16 | 0x4000) & !0x8000;
    write16_checked(io, cap.offset() + 2, control).map_err(MsixError::Config)
}

fn table_write(table: &mut impl TableAccess, offset: u32, value: u32) -> Result<(), MsixError> {
    if !table.write32(offset, value) {
        return Err(MsixError::WriteFailed);
    }
    if table.read32(offset).ok_or(MsixError::ReadFailed)? != value {
        return Err(MsixError::ReadbackMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::pci_interrupts::BarRegion;
    use super::*;

    fn cap(vectors: u16, pending: u32) -> Msix {
        Msix {
            offset: 0x40,
            control: vectors - 1,
            table: BarRegion { bir: 0, offset: 0 },
            pending: BarRegion {
                bir: 0,
                offset: pending,
            },
        }
    }

    fn bars(bytes: u64) -> [Option<MemoryBar>; 6] {
        [
            Some(MemoryBar::new(0x8000_0000, bytes).unwrap()),
            None,
            None,
            None,
            None,
            None,
        ]
    }

    #[test]
    fn table_and_pending_lengths_cover_boundaries() {
        for vectors in [1, 63, 64, 65, 2048] {
            let layout = Layout::new(cap(vectors, 0x8000), &bars(0x10000)).unwrap();
            assert_eq!(layout.table_bytes(), u32::from(vectors) * 16);
            assert_eq!(layout.pending_bytes(), u32::from(vectors).div_ceil(64) * 8);
            assert_eq!(layout.pending_physical(), 0x8000_8000);
        }
    }

    #[test]
    fn bad_extents_missing_bars_and_aliases_are_rejected() {
        assert_eq!(MemoryBar::new(0, 4096), Err(MsixError::InvalidBar));
        assert_eq!(MemoryBar::new(0x1000, 17), Err(MsixError::InvalidBar));
        assert_eq!(
            MemoryBar::new(u64::MAX - 4095, 4096),
            Err(MsixError::InvalidBar)
        );
        assert_eq!(
            Layout::new(cap(1, 0x1000), &[None; 6]),
            Err(MsixError::MissingBar)
        );
        assert_eq!(
            Layout::new(cap(1, 0x1000), &bars(0x1000)),
            Err(MsixError::OutOfBounds)
        );
        assert_eq!(
            Layout::new(cap(2, 16), &bars(0x1000)),
            Err(MsixError::Overlap)
        );
        let mut descriptor = cap(2, 0);
        descriptor.pending.bir = 1;
        let mut alias = bars(0x1000);
        alias[1] = alias[0];
        assert_eq!(Layout::new(descriptor, &alias), Err(MsixError::Overlap));
    }

    struct Config {
        words: [u32; 64],
        writes: usize,
        fail: Option<usize>,
    }

    impl Config {
        fn new() -> Self {
            let mut words = [0u32; 64];
            words[0] = 0x1110_1af4;
            words[1] = 1 << 20;
            words[13] = 0x40;
            words[16] = (1 << 16) | 0x11;
            words[18] = 0x1000;
            Self {
                words,
                writes: 0,
                fail: None,
            }
        }
    }

    impl ConfigSpace for Config {
        fn read32(&mut self, offset: u8) -> Option<u32> {
            Some(self.words[usize::from(offset / 4)])
        }

        fn write16(&mut self, offset: u8, value: u16) -> bool {
            assert_eq!(offset, 0x42);
            self.writes += 1;
            if self.fail == Some(self.writes) {
                return false;
            }
            self.words[16] = (u32::from(value) << 16) | 0x11;
            true
        }

        fn write32(&mut self, _: u8, _: u32) -> bool {
            panic!("MSI-X must not DWORD-write config space");
        }
    }

    struct Table {
        words: [u32; 8],
        writes: usize,
        fail: Option<usize>,
    }

    impl Table {
        fn new() -> Self {
            Self {
                words: [0; 8],
                writes: 0,
                fail: None,
            }
        }
    }

    impl TableAccess for Table {
        fn read32(&mut self, offset: u32) -> Option<u32> {
            assert_eq!(offset & 3, 0);
            self.words.get(offset as usize / 4).copied()
        }

        fn write32(&mut self, offset: u32, value: u32) -> bool {
            assert_eq!(offset & 3, 0);
            self.writes += 1;
            if self.fail == Some(self.writes) {
                return false;
            }
            self.words[offset as usize / 4] = value;
            true
        }
    }

    #[test]
    fn enables_only_selected_entry_and_disables_with_function_mask() {
        let mut io = Config::new();
        let expected = discover(&mut |offset| io.read32(offset)).unwrap();
        let layout = Layout::new(expected.msix.unwrap(), &bars(0x2000)).unwrap();
        let mut table = Table::new();
        // SAFETY: independent in-memory fakes cannot deliver interrupts.
        unsafe {
            enable_entry(
                &mut io,
                &mut table,
                expected,
                layout,
                1,
                Message::new(2, 0x51).unwrap(),
            )
        }
        .unwrap();
        assert_eq!(table.words, [0, 0, 0, 1, 0xfee0_2000, 0, 0x51, 0]);
        assert_eq!(io.words[16] >> 16, 0x8001);
        // SAFETY: same inert fake, no resources can escape.
        unsafe { disable(&mut io, expected.msix.unwrap()) }.unwrap();
        assert_eq!(io.words[16] >> 16, 0x4001);
    }

    #[test]
    fn every_table_and_config_write_failure_disables_in_fake() {
        for (config_fail, table_fail) in (1..=3)
            .map(|n| (Some(n), None))
            .chain((1..=6).map(|n| (None, Some(n))))
        {
            let mut io = Config::new();
            let expected = discover(&mut |offset| io.read32(offset)).unwrap();
            let layout = Layout::new(expected.msix.unwrap(), &bars(0x2000)).unwrap();
            let mut table = Table::new();
            io.fail = config_fail;
            table.fail = table_fail;
            // SAFETY: inert fakes with deliberate failed writes.
            assert!(
                unsafe {
                    enable_entry(
                        &mut io,
                        &mut table,
                        expected,
                        layout,
                        0,
                        Message::new(0, 0x51).unwrap(),
                    )
                }
                .is_err()
            );
            assert_eq!(io.words[16] >> 16, 0x4001);
            assert_eq!(table.words[3] & 1, 1);
        }
    }

    #[test]
    fn out_of_range_vector_is_rejected_before_any_write() {
        let mut io = Config::new();
        let expected = discover(&mut |offset| io.read32(offset)).unwrap();
        let layout = Layout::new(expected.msix.unwrap(), &bars(0x2000)).unwrap();
        let mut table = Table::new();
        // SAFETY: inert fakes, invalid index must fail before writing.
        assert_eq!(
            unsafe {
                enable_entry(
                    &mut io,
                    &mut table,
                    expected,
                    layout,
                    2,
                    Message::new(0, 0x51).unwrap(),
                )
            },
            Err(MsixError::OutOfBounds)
        );
        assert_eq!(io.writes, 0);
        assert_eq!(table.writes, 0);
    }
}
