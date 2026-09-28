//! Native BSP-only adapter for the shared production VM, before APIC activation.
//! Invariants and evidence boundaries: docs/MANAGED_VM_NATIVE.md.
use super::virtual_memory::{SlotWindow, Window, runtime};
use crate::BootInfo;
use core::arch::{asm, x86_64::__cpuid_count};
use core::cell::UnsafeCell;
use vibrix_vmm::address::{
    ARENA_BASE, ARENA_SLOT, PAGE_BYTES, Page, PageRange, Permissions, PhysicalFrame, Privilege,
};
use vibrix_vmm::frames::Frames;
use vibrix_vmm::walk::ADDRESS_MASK;
use vibrix_vmm::{Error, GuardedId, GuardedLayout, GuardedVm, Memory, Translation, Vm};

#[cfg(any(
    feature = "managed-write-probe",
    feature = "managed-unmap-probe",
    feature = "managed-guard-probe",
    feature = "managed-nx-probe"
))]
#[path = "managed_faults.rs"]
mod faults;

const POOL_FRAMES: usize = 24;
const RUNTIME_POOL_FRAMES: usize = 96;
const RUNTIME_GUARDED_SLOTS: usize = 8;
const RUNTIME_SCRATCH_SLOT: usize = 511;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    AlreadyInitialized,
    NotInitialized,
    InterruptsDisabled,
    Scratch,
    Vm(Error),
}

impl From<Error> for RuntimeError {
    fn from(error: Error) -> Self {
        Self::Vm(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeAllocation {
    id: GuardedId,
    layout: GuardedLayout,
}

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

struct RuntimeMemory {
    window: SlotWindow,
    root: u64,
    reserved: [u64; RUNTIME_POOL_FRAMES],
}

impl RuntimeMemory {
    fn check_entry(&self, frame: u64, index: usize) {
        assert!(index < 512, "runtime managed VM entry index out of bounds");
        assert!(
            self.reserved.contains(&frame) || (frame == self.root && index == ARENA_SLOT),
            "runtime managed VM access outside exclusively owned hierarchy"
        );
    }
}

impl Memory for RuntimeMemory {
    fn read_entry(&mut self, frame: u64, index: usize) -> u64 {
        self.check_entry(frame, index);
        // SAFETY: every operation is wrapped by the single-BSP interrupt mask;
        // slot 511 is exclusively reserved for this backend after APIC setup.
        unsafe {
            let address = self
                .window
                .map(frame, false)
                .expect("runtime VM scratch map");
            let entry = (address as *const u64).add(index).read_volatile();
            self.window.unmap().expect("runtime VM scratch unmap");
            entry
        }
    }

    fn write_entry(&mut self, frame: u64, index: usize, value: u64) {
        self.check_entry(frame, index);
        // SAFETY: same exclusive scratch slot; aligned entry publication
        // completes before the local invalidation performed by SlotWindow.
        unsafe {
            let address = self
                .window
                .map(frame, true)
                .expect("runtime VM scratch map");
            (address as *mut u64).add(index).write_volatile(value);
            self.window.unmap().expect("runtime VM scratch unmap");
        }
    }

    fn zero_frame(&mut self, frame: u64) {
        assert!(
            self.reserved.contains(&frame),
            "cannot zero foreign runtime VM RAM"
        );
        // SAFETY: frame belongs to this retained pool and is unpublished while
        // zeroed. No raw access escapes the temporary scratch mapping.
        unsafe {
            let address = self
                .window
                .map(frame, true)
                .expect("runtime VM zero scratch");
            for index in 0..512 {
                (address as *mut u64).add(index).write_volatile(0);
            }
            self.window.unmap().expect("runtime VM zero scratch unmap");
        }
    }

    fn invalidate(&mut self, address: u64) {
        assert!(
            Page::new(address).is_ok(),
            "foreign runtime VM invalidation"
        );
        // SAFETY: one BSP only. Runtime mutation masks interrupts, so no local
        // preemption can observe a partially changed hierarchy.
        unsafe { asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)) };
    }
}

type KernelRuntimeVm = GuardedVm<RuntimeMemory, RUNTIME_POOL_FRAMES, RUNTIME_GUARDED_SLOTS>;

struct RuntimeState(UnsafeCell<Option<KernelRuntimeVm>>);

// SAFETY: there is one BSP. Every post-initialization access masks local
// interrupts before borrowing the UnsafeCell. SMP must replace this contract.
unsafe impl Sync for RuntimeState {}

static RUNTIME_VM: RuntimeState = RuntimeState(UnsafeCell::new(None));

struct InterruptMask {
    restore: bool,
}

impl InterruptMask {
    fn enter() -> Self {
        let flags: u64;
        // SAFETY: kernel executes at CPL0. Record IF before masking so callers
        // from IRQ context stay IRQ-off while thread context is restored to IF=1.
        unsafe {
            asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
            asm!("cli", options(nomem, nostack));
        }
        Self {
            restore: flags & (1 << 9) != 0,
        }
    }
}

impl Drop for InterruptMask {
    fn drop(&mut self) {
        if self.restore {
            // SAFETY: restoring the caller's prior local interrupt state after
            // the RuntimeVm mutable borrow and scratch PTE mutation have ended.
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
    }
}

fn interrupts_enabled() -> bool {
    let flags: u64;
    // SAFETY: read-only RFLAGS inspection at CPL0.
    unsafe { asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags)) };
    flags & (1 << 9) != 0
}

fn with_runtime<T>(
    operation: impl FnOnce(&mut KernelRuntimeVm) -> Result<T, Error>,
) -> Result<T, RuntimeError> {
    let _mask = InterruptMask::enter();
    // SAFETY: local interrupts are masked before the only mutable static borrow;
    // no AP exists. Nested/NMI mutation is outside the supported contract.
    let state = unsafe { &mut *RUNTIME_VM.0.get() };
    let vm = state.as_mut().ok_or(RuntimeError::NotInitialized)?;
    operation(vm).map_err(RuntimeError::Vm)
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

/// Reserve one bounded pool from the monotonic firmware-map allocator.
///
/// # Safety
/// Sole BSP with IF=0; the global early allocator has been initialized and no
/// other caller may claim frames concurrently.
unsafe fn reserve_pool<const N: usize>(bits: u8) -> Result<(Frames<N>, [u64; N]), Error> {
    let mut frames = Frames::<N>::new(bits)?;
    let mut reserved = [0; N];
    for slot in &mut reserved {
        // SAFETY: inherited sole-BSP/IF=0 allocator contract.
        let frame = unsafe { super::allocate_frame() }.ok_or(Error::OutOfFrames)?;
        // SAFETY: same immutable retained firmware map and allocator contract.
        if !unsafe { super::firmware_descriptor_at(frame) }.is_some_and(|(kind, attributes)| {
            kind == 7 && attributes & 8 != 0 && attributes & (1 << 63) == 0
        }) {
            return Err(Error::InvalidFrame);
        }
        frames.register(frame)?;
        *slot = frame;
    }
    Ok((frames, reserved))
}

/// Initialize the persistent, supervisor-only kernel VM after the APIC has
/// retained window slots 0 and 1 but before IF is enabled.
///
/// # Safety
/// Sole BSP, IF=0, matched BootInfo v3, active root unchanged since the early
/// native VM proof, APIC setup has completed and no owner uses scratch slot 511.
pub unsafe fn init_runtime(info: &BootInfo) -> Result<(), RuntimeError> {
    // SAFETY: caller guarantees one-time pre-STI initialization.
    let state = unsafe { &mut *RUNTIME_VM.0.get() };
    if state.is_some() {
        return Err(RuntimeError::AlreadyInitialized);
    }
    // SAFETY: same privileged CPU/root validation as the early native proof.
    let (root, bits) = unsafe { cpu_configuration() }.map_err(RuntimeError::Vm)?;
    // SAFETY: still pre-STI and no concurrent frame allocator user.
    let (frames, reserved) =
        unsafe { reserve_pool::<RUNTIME_POOL_FRAMES>(bits) }.map_err(RuntimeError::Vm)?;
    // SAFETY: APIC owns other slots only; slot 511 is reserved for this service.
    let window = unsafe { runtime::slot_from_boot_info(info, RUNTIME_SCRATCH_SLOT) }
        .map_err(|_| RuntimeError::Scratch)?;
    let backend = RuntimeMemory {
        window,
        root,
        reserved,
    };
    let vm = Vm::new(root, backend, frames).map_err(RuntimeError::Vm)?;
    *state = Some(GuardedVm::new(vm).map_err(RuntimeError::Vm)?);
    Ok(())
}

pub fn runtime_allocate(
    lower_guard: u64,
    payload_pages: usize,
) -> Result<RuntimeAllocation, RuntimeError> {
    let layout = GuardedLayout::new(Page::new(lower_guard)?, payload_pages)?;
    let id = with_runtime(|vm| vm.allocate(layout))?;
    Ok(RuntimeAllocation { id, layout })
}

pub fn runtime_release(allocation: RuntimeAllocation) -> Result<(), RuntimeError> {
    with_runtime(|vm| vm.release(allocation.id))
}

pub fn runtime_query(address: u64) -> Result<Option<Translation>, RuntimeError> {
    let page = Page::new(address)?;
    with_runtime(|vm| vm.query(page))
}

#[cfg(feature = "ring3-probe")]
pub fn prepare_ring3_probe() -> Result<(u64, u64), RuntimeError> {
    const CODE_GUARD: u64 = ARENA_BASE + 8 * PAGE_BYTES;
    const STACK_GUARD: u64 = ARENA_BASE + 12 * PAGE_BYTES;
    const USER_CODE: [u8; 4] = [0xcd, 0x80, 0x0f, 0x0b]; // int 0x80; ud2

    with_runtime(|vm| {
        let code_layout = GuardedLayout::new(Page::new(CODE_GUARD)?, 1)?;
        let stack_layout = GuardedLayout::new(Page::new(STACK_GUARD)?, 1)?;
        let code_id = vm.allocate_user(code_layout)?;
        let stack_id = match vm.allocate_user(stack_layout) {
            Ok(id) => id,
            Err(error) => {
                vm.release(code_id)
                    .expect("ring3 rollback owns code allocation");
                return Err(error);
            }
        };

        let result = (|| {
            let code_page = code_layout
                .payload()
                .page(0)
                .ok_or(Error::InvalidRange)?;
            let stack_page = stack_layout
                .payload()
                .page(0)
                .ok_or(Error::InvalidRange)?;
            // SAFETY: code_page is a live, exclusively owned RW user mapping.
            // Volatile byte stores finish before protection changes to RX.
            unsafe {
                for (index, byte) in USER_CODE.iter().copied().enumerate() {
                    (code_page.address() as *mut u8)
                        .add(index)
                        .write_volatile(byte);
                }
            }
            let previous = vm.protect(code_id, 0, Permissions::ReadExecute)?;
            if previous.privilege != Privilege::User
                || previous.permissions != Permissions::ReadWrite
            {
                return Err(Error::CorruptEntry);
            }
            let code = vm.query(code_page)?.ok_or(Error::NotMapped)?;
            let stack = vm.query(stack_page)?.ok_or(Error::NotMapped)?;
            if code.privilege != Privilege::User
                || code.permissions != Permissions::ReadExecute
                || stack.privilege != Privilege::User
                || stack.permissions != Permissions::ReadWrite
            {
                return Err(Error::CorruptEntry);
            }
            Ok((code_page.address(), stack_layout.payload_end()))
        })();

        if result.is_err() {
            vm.release(stack_id)
                .expect("ring3 rollback owns stack allocation");
            vm.release(code_id)
                .expect("ring3 rollback owns code allocation");
        }
        // On success both allocations deliberately remain live: IRETQ consumes
        // their virtual addresses and the diagnostic trap never returns.
        result
    })
}

pub fn runtime_smoke_test() -> Result<(), RuntimeError> {
    if !interrupts_enabled() {
        return Err(RuntimeError::InterruptsDisabled);
    }
    let allocation = runtime_allocate(ARENA_BASE, 2)?;
    let payload = allocation.layout.payload();
    for index in 0..payload.count() {
        let page = payload
            .page(index)
            .ok_or(RuntimeError::Vm(Error::InvalidRange))?;
        // SAFETY: runtime allocation keeps this RW page mapped and exclusively
        // owned until release below; all volatile accesses finish beforehand.
        unsafe {
            check(page, None)?;
            fill(page, 0xa11c_0000 + index as u64);
        }
    }

    let before = crate::arch::x86_64::irq::timer_ticks();
    while crate::arch::x86_64::irq::timer_ticks() == before {
        // SAFETY: IF=1 and the already-proven PIT route wakes this BSP.
        unsafe { asm!("hlt", options(nomem, nostack)) };
    }

    for index in 0..payload.count() {
        let page = payload
            .page(index)
            .ok_or(RuntimeError::Vm(Error::InvalidRange))?;
        // SAFETY: mapping remained live across the timer interrupt.
        unsafe { check(page, Some(0xa11c_0000 + index as u64))? };
    }
    runtime_release(allocation)?;
    for index in 0..allocation.layout.payload().count() {
        let page = allocation
            .layout
            .payload()
            .page(index)
            .ok_or(RuntimeError::Vm(Error::InvalidRange))?;
        if runtime_query(page.address())?.is_some() {
            return Err(RuntimeError::Vm(Error::CorruptEntry));
        }
    }
    if runtime_query(allocation.layout.lower_guard().address())?.is_some()
        || runtime_query(allocation.layout.upper_guard().address())?.is_some()
    {
        return Err(RuntimeError::Vm(Error::CorruptEntry));
    }
    let (free, active) = with_runtime(|vm| Ok((vm.free_frames(), vm.active_allocations())))?;
    if free != RUNTIME_POOL_FRAMES || active != 0 {
        return Err(RuntimeError::Vm(Error::CorruptEntry));
    }

    crate::debugcon::write("VIBRIX: kernel managed VM runtime verified\r\n");
    crate::println!(
        "managed VM runtime: guarded mapping survived timer IRQ; {} retained pool frames free",
        free
    );
    Ok(())
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
        let (frames, reserved) = reserve_pool::<POOL_FRAMES>(bits)?;
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
