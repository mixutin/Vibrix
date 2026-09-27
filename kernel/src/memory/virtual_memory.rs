//! BootInfo v3 bounded, supervisor-only, non-executable 4 KiB mappings.
#[path = "../../../shared/vm_window.rs"]
mod window;

const ADDRESS: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const WRITE: u64 = 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const NX: u64 = 1 << 63;

#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    InvalidAddress,
    Occupied,
    Unmapped,
    InvalidTable,
}

/// Exclusive owner of one preallocated leaf table. No frame ownership is
/// implied by a mapping, and dropping this value does not free any frames.
pub struct Window {
    table: *mut u64,
    physical_bits: u8,
    invalidate: unsafe fn(u64),
}

fn page_address(index: usize) -> Result<u64, MapError> {
    if index >= window::PAGES {
        return Err(MapError::InvalidAddress);
    }
    Ok(window::BASE + index as u64 * window::PAGE_BYTES)
}

fn leaf(physical: u64, bits: u8, writable: bool) -> Result<u64, MapError> {
    if !(36..=52).contains(&bits)
        || physical == 0
        || physical & !ADDRESS != 0
        || physical >= (1u64 << bits)
    {
        return Err(MapError::InvalidAddress);
    }
    Ok(physical | PRESENT | NX | if writable { WRITE } else { 0 })
}

impl Window {
    /// # Safety
    /// Table has 512 readable/writable, u64-aligned entries, exclusively owned
    /// by the sole boot CPU with IRQs off. Its parent hierarchy maps BASE;
    /// invalidate must flush a local translation for the supplied address.
    unsafe fn from_table(
        table: *mut u64,
        physical_bits: u8,
        invalidate: unsafe fn(u64),
    ) -> Result<Self, MapError> {
        if table.is_null() || !(36..=52).contains(&physical_bits) {
            return Err(MapError::InvalidTable);
        }
        for i in 0..window::PAGES {
            // SAFETY: full mapped table and exclusive ownership required above.
            if unsafe { table.add(i).read_volatile() } != 0 {
                return Err(MapError::Occupied);
            }
        }
        Ok(Self {
            table,
            physical_bits,
            invalidate,
        })
    }

    /// Map a caller-owned RAM frame; existing mappings are never overwritten.
    /// # Safety
    /// Sole CPU, IRQs off. Frame must be live, reserved RAM with compatible WB
    /// cache policy; any aliases must obey Rust lifetimes. Initialize before
    /// reading. The caller retains the frame until all mappings are retired.
    pub unsafe fn map(
        &mut self,
        index: usize,
        physical: u64,
        writable: bool,
    ) -> Result<u64, MapError> {
        let address = page_address(index)?;
        let entry = leaf(physical, self.physical_bits, writable)?;
        // SAFETY: index is bounded and this object exclusively owns the table.
        unsafe {
            let slot = self.table.add(index);
            if slot.read_volatile() != 0 {
                return Err(MapError::Occupied);
            }
            slot.write_volatile(entry);
            (self.invalidate)(address);
        }
        Ok(address)
    }

    /// Map one page of read-only device configuration MMIO as supervisor
    /// NX and UC (PWT+PCD under the loader's default x86-64 PAT).
    /// Unlike map(), the physical page must NOT belong to ordinary RAM.
    ///
    /// # Safety
    /// Sole boot CPU, IF=0, no other virtual/cache aliases with conflicting
    /// effective memory types. The caller must validate actual PCI ECAM
    /// physical ownership and span before creating/using any MMIO pointer.
    /// The mapped page must stay live and no references may escape unmap().
    pub unsafe fn map_mmio_readonly(
        &mut self,
        index: usize,
        physical: u64,
    ) -> Result<u64, MapError> {
        let address = page_address(index)?;
        let entry = leaf(physical, self.physical_bits, false)? | PWT | PCD;
        // SAFETY: same exclusively owned live PT as map(), but UC and RO.
        unsafe {
            let slot = self.table.add(index);
            if slot.read_volatile() != 0 {
                return Err(MapError::Occupied);
            }
            slot.write_volatile(entry);
            (self.invalidate)(address);
        }
        Ok(address)
    }

    /// Map one page of writable device MMIO as supervisor NX and UC.
    /// Use only for architecturally defined control registers whose writes
    /// are explicitly bounded by the caller; never for ordinary RAM.
    ///
    /// # Safety
    /// Sole boot CPU, IF=0, validated device MMIO ownership, no conflicting
    /// cache aliases, and no Rust reference may outlive unmap().
    pub unsafe fn map_mmio_writable(
        &mut self,
        index: usize,
        physical: u64,
    ) -> Result<u64, MapError> {
        let address = page_address(index)?;
        let entry = leaf(physical, self.physical_bits, true)? | PWT | PCD;
        // SAFETY: exclusive PT ownership; UC writable leaf for device MMIO.
        unsafe {
            let slot = self.table.add(index);
            if slot.read_volatile() != 0 {
                return Err(MapError::Occupied);
            }
            slot.write_volatile(entry);
            (self.invalidate)(address);
        }
        Ok(address)
    }

    /// # Safety
    /// Sole CPU, IRQs off; no references/accesses may outlive this mapping.
    /// The returned physical frame remains owned and is not automatically freed.
    pub unsafe fn unmap(&mut self, index: usize) -> Result<u64, MapError> {
        let address = page_address(index)?;
        // SAFETY: validated index, exclusive table and retired caller accesses.
        unsafe {
            let slot = self.table.add(index);
            let current = slot.read_volatile();
            if current & PRESENT == 0 {
                return Err(MapError::Unmapped);
            }
            slot.write_volatile(0);
            (self.invalidate)(address);
            Ok(current & ADDRESS)
        }
    }

    /// # Safety
    /// Sole CPU, IRQs off. Retire incompatible references before changing
    /// permissions, and own the frame before granting write permission.
    pub unsafe fn protect(&mut self, index: usize, writable: bool) -> Result<(), MapError> {
        let address = page_address(index)?;
        // SAFETY: validated index and exclusive table; preserve hardware A/D.
        unsafe {
            let slot = self.table.add(index);
            let current = slot.read_volatile();
            if current & PRESENT == 0 {
                return Err(MapError::Unmapped);
            }
            slot.write_volatile((current & !WRITE) | if writable { WRITE } else { 0 });
            (self.invalidate)(address);
        }
        Ok(())
    }

    /// Numeric query only. Does not grant access or ownership of physical RAM.
    pub fn translation(&self, index: usize) -> Result<Option<(u64, bool)>, MapError> {
        page_address(index)?;
        // SAFETY: construction grants exclusive live table access; index bounded.
        let entry = unsafe { self.table.add(index).read_volatile() };
        Ok((entry & PRESENT != 0).then_some((entry & ADDRESS, entry & WRITE != 0)))
    }
}

#[cfg(target_os = "none")]
pub mod runtime {
    use super::*;
    use crate::BootInfo;
    use core::arch::{asm, x86_64::__cpuid_count};

    unsafe fn invalidate(address: u64) {
        // SAFETY: CPL0, sole CPU; caller published a leaf then requests local
        // invalidation. No nomem option: compiler may not reorder PTE stores.
        unsafe { asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)) };
    }

    /// # Safety
    /// Matched v3 loader has mapped and retained kernel_window_table RW/NX,
    /// and linked it to BASE. Sole boot CPU, IF=0, no other Window exists.
    pub unsafe fn from_boot_info(info: &BootInfo) -> Result<Window, MapError> {
        info.validate().map_err(|_| MapError::InvalidTable)?;
        let extended = __cpuid_count(0x8000_0000, 0).eax;
        let bits = if extended >= 0x8000_0008 {
            (__cpuid_count(0x8000_0008, 0).eax & 0xff) as u8
        } else {
            36
        };
        leaf(info.kernel_window_table, bits, true)?;
        // SAFETY: supervisor writes must obey read-only PTEs. Preserve all
        // existing CR0 bits. Loader has already enabled NX and four-level paging.
        unsafe {
            let mut cr0: u64;
            asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
            cr0 |= 1 << 16;
            asm!("mov cr0, {}", in(reg) cr0, options(nostack, preserves_flags));
            Window::from_table(info.kernel_window_table as *mut u64, bits, invalidate)
        }
    }

    /// # Safety
    /// Same v3 table/CPU contract as from_boot_info; frame allocator initialized.
    pub unsafe fn smoke_test(info: &BootInfo) -> Result<(), MapError> {
        // SAFETY: all pages are exclusively claimed, accessed while mapped,
        // and retained (not freed) after the final unmap. No references escape.
        unsafe {
            let mut vm = from_boot_info(info)?;
            let a = crate::memory::allocate_frame().ok_or(MapError::InvalidAddress)?;
            let b = crate::memory::allocate_frame().ok_or(MapError::InvalidAddress)?;
            let address = vm.map(0, a, true)?;
            let other = vm.map(1, b, true)?;
            let first = address as *mut u64;
            let second = other as *mut u64;
            for i in 0..512 {
                first.add(i).write_volatile(0x1234_5678_9abc_def0);
                second.add(i).write_volatile(0xfedc_ba98_7654_3210);
            }
            for i in 0..512 {
                if first.add(i).read_volatile() != 0x1234_5678_9abc_def0
                    || second.add(i).read_volatile() != 0xfedc_ba98_7654_3210
                {
                    return Err(MapError::InvalidTable);
                }
            }
            vm.protect(0, false)?;
            if vm.translation(0)? != Some((a, false)) {
                return Err(MapError::InvalidTable);
            }
            vm.unmap(0)?;
            vm.unmap(1)?;
            // Reusing the same virtual address for a different physical frame
            // must observe b, never the prior cached translation of a.
            vm.map(0, b, false)?;
            for i in 0..512 {
                if first.add(i).read_volatile() != 0xfedc_ba98_7654_3210 {
                    return Err(MapError::InvalidTable);
                }
            }
            vm.unmap(0)?;
            if vm.translation(0)?.is_some() {
                return Err(MapError::InvalidTable);
            }
        }
        Ok(())
    }

    /// QEMU-only actual CPU permission/unmap faults after ordinary smoke passes.
    /// # Safety
    /// Same v3 mapping ownership contract as smoke_test.
    #[cfg(any(feature = "vm-write-probe", feature = "vm-unmap-probe"))]
    pub unsafe fn fault_probe(info: &BootInfo) {
        unsafe {
            let mut vm = from_boot_info(info).expect("empty mapping window");
            let frame = crate::memory::allocate_frame().expect("probe frame");
            let address = vm.map(0, frame, true).expect("probe mapping");
            (address as *mut u64).write_volatile(0x55);
            #[cfg(feature = "vm-write-probe")]
            {
                vm.protect(0, false).expect("read-only probe page");
                asm!("mov qword ptr [rax], 1", in("rax") address, options(nostack));
            }
            #[cfg(feature = "vm-unmap-probe")]
            {
                vm.unmap(0).expect("unmapped probe page");
                asm!("mov rax, qword ptr [rdx]", in("rdx") address, out("rax") _, options(nostack));
            }
        }
        panic!("VM probe failed to fault");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn no_flush(_: u64) {}

    #[test]
    fn maps_queries_protects_unmaps_and_reuses() {
        let mut table = [0u64; 512];
        let mut vm = unsafe { Window::from_table(table.as_mut_ptr(), 48, no_flush) }.unwrap();
        assert_eq!(
            unsafe { vm.map(511, 0x2000, true) },
            Ok(window::BASE + 511 * 4096)
        );
        assert_eq!(vm.translation(511), Ok(Some((0x2000, true))));
        assert_eq!(
            unsafe { vm.map(511, 0x3000, true) },
            Err(MapError::Occupied)
        );
        unsafe { vm.protect(511, false) }.unwrap();
        assert_eq!(vm.translation(511), Ok(Some((0x2000, false))));
        assert_eq!(unsafe { vm.unmap(511) }, Ok(0x2000));
        assert_eq!(vm.translation(511), Ok(None));
        assert_eq!(unsafe { vm.unmap(511) }, Err(MapError::Unmapped));
        assert_eq!(unsafe { vm.protect(511, true) }, Err(MapError::Unmapped));
        unsafe { vm.map(511, 0x3000, false) }.unwrap();
        assert_eq!(table[511], 0x3000 | PRESENT | NX);
    }

    #[test]
    fn mmio_maps_readonly_nonexecuting_and_uncached_without_ram_alias() {
        let mut table = [0u64; 512];
        let mut vm = unsafe { Window::from_table(table.as_mut_ptr(), 48, no_flush) }.unwrap();
        assert_eq!(
            unsafe { vm.map_mmio_readonly(1, 0xe000_0000) },
            Ok(window::BASE + 4096)
        );
        assert_eq!(table[1], 0xe000_0000 | PRESENT | NX | PWT | PCD);
        assert_eq!(vm.translation(1), Ok(Some((0xe000_0000, false))));
        assert_eq!(
            unsafe { vm.map_mmio_readonly(1, 0xe000_1000) },
            Err(MapError::Occupied)
        );
        assert_eq!(unsafe { vm.unmap(1) }, Ok(0xe000_0000));
        assert_eq!(table[1], 0);
        assert_eq!(
            unsafe { vm.map_mmio_readonly(512, 0xe000_0000) },
            Err(MapError::InvalidAddress)
        );
        assert_eq!(
            unsafe { vm.map_mmio_readonly(0, 0xe000_0001) },
            Err(MapError::InvalidAddress)
        );
    }

    #[test]
    fn writable_mmio_mapping_sets_write_and_uc_bits() {
        let mut table = [0u64; 512];
        let mut vm = unsafe { Window::from_table(table.as_mut_ptr(), 48, no_flush) }.unwrap();
        assert_eq!(
            unsafe { vm.map_mmio_writable(2, 0xfec0_0000) },
            Ok(window::BASE + 2 * 4096)
        );
        assert_eq!(table[2], 0xfec0_0000 | PRESENT | WRITE | NX | PWT | PCD);
        assert_eq!(vm.translation(2), Ok(Some((0xfec0_0000, true))));
        assert_eq!(unsafe { vm.unmap(2) }, Ok(0xfec0_0000));
    }

    #[test]
    fn malformed_requests_leave_table_unchanged() {
        let mut table = [0u64; 512];
        let mut vm = unsafe { Window::from_table(table.as_mut_ptr(), 36, no_flush) }.unwrap();
        for index in [512, usize::MAX] {
            assert_eq!(
                unsafe { vm.map(index, 4096, true) },
                Err(MapError::InvalidAddress)
            );
            assert_eq!(unsafe { vm.unmap(index) }, Err(MapError::InvalidAddress));
        }
        for physical in [0, 1, 4097, 1u64 << 36, u64::MAX] {
            assert_eq!(
                unsafe { vm.map(0, physical, true) },
                Err(MapError::InvalidAddress)
            );
        }
        assert!(table.iter().all(|&v| v == 0));
        assert!(leaf(4096, 35, true).is_err());
        assert!(leaf(4096, 53, true).is_err());
    }

    #[test]
    fn nonempty_handoff_is_rejected() {
        let mut table = [0u64; 512];
        table[511] = 1;
        assert!(matches!(
            unsafe { Window::from_table(table.as_mut_ptr(), 48, no_flush) },
            Err(MapError::Occupied)
        ));
    }
}
