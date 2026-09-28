//! Feature-gated M5 proof for an independently owned userspace CR3.
//!
//! This is intentionally bounded to one BSP and one diagnostic address space.
//! Kernel mappings are shared read-only at the PML4 ownership boundary while
//! the managed user arena gets a fresh hierarchy rooted in a different CR3.
use super::virtual_memory::{SlotWindow, runtime};
use crate::BootInfo;
use core::arch::{asm, global_asm, x86_64::__cpuid_count};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "elf-load-probe")]
use vibrix_kernel::user_image;
#[cfg(feature = "elf-load-probe")]
use vibrix_vmm::GuardedId;
use vibrix_vmm::address::{Page, Permissions, PhysicalFrame, Privilege, USER_SLOT};
use vibrix_vmm::frames::Frames;
use vibrix_vmm::walk::{ADDRESS_MASK, Memory, USER};
use vibrix_vmm::{Error, GuardedLayout, GuardedVm, Vm};

const ADDRESS_SPACE_POOL_FRAMES: usize = 32;
const ADDRESS_SPACE_GUARDED_SLOTS: usize = 4;
const ADDRESS_SPACE_SCRATCH_SLOT: usize = 510;
const ADDRESS_SPACE_DATA_SLOT: usize = 509;
const TRANSITION_STACK_BYTES: usize = 16 * 1024;
const ROOT_COPY_CHUNK: usize = 64;

type UserVm = GuardedVm<AddressSpaceMemory, ADDRESS_SPACE_POOL_FRAMES, ADDRESS_SPACE_GUARDED_SLOTS>;

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
            self.reserved.contains(&frame) || (frame == self.root && index == USER_SLOT),
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
            self.window.unmap().expect("userspace VM scratch unmap");
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
            self.window.unmap().expect("userspace VM scratch unmap");
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
        assert!(
            Page::new_user(address).is_ok(),
            "foreign userspace VM invalidation"
        );
        // SAFETY: one BSP only. Callers mask interrupts while this backend is
        // mutating the active or inactive private hierarchy.
        unsafe { asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)) };
    }
}

struct AddressSpace {
    vm: UserVm,
    staging: SlotWindow,
    kernel_root: u64,
    user_root: u64,
}

struct AddressSpaceCell(UnsafeCell<Option<AddressSpace>>);

// SAFETY: initialization is pre-STI on the sole BSP. Probe activation masks
// local interrupts before borrowing the cell. SMP is explicitly unsupported.
unsafe impl Sync for AddressSpaceCell {}

static ADDRESS_SPACE: AddressSpaceCell = AddressSpaceCell(UnsafeCell::new(None));
static ACTIVE_ROOT: AtomicU64 = AtomicU64::new(0);

#[repr(C, align(16))]
struct TransitionStack([u8; TRANSITION_STACK_BYTES]);

struct StaticTransitionStack(UnsafeCell<TransitionStack>);

// SAFETY: only the sole BSP uses this feature-gated transition stack, and no
// Rust reference to its bytes exists while hardware is using RSP within it.
unsafe impl Sync for StaticTransitionStack {}

static TRANSITION_STACK: StaticTransitionStack = StaticTransitionStack(UnsafeCell::new(
    TransitionStack([0; TRANSITION_STACK_BYTES]),
));
static mut ADDRESS_SPACE_KERNEL_RSP: u64 = 0;

global_asm!(
    r#"
    .global vibrix_address_space_enter
    vibrix_address_space_enter:
        cli
        mov rsp, qword ptr [rip + {kernel_rsp}]
        mov cr3, rdi
        mov rdi, rsi
        mov rsi, rdx
        call {entered}
        ud2
    "#,
    kernel_rsp = sym ADDRESS_SPACE_KERNEL_RSP,
    entered = sym address_space_entered,
);

unsafe extern "C" {
    fn vibrix_address_space_enter(user_root: u64, user_rip: u64, user_rsp: u64) -> !;
}

extern "C" fn address_space_entered(user_rip: u64, user_rsp: u64) -> ! {
    let expected = ACTIVE_ROOT.load(Ordering::SeqCst);
    let current = current_root();
    if expected == 0 || current != expected {
        panic!(
            "userspace CR3 transition mismatch: expected={:#x} current={:#x}",
            expected, current
        );
    }
    crate::debugcon::write("VIBRIX: kernel userspace CR3 activated\r\n");
    // SAFETY: enter_probe validated the live private root plus user RX/RW
    // mappings and this helper runs on a kernel-mapped transition stack.
    unsafe { crate::arch::x86_64::ring3::enter(user_rip, user_rsp) }
}

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
    // SAFETY: caller supplies one of the already-validated PML4 roots owned by
    // this single-BSP address-space proof and keeps IF masked across the switch.
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
    let valid =
        unsafe { super::firmware_descriptor_at(frame) }.is_some_and(|(kind, attributes)| {
            kind == 7 && attributes & 8 != 0 && attributes & (1 << 63) == 0
        });
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
        window.unmap().map_err(|_| AddressSpaceError::Scratch)?;
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
                *word = (source as *const u64).add(base + offset).read_volatile();
            }
            window.unmap().map_err(|_| AddressSpaceError::Scratch)?;
        }

        for (offset, entry) in chunk.iter().copied().enumerate() {
            let index = base + offset;
            if index != USER_SLOT && entry & USER != 0 {
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
                let value = if index == USER_SLOT { 0 } else { entry };
                (target as *mut u64).add(index).write_volatile(value);
            }
            window.unmap().map_err(|_| AddressSpaceError::Scratch)?;
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
    if !unsafe { super::firmware_descriptor_at(kernel_root) }.is_some_and(|(kind, attributes)| {
        kind == 2 && attributes & 8 != 0 && attributes & (1 << 63) == 0
    }) {
        return Err(AddressSpaceError::InvalidRoot);
    }

    // SAFETY: still pre-STI with exclusive early allocator ownership.
    let user_root = unsafe { claim_conventional(bits) }?;
    // SAFETY: same allocator contract.
    let (frames, reserved) = unsafe { reserve_pool(bits) }?;
    // SAFETY: APIC uses its documented low slots; managed runtime owns 511.
    // Slot 510 owns page-table scratch while slot 509 stages private data
    // frames without activating the userspace CR3.
    let mut window = unsafe { runtime::slot_from_boot_info(info, ADDRESS_SPACE_SCRATCH_SLOT) }
        .map_err(|_| AddressSpaceError::Scratch)?;
    let staging = unsafe { runtime::slot_from_boot_info(info, ADDRESS_SPACE_DATA_SLOT) }
        .map_err(|_| AddressSpaceError::Scratch)?;
    // SAFETY: both roots are validated, and user_root is not active/published.
    unsafe { copy_kernel_root(&mut window, kernel_root, user_root) }?;

    let backend = AddressSpaceMemory {
        window,
        root: user_root,
        reserved,
    };
    let vm = Vm::new_in_slot(user_root, backend, frames, USER_SLOT)?;
    let vm = GuardedVm::new(vm)?;

    let transition_top = TRANSITION_STACK.0.get() as u64 + TRANSITION_STACK_BYTES as u64;
    if transition_top == 0 || transition_top & 0xf != 0 {
        return Err(AddressSpaceError::InvalidRoot);
    }
    // SAFETY: initialization is sole-BSP and pre-STI. No private-root entry can
    // occur until ADDRESS_SPACE is published below.
    unsafe {
        core::ptr::addr_of_mut!(ADDRESS_SPACE_KERNEL_RSP).write(transition_top);
    }

    *state = Some(AddressSpace {
        vm,
        staging,
        kernel_root,
        user_root,
    });
    Ok(())
}

/// Build guarded user code/stack under the inactive private root and stage
/// code through a kernel-root scratch mapping. No CR3 switch occurs here.
///
/// # Safety
/// Must run on the sole BSP after init(), with permanent GDT/TSS/IDT and kernel
/// mappings still live. The returned probe must be consumed by enter_probe().
#[cfg(not(feature = "elf-load-probe"))]
pub unsafe fn activate_probe() -> Result<ActivatedProbe, AddressSpaceError> {
    const CODE_GUARD: u64 = 0x003f_f000;
    const STACK_GUARD: u64 = 0x007f_e000;
    #[cfg(feature = "process-syscall-probe")]
    const USER_CODE: &[u8] = &[
        0xb8, 0x02, 0x00, 0x00, 0x00, // mov eax, 2 (getpid)
        0x0f, 0x05, // syscall
        0x48, 0x83, 0xf8, 0x01, // cmp rax, 1
        0x74, 0x02, // je next
        0x0f, 0x0b, // ud2
        0xbf, 0x02, 0x00, 0x00, 0x00, // mov edi, 2 (child pid)
        0x48, 0x8d, 0x74, 0x24, 0xf8, // lea rsi, [rsp - 8] (status)
        0x31, 0xd2, // xor edx, edx (options)
        0xb8, 0x07, 0x00, 0x00, 0x00, // mov eax, 7 (wait)
        0x0f, 0x05, // syscall
        0x48, 0x83, 0xf8, 0x02, // cmp rax, 2
        0x74, 0x02, // je next
        0x0f, 0x0b, // ud2
        0x83, 0x7c, 0x24, 0xf8, 0x17, // cmp dword ptr [rsp - 8], 23
        0x74, 0x02, // je next
        0x0f, 0x0b, // ud2
        0xbf, 0x2a, 0x00, 0x00, 0x00, // mov edi, 42 (exit status)
        0xb8, 0x00, 0x00, 0x00, 0x00, // mov eax, 0 (exit)
        0x0f, 0x05, // syscall; must not return
        0x0f, 0x0b, // ud2
    ];
    #[cfg(all(not(feature = "process-syscall-probe"), not(feature = "syscall-probe")))]
    const USER_CODE: &[u8] = &[0xcd, 0x80, 0x0f, 0x0b];
    #[cfg(all(not(feature = "process-syscall-probe"), feature = "syscall-probe"))]
    const USER_CODE: &[u8] = &[
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // mov rax, -1
        0x0f, 0x05, // syscall
        0x48, 0x83, 0xf8, 0xfb, // cmp rax, -5 (NotSupported)
        0x75, 0x02, // jne trailing ud2
        0xcd, 0x80, // diagnostic DPL3 trap after SYSRETQ
        0x0f, 0x0b, // ud2: failure/fallthrough stop
    ];

    let restore_interrupts = interrupts_enabled();
    // SAFETY: serialize the one static address-space owner and scratch slots.
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

    let code_layout = GuardedLayout::new(Page::new_user(CODE_GUARD)?, 1)?;
    let stack_layout = GuardedLayout::new(Page::new_user(STACK_GUARD)?, 1)?;
    let code_id = space.vm.allocate_user(code_layout)?;
    let stack_id = match space.vm.allocate_user(stack_layout) {
        Ok(id) => id,
        Err(error) => {
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
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(AddressSpaceError::InvalidRoot);
    }

    // Stage bytes while the kernel CR3 and its low boot stack are still active.
    // The private data frame is reached only through scratch slot 509.
    let staged = unsafe { space.staging.map(before_code.physical, true) }
        .map_err(|_| AddressSpaceError::Scratch)?;
    unsafe {
        for (index, byte) in USER_CODE.iter().copied().enumerate() {
            (staged as *mut u8).add(index).write_volatile(byte);
        }
        space
            .staging
            .unmap()
            .map_err(|_| AddressSpaceError::Scratch)?;
    }

    let protected = match space.vm.protect(code_id, 0, Permissions::ReadExecute) {
        Ok(previous) => previous,
        Err(error) => {
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
        if restore_interrupts {
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(AddressSpaceError::InvalidRoot);
    }

    if restore_interrupts {
        // No CR3 transition occurred here; restore the caller's original state.
        unsafe { asm!("sti", options(nomem, nostack)) };
    }

    Ok(ActivatedProbe {
        kernel_root: space.kernel_root,
        user_root: space.user_root,
        user_rip: code_page.address(),
        user_rsp: stack_layout.payload_end(),
    })
}

#[cfg(feature = "elf-load-probe")]
const ELF_SEGMENT_SLOTS: usize = 3;

#[cfg(feature = "elf-load-probe")]
#[derive(Clone, Copy)]
struct ImageAllocation {
    id: GuardedId,
    start: u64,
    bytes: usize,
    pages: usize,
    target: Permissions,
}

#[cfg(feature = "elf-load-probe")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSinkError {
    State,
    Layout,
    Range,
    Scratch,
    Vm(Error),
}

#[cfg(feature = "elf-load-probe")]
impl From<Error> for ImageSinkError {
    fn from(error: Error) -> Self {
        Self::Vm(error)
    }
}

#[cfg(feature = "elf-load-probe")]
#[derive(Debug, PartialEq, Eq)]
pub enum ElfProbeError {
    AddressSpace(AddressSpaceError),
    Image(user_image::Error),
    Sink(ImageSinkError),
}

#[cfg(feature = "elf-load-probe")]
struct ImageSink<'a> {
    space: &'a mut AddressSpace,
    allocations: [Option<ImageAllocation>; ELF_SEGMENT_SLOTS],
    count: usize,
    begun: bool,
    committed: bool,
}

#[cfg(feature = "elf-load-probe")]
impl<'a> ImageSink<'a> {
    fn new(space: &'a mut AddressSpace) -> Self {
        Self {
            space,
            allocations: [None; ELF_SEGMENT_SLOTS],
            count: 0,
            begun: false,
            committed: false,
        }
    }

    fn allocation_for(
        &self,
        address: u64,
        bytes: usize,
    ) -> Result<ImageAllocation, ImageSinkError> {
        let end = address
            .checked_add(bytes as u64)
            .ok_or(ImageSinkError::Range)?;
        self.allocations[..self.count]
            .iter()
            .flatten()
            .copied()
            .find(|allocation| {
                let allocation_end = allocation
                    .start
                    .checked_add(allocation.bytes as u64)
                    .unwrap_or(0);
                address >= allocation.start && end <= allocation_end
            })
            .ok_or(ImageSinkError::Range)
    }

    fn stage(&mut self, address: u64, bytes: &[u8]) -> Result<(), ImageSinkError> {
        self.allocation_for(address, bytes.len())?;
        let mut copied = 0usize;
        while copied < bytes.len() {
            let current = address
                .checked_add(copied as u64)
                .ok_or(ImageSinkError::Range)?;
            let page_address = current & !0xfff;
            let offset = (current - page_address) as usize;
            let count = (4096 - offset).min(bytes.len() - copied);
            let page = Page::new_user(page_address).map_err(ImageSinkError::Vm)?;
            let translation = self
                .space
                .vm
                .query(page)
                .map_err(ImageSinkError::Vm)?
                .ok_or(ImageSinkError::Range)?;
            if translation.privilege != Privilege::User
                || translation.permissions != Permissions::ReadWrite
            {
                return Err(ImageSinkError::State);
            }
            let scratch = unsafe {
                self.space
                    .staging
                    .map(translation.physical, true)
                    .map_err(|_| ImageSinkError::Scratch)?
            };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    bytes[copied..copied + count].as_ptr(),
                    (scratch as *mut u8).add(offset),
                    count,
                );
                self.space
                    .staging
                    .unmap()
                    .map_err(|_| ImageSinkError::Scratch)?;
            }
            copied += count;
        }
        Ok(())
    }

    fn zero_bytes(&mut self, address: u64, bytes: usize) -> Result<(), ImageSinkError> {
        self.allocation_for(address, bytes)?;
        let mut done = 0usize;
        while done < bytes {
            let current = address
                .checked_add(done as u64)
                .ok_or(ImageSinkError::Range)?;
            let page_address = current & !0xfff;
            let offset = (current - page_address) as usize;
            let count = (4096 - offset).min(bytes - done);
            let page = Page::new_user(page_address).map_err(ImageSinkError::Vm)?;
            let translation = self
                .space
                .vm
                .query(page)
                .map_err(ImageSinkError::Vm)?
                .ok_or(ImageSinkError::Range)?;
            if translation.privilege != Privilege::User
                || translation.permissions != Permissions::ReadWrite
            {
                return Err(ImageSinkError::State);
            }
            let scratch = unsafe {
                self.space
                    .staging
                    .map(translation.physical, true)
                    .map_err(|_| ImageSinkError::Scratch)?
            };
            unsafe {
                core::ptr::write_bytes((scratch as *mut u8).add(offset), 0, count);
                self.space
                    .staging
                    .unmap()
                    .map_err(|_| ImageSinkError::Scratch)?;
            }
            done += count;
        }
        Ok(())
    }

    fn release_all(&mut self) {
        while self.count != 0 {
            self.count -= 1;
            if let Some(allocation) = self.allocations[self.count].take() {
                let _ = self.space.vm.release(allocation.id);
            }
        }
    }
}

#[cfg(feature = "elf-load-probe")]
impl user_image::Sink for ImageSink<'_> {
    type Error = ImageSinkError;

    fn begin(&mut self) -> Result<(), Self::Error> {
        if self.begun || self.committed {
            return Err(ImageSinkError::State);
        }
        self.begun = true;
        Ok(())
    }

    fn map(
        &mut self,
        virtual_address: u64,
        memory_size: usize,
        permissions: user_image::Permissions,
    ) -> Result<(), Self::Error> {
        if !self.begun || self.committed || self.count == ELF_SEGMENT_SLOTS || memory_size == 0 {
            return Err(ImageSinkError::State);
        }
        if virtual_address & 0xfff != 0 {
            return Err(ImageSinkError::Layout);
        }
        let pages = memory_size.checked_add(4095).ok_or(ImageSinkError::Range)? / 4096;
        let lower = virtual_address
            .checked_sub(4096)
            .ok_or(ImageSinkError::Layout)?;
        let layout = GuardedLayout::new(Page::new_user(lower).map_err(ImageSinkError::Vm)?, pages)
            .map_err(ImageSinkError::Vm)?;
        let id = self
            .space
            .vm
            .allocate_user(layout)
            .map_err(ImageSinkError::Vm)?;
        let target = if permissions.write {
            Permissions::ReadWrite
        } else if permissions.execute {
            Permissions::ReadExecute
        } else {
            Permissions::ReadOnly
        };
        self.allocations[self.count] = Some(ImageAllocation {
            id,
            start: virtual_address,
            bytes: memory_size,
            pages,
            target,
        });
        self.count += 1;
        Ok(())
    }

    fn write(&mut self, virtual_address: u64, bytes: &[u8]) -> Result<(), Self::Error> {
        if !self.begun || self.committed {
            return Err(ImageSinkError::State);
        }
        self.stage(virtual_address, bytes)
    }

    fn zero(&mut self, virtual_address: u64, bytes: usize) -> Result<(), Self::Error> {
        if !self.begun || self.committed {
            return Err(ImageSinkError::State);
        }
        self.zero_bytes(virtual_address, bytes)
    }

    fn commit(&mut self, entry: u64) -> Result<(), Self::Error> {
        if !self.begun || self.committed || self.count == 0 {
            return Err(ImageSinkError::State);
        }
        for allocation in self.allocations[..self.count].iter().flatten().copied() {
            for index in 0..allocation.pages {
                if allocation.target != Permissions::ReadWrite {
                    self.space
                        .vm
                        .protect(allocation.id, index, allocation.target)
                        .map_err(ImageSinkError::Vm)?;
                }
            }
        }
        let page = Page::new_user(entry & !0xfff).map_err(ImageSinkError::Vm)?;
        let translation = self
            .space
            .vm
            .query(page)
            .map_err(ImageSinkError::Vm)?
            .ok_or(ImageSinkError::Range)?;
        if translation.privilege != Privilege::User || !translation.permissions.executable() {
            return Err(ImageSinkError::State);
        }
        self.committed = true;
        Ok(())
    }

    fn abort(&mut self) {
        self.release_all();
        self.begun = false;
        self.committed = false;
    }
}

#[cfg(feature = "elf-load-probe")]
fn elf_probe_image() -> [u8; 124] {
    const ELF_HEADER: usize = 64;
    const PROGRAM_HEADER: usize = 56;
    const PAYLOAD: usize = ELF_HEADER + PROGRAM_HEADER;
    const VADDR: u64 = 0x0040_0000;

    let mut data = [0u8; 124];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    data[16..18].copy_from_slice(&2u16.to_le_bytes());
    data[18..20].copy_from_slice(&62u16.to_le_bytes());
    data[20..24].copy_from_slice(&1u32.to_le_bytes());
    data[24..32].copy_from_slice(&VADDR.to_le_bytes());
    data[32..40].copy_from_slice(&(ELF_HEADER as u64).to_le_bytes());
    data[52..54].copy_from_slice(&(ELF_HEADER as u16).to_le_bytes());
    data[54..56].copy_from_slice(&(PROGRAM_HEADER as u16).to_le_bytes());
    data[56..58].copy_from_slice(&1u16.to_le_bytes());

    let ph = ELF_HEADER;
    data[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());
    data[ph + 4..ph + 8].copy_from_slice(&1u32.to_le_bytes());
    data[ph + 8..ph + 16].copy_from_slice(&(PAYLOAD as u64).to_le_bytes());
    data[ph + 16..ph + 24].copy_from_slice(&VADDR.to_le_bytes());
    data[ph + 32..ph + 40].copy_from_slice(&4u64.to_le_bytes());
    data[ph + 40..ph + 48].copy_from_slice(&4096u64.to_le_bytes());
    data[ph + 48..ph + 56].copy_from_slice(&1u64.to_le_bytes());
    data[PAYLOAD..PAYLOAD + 4].copy_from_slice(&[0xcd, 0x80, 0x0f, 0x0b]);
    data
}

#[cfg(feature = "elf-load-probe")]
pub unsafe fn load_elf_probe() -> Result<ActivatedProbe, ElfProbeError> {
    const STACK_GUARD: u64 = 0x007f_e000;

    let restore_interrupts = interrupts_enabled();
    unsafe { asm!("cli", options(nomem, nostack)) };

    let state = unsafe { &mut *ADDRESS_SPACE.0.get() };
    let Some(space) = state.as_mut() else {
        if restore_interrupts {
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(ElfProbeError::AddressSpace(
            AddressSpaceError::NotInitialized,
        ));
    };

    let stack_layout = GuardedLayout::new(
        Page::new_user(STACK_GUARD).map_err(|e| ElfProbeError::AddressSpace(e.into()))?,
        1,
    )
    .map_err(|e| ElfProbeError::AddressSpace(e.into()))?;
    let stack_id = space
        .vm
        .allocate_user(stack_layout)
        .map_err(|e| ElfProbeError::AddressSpace(e.into()))?;

    let image = elf_probe_image();
    let loaded_result = {
        let mut sink = ImageSink::new(space);
        user_image::load(&image, &mut sink)
    };
    let loaded = match loaded_result {
        Ok(loaded) => loaded,
        Err(user_image::LoadError::Image(error)) => {
            let _ = space.vm.release(stack_id);
            if restore_interrupts {
                unsafe { asm!("sti", options(nomem, nostack)) };
            }
            return Err(ElfProbeError::Image(error));
        }
        Err(user_image::LoadError::Sink(error)) => {
            let _ = space.vm.release(stack_id);
            if restore_interrupts {
                unsafe { asm!("sti", options(nomem, nostack)) };
            }
            return Err(ElfProbeError::Sink(error));
        }
    };

    let stack_page = stack_layout
        .payload()
        .page(0)
        .ok_or(ElfProbeError::AddressSpace(AddressSpaceError::Vm(
            Error::InvalidRange,
        )))?;
    let stack = space
        .vm
        .query(stack_page)
        .map_err(|e| ElfProbeError::AddressSpace(e.into()))?
        .ok_or(ElfProbeError::AddressSpace(AddressSpaceError::Vm(
            Error::NotMapped,
        )))?;
    let entry_page =
        Page::new_user(loaded.entry & !0xfff).map_err(|e| ElfProbeError::AddressSpace(e.into()))?;
    let entry = space
        .vm
        .query(entry_page)
        .map_err(|e| ElfProbeError::AddressSpace(e.into()))?
        .ok_or(ElfProbeError::AddressSpace(AddressSpaceError::Vm(
            Error::NotMapped,
        )))?;
    if stack.privilege != Privilege::User
        || stack.permissions != Permissions::ReadWrite
        || entry.privilege != Privilege::User
        || entry.permissions != Permissions::ReadExecute
    {
        if restore_interrupts {
            unsafe { asm!("sti", options(nomem, nostack)) };
        }
        return Err(ElfProbeError::AddressSpace(AddressSpaceError::InvalidRoot));
    }

    if restore_interrupts {
        unsafe { asm!("sti", options(nomem, nostack)) };
    }

    Ok(ActivatedProbe {
        kernel_root: space.kernel_root,
        user_root: space.user_root,
        user_rip: loaded.entry,
        user_rsp: stack_layout.payload_end(),
    })
}

/// Switch first to a permanent higher-half kernel transition stack, then load
/// the private CR3 and immediately enter CPL3. This function never returns.
///
/// # Safety
/// probe must come from activate_probe() for the current initialized owner.
/// GDT/TSS/IDT and the copied supervisor kernel mappings must remain live.
pub unsafe fn enter_probe(probe: ActivatedProbe) -> ! {
    ACTIVE_ROOT.store(probe.user_root, Ordering::SeqCst);
    // SAFETY: assembly changes RSP before CR3, so clearing lower slot zero cannot
    // unmap the live kernel stack. The called helper validates the new CR3.
    unsafe { vibrix_address_space_enter(probe.user_root, probe.user_rip, probe.user_rsp) }
}

#[cfg(feature = "process-syscall-probe")]
pub fn copy_to_user(address: u64, bytes: &[u8]) -> Result<(), AddressSpaceError> {
    if bytes.is_empty() {
        return Ok(());
    }
    let final_address = address
        .checked_add(bytes.len() as u64 - 1)
        .ok_or(AddressSpaceError::InvalidRoot)?;
    Page::new_user(address & !0xfff)?;
    Page::new_user(final_address & !0xfff)?;
    if interrupts_enabled() {
        return Err(AddressSpaceError::InterruptsEnabled);
    }

    // SAFETY: process syscalls run on the sole BSP with IF masked by FMASK.
    let state = unsafe { &mut *ADDRESS_SPACE.0.get() };
    let space = state.as_mut().ok_or(AddressSpaceError::NotInitialized)?;
    let user_root = ACTIVE_ROOT.load(Ordering::SeqCst);
    if user_root == 0 || user_root != space.user_root || current_root() != user_root {
        return Err(AddressSpaceError::InvalidRoot);
    }

    unsafe { switch_root(space.kernel_root) };
    let result = (|| {
        // First validate every page so a failed copy cannot partially mutate
        // userspace and a wait status is never half-written before reap.
        let mut cursor = address;
        loop {
            let page = Page::new_user(cursor & !0xfff)?;
            let mapping = space.vm.query(page)?.ok_or(Error::NotMapped)?;
            if mapping.privilege != Privilege::User || mapping.permissions != Permissions::ReadWrite
            {
                return Err(AddressSpaceError::InvalidRoot);
            }
            if (cursor & !0xfff) == (final_address & !0xfff) {
                break;
            }
            cursor = (cursor & !0xfff)
                .checked_add(4096)
                .ok_or(AddressSpaceError::InvalidRoot)?;
        }

        let mut copied = 0usize;
        while copied < bytes.len() {
            let current = address
                .checked_add(copied as u64)
                .ok_or(AddressSpaceError::InvalidRoot)?;
            let page_address = current & !0xfff;
            let offset = (current - page_address) as usize;
            let count = core::cmp::min(4096 - offset, bytes.len() - copied);
            let page = Page::new_user(page_address)?;
            let mapping = space.vm.query(page)?.ok_or(Error::NotMapped)?;
            let staged = unsafe { space.staging.map(mapping.physical, true) }
                .map_err(|_| AddressSpaceError::Scratch)?;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    bytes[copied..copied + count].as_ptr(),
                    (staged as *mut u8).add(offset),
                    count,
                );
                space
                    .staging
                    .unmap()
                    .map_err(|_| AddressSpaceError::Scratch)?;
            }
            copied += count;
        }
        Ok(())
    })();
    unsafe { switch_root(user_root) };
    result
}

/// Used by the CPL3 diagnostic interrupt handler to prove the CPU returned to
/// CPL0 without silently switching back to the kernel address-space root.
pub fn active_root_status() -> (u64, u64, bool) {
    let expected = ACTIVE_ROOT.load(Ordering::SeqCst);
    let current = current_root();
    (expected, current, expected != 0 && expected == current)
}
