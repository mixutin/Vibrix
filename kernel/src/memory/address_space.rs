//! Feature-gated M5 proof for an independently owned userspace CR3.
//!
//! This is intentionally bounded to one BSP and one diagnostic address space.
//! Kernel mappings are shared read-only at the PML4 ownership boundary while
//! the managed user arena gets a fresh hierarchy rooted in a different CR3.
use super::virtual_memory::{SlotWindow, runtime};
use crate::BootInfo;
use core::arch::{asm, x86_64::__cpuid_count};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};
use vibrix_vmm::address::{
    ARENA_BASE, ARENA_SLOT, PAGE_BYTES, Page, Permissions, PhysicalFrame, Privilege,
};
use vibrix_vmm::frames::Frames;
use vibrix_vmm::walk::{ADDRESS_MASK, Memory, USER};
use vibrix_vmm::{Error, GuardedLayout, GuardedVm, Vm};

const ADDRESS_SPACE_POOL_FRAMES: usize = 32;
const ADDRESS_SPACE_GUARDED_SLOTS: usize = 4;
const ADDRESS_SPACE_SCRATCH_SLOT: usize = 510;
const ROOT_COPY_CHUNK: usize = 64;

type UserVm =
    GuardedVm<AddressSpaceMemory, ADDRESS_SPACE_POOL_FRAMES, ADDRESS_SPACE_GUARDED_SLOTS>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressSpaceError {
    AlreadyInitialized,
    NotInitialized,
    InterruptsEnabled,
    Scratch,
    InvalidRoot,
    OutOfFrames,
    Vm(Error),
}

impl From<Error> for AddressSpaceError {
    fn from(error: Error) -> Self {
        Self::Vm(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivatedProbe {
    pub kernel_root: u64,
    pub user_root: u64,
    pub user_rip: u64,
    pub user_rsp: u64,
}

struct AddressSpaceMemory {
    window: SlotWindow,
    root: u64,
    reserved: [u64; ADDRESS_SPACE_POOL_FRAMES],
}

impl AddressSpaceMemory {
    fn check_entry(&self, frame: u64, index: usize) {
        assert!(index < 512, "userspace VM entry index out of bounds");
        assert!(
            self.reserved.contains(&frame) || (frame == self.root && index == ARENA_SLOT),
            "userspace VM access outside owned hierarchy"
        );
    }
}

impl Memory for AddressSpaceMemory {
    fn read_entry(&mut self, frame: u64, index: usize) -> u64 {
        self.check_entry(frame, index);
        // SAFETY: slot 510 is exclusively owned by this backend; the frame is
        // either the private root slot or one of this VM's reserved frames.
        unsafe {
            let address = self
                .window
                .map(frame, false)
                .expect("userspace VM scratch map");
            let entry = (address as *const u64).add(index).read_volatile();
            self.window
                .unmap()
                .expect("userspace VM scratch unmap");
            entry
        }
    }

    fn write_entry(&mut self, frame: u64, index: usize, value: u64) {
        self.check_entry(frame, index);
        // SAFETY: same exclusive scratch mapping and owned page-table frame.
        unsafe {
            let address = self
                .window
                .map(frame, true)
                .expect("userspace VM scratch map");
            (address as *mut u64).add(index).write_volatile(value);
            self.window
                .unmap()
                .expect("userspace VM scratch unmap");
        }
    }

    fn zero_frame(&mut self, frame: u64) {
        assert!(
            self.reserved.contains(&frame),
            "cannot zero foreign userspace VM RAM"
        );
        // SAFETY: the frame is unpublished private RAM from this VM's pool.
        unsafe {
            let address = self
                .window
                .map(frame, true)
                .expect("userspace VM zero scratch");
            for index in 0..512 {
                (address as *mut u64).add(index).write_volatile(0);
            }
            self.window
                .unmap()
                .expect("userspace VM zero scratch unmap");
        }
    }

    fn invalidate(&mut self, address: u64) {
        assert!(Page::new(address).is_ok(), "foreign userspace VM invalidation");
        // SAFETY: one BSP only. Callers mask interrupts while this backend is
        // mutating the active or inactive private hierarchy.
        unsafe { asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)) };
    }
}

struct AddressSpace {
    vm: UserVm,
    kernel_root: u64,
    user_root: u64,
}

struct AddressSpaceCell(UnsafeCell<Option<AddressSpace>>);

// SAFETY: initialization is pre-STI on the sole BSP. Probe activation masks
// local interrupts before borrowing the cell. SMP is explicitly unsupported.
unsafe impl Sync for AddressSpaceCell {}

static ADDRESS_SPACE: AddressSpaceCell = AddressSpaceCell(UnsafeCell::new(None));
static ACTIVE_ROOT: AtomicU64 = AtomicU64::new(0);

fn interrupts_enabled() -> bool {
    let flags: u64;
    // SAFETY: read-only RFLAGS inspection at CPL0.
    unsafe { asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags)) };
    flags & (1 << 9) != 0
}

fn current_root() -> u64 {
    let root: u64;
    // SAFETY: privileged read at CPL0.
    unsafe { asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags)) };
    root & ADDRESS_MASK
}

unsafe fn switch_root(root: u64) {
    // SAFETY: caller has validated a complete PML4 physical frame and retained
    // the kernel mappings needed to execute across this CR3 switch.
    unsafe { asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags)) };
}

fn physical_bits_and_root() -> Result<(u8, u64), AddressSpaceError> {
    let root = current_root();
    let cr4: u64;
    // SAFETY: privileged CR4 read at CPL0.
    unsafe { asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags)) };
    // Keep the proof deliberately simple: no five-level paging, PCID or SMAP.
    if cr4 & ((1 << 12) | (1 << 17) | (1 << 21)) != 0 {
        return Err(AddressSpaceError::InvalidRoot);
    }
    let extended = __cpuid_count(0x8000_0000, 0).eax;
    let bits = if extended >= 0x8000_0008 {
        (__cpuid_count(0x8000_0008, 0).eax & 0xff) as u8
    } else {
        36
    };
    PhysicalFrame::new(root, bits).map_err(AddressSpaceError::Vm)?;
    Ok((bits, root))
}

unsafe fn claim_conventional(bits: u8) -> Result<u64, AddressSpaceError> {
    // SAFETY: init() is sole-BSP and pre-STI.
    let frame = unsafe { super::allocate_frame() }.ok_or(AddressSpaceError::OutOfFrames)?;
    PhysicalFrame::new(frame, bits).map_err(AddressSpaceError::Vm)?;
    // SAFETY: immutable retained UEFI map, same sole-BSP contract.
    let valid = unsafe { super::firmware_descriptor_at(frame) }
        .is_some_and(|(kind, attributes)| kind == 7 && attributes & 8 != 0 && attributes & (1 << 63) == 0);
    if !valid {
        return Err(AddressSpaceError::InvalidRoot);
    }
    Ok(frame)
}

unsafe fn reserve_pool(
    bits: u8,
) -> Result<
    (
        Frames<ADDRESS_SPACE_POOL_FRAMES>,
        [u64; ADDRESS_SPACE_POOL_FRAMES],
    ),
    AddressSpaceError,
> {
    let mut frames = Frames::<ADDRESS_SPACE_POOL_FRAMES>::new(bits)?;
    let mut reserved = [0; ADDRESS_SPACE_POOL_FRAMES];
    for slot in &mut reserved {
        // SAFETY: inherited pre-STI sole-BSP allocator ownership.
        let frame = unsafe { claim_conventional(bits) }?;
        frames.register(frame)?;
        *slot = frame;
    }
    Ok((frames, reserved))
}

unsafe fn copy_kernel_root(
    window: &mut SlotWindow,
    kernel_root: u64,
    user_root: u64,
) -> Result<(), AddressSpaceError> {
    // Start from an entirely empty new PML4.
    // SAFETY: user_root is newly claimed unpublished RAM.
    unsafe {
        let target = window
            .map(user_root, true)
            .map_err(|_| AddressSpaceError::Scratch)?;
        for index in 0..512 {
            (target as *mut u64).add(index).write_volatile(0);
        }
        window
            .unmap()
            .map_err(|_| AddressSpaceError::Scratch)?;
    }

    let mut chunk = [0u64; ROOT_COPY_CHUNK];
    for base in (0..512).step_by(ROOT_COPY_CHUNK) {
        // SAFETY: kernel_root is the currently active validated PML4. The
        // scratch mapping is read-only and is removed before target mapping.
        unsafe {
            let source = window
                .map(kernel_root, false)
                .map_err(|_| AddressSpaceError::Scratch)?;
            for (offset, word) in chunk.iter_mut().enumerate() {
                *word = (source as *const u64)
                    .add(base + offset)
                    .read_volatile();
            }
            window
                .unmap()
                .map_err(|_| AddressSpaceError::Scratch)?;
        }

        for (offset, entry) in chunk.iter().copied().enumerate() {
            let index = base + offset;
            if index != ARENA_SLOT && entry & USER != 0 {
                return Err(AddressSpaceError::InvalidRoot);
            }
        }

        // SAFETY: target root remains unpublished. Copy every kernel PML4
        // entry except the managed arena slot, which stays private and empty.
        unsafe {
            let target = window
                .map(user_root, true)
                .map_err(|_| AddressSpaceError::Scratch)?;
            for (offset, entry) in chunk.iter().copied().enumerate() {
                let index = base + offset;
                let value = if index == ARENA_SLOT { 0 } else { entry };
                (target as *mut u64).add(index).write_volatile(value);
            }
            window
                .unmap()
                .map_err(|_| AddressSpaceError::Scratch)?;
        }
    }
    Ok(())
}

/// Construct one private userspace root before interrupts are enabled.
///
/// # Safety
/// Sole BSP, IF=0, matched BootInfo v3, the physical allocator is initialized,
/// and scratch slot 510 has no other owner.
pub unsafe fn init(info: &BootInfo) -> Result<(), AddressSpaceError> {
    if interrupts_enabled() {
        return Err(AddressSpaceError::InterruptsEnabled);
    }
    // SAFETY: sole pre-STI owner.
    let state = unsafe { &mut *ADDRESS_SPACE.0.get() };
    if state.is_some() {
        return Err(AddressSpaceError::AlreadyInitialized);
    }

    let (bits, kernel_root) = physical_bits_and_root()?;
    // The inherited kernel root must itself be retained loader memory.
    // SAFETY: immutable final UEFI map.
    if !unsafe { super::firmware_descriptor_at(kernel_root) }
        .is_some_and(|(kind, attributes)| kind == 2 && attributes & 8 != 0 && attributes & (1 << 63) == 0)
    {
        return Err(AddressSpaceError::InvalidRoot);
    }

    // SAFETY: still pre-STI with exclusive early allocator ownership.
    let user_root = unsafe { claim_conventional(bits) }?;
    // SAFETY: same allocator contract.
    let (frames, reserved) = unsafe { reserve_pool(bits) }?;
    // SAFETY: APIC uses its documented low slots; managed runtime owns 511.
    let mut window = unsafe { runtime::slot_from_boot_info(info, ADDRESS_SPACE_SCRATCH_SLOT) }
        .map_err(|_| AddressSpaceError::Scratch)?;
    // SAFETY: both roots are validated, and user_root is not active/published.
    unsafe { copy_kernel_root(&mut window, kernel_root, user_root) }?;

    let backend = AddressSpaceMemory {
        window,
        root: user_root,
        reserved,
    };
    let vm = Vm::new(user_root, backend, frames)?;
    let vm = GuardedVm::new(vm)?;
    *state = Some(AddressSpace {
        vm,
        kernel_root,
        user_root,
    });
    Ok(())
}

/// Build guarded user code/stack, switch to the private CR3, stage code there,
/// and leave IF=0 for the immediate IRETQ into CPL3.
///
/// # Safety
/// Must run on the sole BSP after init(), with permanent GDT/TSS/IDT and kernel
/// mappings still live. On success the caller must immediately enter CPL3; the
/// diagnostic probe intentionally never returns.
pub unsafe fn activate_probe() -> Result<ActivatedProbe, AddressSpaceError> {
    const CODE_GUARD: u64 = ARENA_BASE;
    const STACK_GUARD: u64 = ARENA_BASE + 4 * PAGE_BYTES;
    const USER_CODE: [u8; 4] = [0xcd, 0x80, 0x0f, 0x0b];

    let restore_interrupts = interrupts_enabled();
    // SAFETY: serialize the one static address-space owner and CR3 mutation.
    unsafe { asm!("cli", options(nomem, nostack)) };

    // SAFETY: local interrupts are now masked and no AP exists.
    let state = unsafe { &mut *ADDRESS_SPACE.0.get() };
    let space = match state.as_mut() {
        Some(space) => space,
        None => {
            if restore_interrupts {
                // SAFETY: restore the caller's prior local IF state on failure.
                unsafe { asm!("sti", options(nomem, nostack)) };
            }
            return Err(AddressSpaceError::NotInitialized);
        }
    };

    let code_layout = GuardedLayout::new(Page::new(CODE_GUARD)?, 1)?;
    let stack_layout = GuardedLayout::new(Page::new(STACK_GUARD)?, 1)?;
    let code_id = space.vm.allocate_user(code_layout)?;
    let stack_id = match space.vm.allocate_user(stack_layout) {
        Ok(id) => id,
        Err(error) => {
            space
                .vm
                .release(code_id)
                .expect("userspace probe rollback owns code");
            if restore_interrupts {
                // SAFETY: no CR3 change occurred.
                unsafe { asm!("sti", options(nomem, nostack)) };
            }
            return Err(AddressSpaceError::Vm(error));
        }
    };

    let code_page = code_layout.payload().page(0).ok_or(Error::InvalidRange)?;
    let stack_page = stack_layout.payload().page(0).ok_or(Error::InvalidRange)?;
    let before_code = space.vm.query(code_page)?.ok_or(Error::NotMapped)?;
    let before_stack = space.vm.query(stack_page)?.ok_or(Error::NotMapped)?;
    if before_code.privilege != Privilege::User
        || before_code.permissions != Permissions::ReadWrite
        || before_stack.privilege != Privilege::User
        || before_stack.permissions != Permissions::ReadWrite
        || space.kernel_root == space.user_root
    {
        space
            .vm
            .release(stack_id)
            .expect("userspace probe rollback owns stack");
        space
            .vm
            .release(code_id)
            .expect("userspace probe rollback owns code");
        if restore_interrupts {
            // SAFETY: no CR3 change occurred.
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(AddressSpaceError::InvalidRoot);
    }

    ACTIVE_ROOT.store(space.user_root, Ordering::SeqCst);
    // SAFETY: the private root copied every supervisor kernel PML4 entry and
    // owns a fresh arena slot. IF is disabled around the transition.
    unsafe { switch_root(space.user_root) };
    if current_root() != space.user_root {
        ACTIVE_ROOT.store(0, Ordering::SeqCst);
        // SAFETY: restore known-good kernel root before returning.
        unsafe { switch_root(space.kernel_root) };
        if restore_interrupts {
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(AddressSpaceError::InvalidRoot);
    }

    // SAFETY: code_page is now reachable only through the active private root
    // as an exclusively owned user RW mapping. Volatile staging completes
    // before the PTE is changed to RX.
    unsafe {
        for (index, byte) in USER_CODE.iter().copied().enumerate() {
            (code_page.address() as *mut u8)
                .add(index)
                .write_volatile(byte);
        }
    }

    let protected = match space.vm.protect(code_id, 0, Permissions::ReadExecute) {
        Ok(previous) => previous,
        Err(error) => {
            ACTIVE_ROOT.store(0, Ordering::SeqCst);
            // SAFETY: switch back before manipulating/restoring normal state.
            unsafe { switch_root(space.kernel_root) };
            space
                .vm
                .release(stack_id)
                .expect("userspace probe rollback owns stack");
            space
                .vm
                .release(code_id)
                .expect("userspace probe rollback owns code");
            if restore_interrupts {
                unsafe { asm!("sti", options(nomem, nostack)) };
            }
            return Err(AddressSpaceError::Vm(error));
        }
    };
    let code = space.vm.query(code_page)?.ok_or(Error::NotMapped)?;
    if protected.privilege != Privilege::User
        || protected.permissions != Permissions::ReadWrite
        || code.privilege != Privilege::User
        || code.permissions != Permissions::ReadExecute
    {
        ACTIVE_ROOT.store(0, Ordering::SeqCst);
        // SAFETY: restore the known-good root on validation failure.
        unsafe { switch_root(space.kernel_root) };
        if restore_interrupts {
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(AddressSpaceError::InvalidRoot);
    }

    Ok(ActivatedProbe {
        kernel_root: space.kernel_root,
        user_root: space.user_root,
        user_rip: code_page.address(),
        user_rsp: stack_layout.payload_end(),
    })
}

/// Used by the CPL3 diagnostic interrupt handler to prove the CPU returned to
/// CPL0 without silently switching back to the kernel address-space root.
pub fn active_root_status() -> (u64, u64, bool) {
    let expected = ACTIVE_ROOT.load(Ordering::SeqCst);
    let current = current_root();
    (expected, current, expected != 0 && expected == current)
}
