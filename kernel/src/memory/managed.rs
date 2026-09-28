//! Native BSP-only adapter for the shared production VM, before APIC activation.
//! Invariants and evidence boundaries: docs/MANAGED_VM_NATIVE.md.
use super::virtual_memory::{Window, runtime};
use crate::BootInfo;
use core::arch::{asm, x86_64::__cpuid_count};
use vibrix_vmm::address::{ARENA_BASE, ARENA_SLOT, Page, PageRange, Permissions, PhysicalFrame};
use vibrix_vmm::frames::Frames;
use vibrix_vmm::walk::ADDRESS_MASK;
use vibrix_vmm::{Error, GuardedLayout, GuardedVm, Memory, Vm};

#[cfg(any(
    feature = "managed-write-probe",
    feature = "managed-unmap-probe",
    feature = "managed-guard-probe",
    feature = "managed-nx-probe"
))]
#[path = "managed_faults.rs"]
mod faults;

const POOL_FRAMES: usize = 24;

/// Private: cannot escape the early validation routine or outlive IF=0.
/// Scratch mappings and active arena mappings use the same WB memory type.
/// All memory access is bounded volatile raw access; no Rust reference escapes.
struct NativeMemory {
    window: Window,
    root: u64,
    reserved: [u64; POOL_FRAMES],
}

impl NativeMemory {
    fn check_entry(&self, frame: u64, index: usize) {
        assert!(index < 512, "managed VM entry index out of bounds");
        assert!(
            self.reserved.contains(&frame) || (frame == self.root && index == ARENA_SLOT),
            "managed VM access outside exclusively owned hierarchy"
        );
    }
}

impl Memory for NativeMemory {
    fn read_entry(&mut self, frame: u64, index: usize) -> u64 {
        self.check_entry(frame, index);
        // SAFETY: private construction reserves this RAM/root; exclusive
        // scratch slot 0 is mapped WB/NX only for this bounded raw access.
        unsafe {
            let address = self.window.map(0, frame, false).expect("VM scratch map");
            let entry = (address as *const u64).add(index).read_volatile();
            self.window.unmap(0).expect("VM scratch unmap");
            entry
        }
    }

    fn write_entry(&mut self, frame: u64, index: usize, value: u64) {
        self.check_entry(frame, index);
        // SAFETY: sole BSP, IF=0, exact owned PTE slot. Core publishes aligned
        // u64 entries before requesting invalidation; other root slots are denied.
        unsafe {
            let address = self.window.map(0, frame, true).expect("VM scratch map");
            (address as *mut u64).add(index).write_volatile(value);
            self.window.unmap(0).expect("VM scratch unmap");
        }
    }

    fn zero_frame(&mut self, frame: u64) {
        assert!(
            self.reserved.contains(&frame),
            "cannot zero foreign/root RAM"
        );
        // SAFETY: frame is from the reserved conventional pool; the core zeros
        // it before publication/reuse. No live raw accesses or references exist.
        unsafe {
            let address = self.window.map(0, frame, true).expect("VM zero scratch");
            for index in 0..512 {
                (address as *mut u64).add(index).write_volatile(0);
            }
            self.window.unmap(0).expect("VM zero scratch unmap");
        }
    }

    fn invalidate(&mut self, address: u64) {
        assert!(Page::new(address).is_ok(), "foreign VM invalidation");
        // SAFETY: current CR3, CPL0, one CPU, PCID disabled. INVLPG completes
        // leaf/paging-cache invalidation before the core recycles any backing.
        // Deliberately no nomem: table stores cannot move after this operation.
        unsafe { asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)) };
    }
}

/// # Safety
/// CPL0 after the matched v3 loader, single BSP with no APs started.
unsafe fn cpu_configuration() -> Result<(u64, u8), Error> {
    let root: u64;
    let cr4: u64;
    let flags: u64;
    // SAFETY: privileged reads at CPL0; pushfq/pop restores the current stack.
    unsafe {
        asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags));
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    if flags & (1 << 9) != 0 || cr4 & ((1 << 12) | (1 << 17)) != 0 || root & !ADDRESS_MASK != 0 {
        crate::println!("managed VM rejected CPU: CR3={root:#x} CR4={cr4:#x} RFLAGS={flags:#x}");
        return Err(Error::InvalidRoot);
    }
    if __cpuid_count(1, 0).edx & (1 << 16) == 0 {
        return Err(Error::InvalidRoot);
    }
    let pat_low: u32;
    // SAFETY: CPUID above establishes IA32_PAT availability. Read only.
    unsafe {
        asm!("rdmsr", in("ecx") 0x277u32, out("eax") pat_low, out("edx") _, options(nomem, nostack, preserves_flags));
    }
    if pat_low & 0xff != 6 {
        return Err(Error::InvalidRoot);
    }
    let extended = __cpuid_count(0x8000_0000, 0).eax;
    let bits = if extended >= 0x8000_0008 {
        (__cpuid_count(0x8000_0008, 0).eax & 0xff) as u8
    } else {
        36
    };
    PhysicalFrame::new(root, bits)?;
    // SAFETY: the single allocator has already been initialized; map immutable.
    let descriptor = unsafe { super::firmware_descriptor_at(root) };
    if !descriptor.is_some_and(|(kind, attributes)| {
        kind == 2 && attributes & 8 != 0 && attributes & (1 << 63) == 0
    }) {
        crate::println!("managed VM rejected root backing: {root:#x} {descriptor:?}");
        return Err(Error::InvalidRoot);
    }
    Ok((root, bits))
}

/// # Safety
/// The page is currently owned and mapped RW; caller retires raw accesses
/// before any protect/unmap. Loop stays within one complete initialized page.
unsafe fn fill(page: Page, pattern: u64) {
    for index in 0..512 {
        // SAFETY: bounded aligned raw store into the live owned page.
        unsafe {
            (page.address() as *mut u64)
                .add(index)
                .write_volatile(pattern ^ index as u64);
        }
    }
}

/// # Safety
/// The page is currently mapped readable and fully initialized by the mapper.
unsafe fn check(page: Page, pattern: Option<u64>) -> Result<(), Error> {
    for index in 0..512 {
        let expected = pattern.map_or(0, |value| value ^ index as u64);
        // SAFETY: bounded aligned read from initialized live mapped RAM.
        let actual = unsafe { (page.address() as *const u64).add(index).read_volatile() };
        if actual != expected {
            return Err(Error::CorruptEntry);
        }
    }
    Ok(())
}

/// Run the real mapper against the active PML4, not a fabricated host trace.
/// All arena/scratch mappings are retired on success. The monotonic physical
/// allocator retains the 24-page supply; it is not returned to another owner.
///
/// # Safety
/// Matched BootInfo v3 with live loader root/scratch table; initialized frame
/// allocator; sole BSP with IF=0, no other Window or arena owner, no DMA to
/// acquired RAM. Caller must fail-stop on error and not continue initialization.
pub unsafe fn smoke_test(info: &BootInfo) -> Result<(), Error> {
    // SAFETY: entry contract covers all privileged and raw accesses below.
    // Every frame comes from the one allocator; all references are numeric and
    // every volatile payload access finishes before permission/lifetime changes.
    unsafe {
        let (root, bits) = cpu_configuration()?;
        let mut frames = Frames::<POOL_FRAMES>::new(bits)?;
        let mut reserved = [0; POOL_FRAMES];
        for slot in &mut reserved {
            let frame = super::allocate_frame().ok_or(Error::OutOfFrames)?;
            if !super::firmware_descriptor_at(frame).is_some_and(|(kind, attributes)| {
                kind == 7 && attributes & 8 != 0 && attributes & (1 << 63) == 0
            }) {
                return Err(Error::InvalidFrame);
            }
            frames.register(frame)?;
            *slot = frame;
        }
        {
            let backend = NativeMemory {
                window: runtime::from_boot_info(info).map_err(|_| Error::InvalidRoot)?,
                root,
                reserved,
            };
            let mut vm = Vm::new(root, backend, frames)?;
            let pages = [0, 1 << 21, 1 << 30].map(|offset| Page::new(ARENA_BASE + offset).unwrap());
            let mut physical = [0; 3];
            for (index, page) in pages.iter().copied().enumerate() {
                physical[index] = vm.map_zeroed(page, Permissions::ReadWrite)?;
                check(page, None)?;
                fill(page, 0xfeed_0000 + index as u64);
            }
            for (index, page) in pages.iter().copied().enumerate() {
                check(page, Some(0xfeed_0000 + index as u64))?;
            }
            if vm.table_frames() != 6 || vm.data_frames() != 3 {
                return Err(Error::CorruptEntry);
            }
            vm.protect(pages[0], Permissions::ReadOnly)?;
            check(pages[0], Some(0xfeed_0000))?;
            if vm.query(pages[0])?.ok_or(Error::NotMapped)?.permissions != Permissions::ReadOnly {
                return Err(Error::CorruptEntry);
            }
            vm.unmap(pages[0])?;
            if vm.map_zeroed(pages[0], Permissions::ReadWrite)? != physical[0] {
                return Err(Error::CorruptEntry);
            }
            check(pages[0], None)?;
            check(pages[1], Some(0xfeed_0001))?;
            check(pages[2], Some(0xfeed_0002))?;
            for page in pages {
                vm.unmap(page)?;
            }
            if vm.free_frames() != POOL_FRAMES {
                return Err(Error::CorruptEntry);
            }
            let region = PageRange::new(Page::new(ARENA_BASE + (1 << 21) - 4096)?, 3)?;
            vm.map_region(region, Permissions::ReadWrite)?;
            for index in 0..region.count() {
                let page = region.page(index).ok_or(Error::InvalidRange)?;
                check(page, None)?;
                fill(page, 0xcafe_0000 + index as u64);
                check(page, Some(0xcafe_0000 + index as u64))?;
            }
            vm.unmap_region(region)?;
            let mut guarded = GuardedVm::<_, POOL_FRAMES, 4>::new(vm)?;
            let layout = GuardedLayout::new(Page::new(ARENA_BASE)?, 2)?;
            let id = guarded.allocate(layout)?;
            if guarded.query(layout.lower_guard())?.is_some()
                || guarded.query(layout.upper_guard())?.is_some()
            {
                return Err(Error::CorruptEntry);
            }
            for index in 0..layout.payload().count() {
                let page = layout.payload().page(index).ok_or(Error::InvalidRange)?;
                check(page, None)?;
                fill(page, 0xface_0000 + index as u64);
                check(page, Some(0xface_0000 + index as u64))?;
            }
            let overlap = GuardedLayout::new(layout.upper_guard(), 1)?;
            if guarded.allocate(overlap) != Err(Error::AddressInUse) {
                return Err(Error::CorruptEntry);
            }
            guarded.release(id)?;
            if guarded.free_frames() != POOL_FRAMES || guarded.active_allocations() != 0 {
                return Err(Error::CorruptEntry);
            }
        }
        // The previous backend/Window has been dropped after complete teardown.
        // Reconstructing the scratch owner independently verifies all slots are
        // empty; explicitly verify the active root's arena slot was cleared too.
        let mut window = runtime::from_boot_info(info).map_err(|_| Error::InvalidRoot)?;
        let mapped = window.map(0, root, false).map_err(|_| Error::InvalidRoot)?;
        let entry = (mapped as *const u64).add(ARENA_SLOT).read_volatile();
        window.unmap(0).map_err(|_| Error::InvalidRoot)?;
        if entry != 0 {
            return Err(Error::CorruptEntry);
        }
    }
    crate::debugcon::write("VIBRIX: kernel managed VM native verified\r\n");
    crate::println!(
        "managed VM: dynamic map/protect/reclaim, zeroed regions and reserved guards verified"
    );
    crate::println!("managed VM: 24 retained pool frames free; active arena and scratch empty");
    // SAFETY: normal validation retired the arena and scratch owner; the BSP
    // remains IRQ-off. This explicitly enabled terminal test uses a new pool.
    #[cfg(any(
        feature = "managed-write-probe",
        feature = "managed-unmap-probe",
        feature = "managed-guard-probe",
        feature = "managed-nx-probe"
    ))]
    unsafe {
        faults::run(info)?;
    }
    Ok(())
}
