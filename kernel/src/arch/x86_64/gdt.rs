//! x86-64 Global Descriptor Table (GDT) and Task State Segment (TSS).
//!
//! Clean-room implementation from the Intel 64 and IA-32 Architectures
//! Software Developer's Manual, Volume 3, Chapter 7 (Task Management).
//!
//! In 64-bit mode, segmentation is mostly disabled but the GDT is still
//! required for:
//! - Kernel code/data segment selectors (CS, DS, SS)
//! - The Task State Segment (TSS) descriptor
//! - The GS base (per-CPU pointer, set later)

#![allow(dead_code)]

use core::{arch::asm, cell::UnsafeCell};

const RING0_STACK_BYTES: usize = 16 * 1024;

#[repr(C, align(16))]
struct Ring0Stack([u8; RING0_STACK_BYTES]);

/// A single 8-byte GDT entry (segment descriptor).
///
/// In 64-bit mode, the descriptor format is simplified:
/// - Base and limit are ignored for code/data segments (flat model)
/// - Only the type, S, DPL, P, L, and D/B bits matter
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    limit_high_flags: u8,
    base_high: u8,
}

impl GdtEntry {
    /// Create a null descriptor (required first entry).
    pub const NULL: Self = Self::new(0, 0, 0, 0, 0);

    /// Create a new GDT entry.
    ///
    /// # Arguments
    /// * `base` — segment base address (ignored in 64-bit flat mode)
    /// * `limit` — segment limit (ignored in 64-bit flat mode)
    /// * `access` — access byte (type, S, DPL, P bits)
    /// * `flags` — flags nibble (L, D/B, G bits) in high nibble, limit high in low nibble
    pub const fn new(base: u32, limit: u32, access: u8, flags: u8, base_high: u8) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access,
            limit_high_flags: (((limit >> 16) & 0x0F) | ((flags as u32) & 0xF0)) as u8,
            base_high,
        }
    }

    /// Create a 64-bit kernel code segment descriptor.
    ///
    /// Access byte: 0x9A = present, ring 0, code, executable, readable
    /// Flags: 0xA0 = 64-bit (L), granularity 4K (G)
    pub const KERNEL_CODE: Self = Self::new(
        0,       // base (ignored in 64-bit)
        0xFFFFF, // limit (max, for 4K granularity)
        0x9A,    // present | ring 0 | code | executable | readable
        0xA0,    // L=1 (64-bit), G=1 (4K granularity)
        0,       // base_high
    );

    /// Create a 64-bit kernel data segment descriptor.
    ///
    /// Access byte: 0x92 = present, ring 0, data, writable
    /// Flags: 0xC0 = D/B=1 (32-bit data), G=1 (4K granularity)
    pub const KERNEL_DATA: Self = Self::new(
        0,       // base (ignored in 64-bit)
        0xFFFFF, // limit (max, for 4K granularity)
        0x92,    // present | ring 0 | data | writable
        0xC0,    // D/B=1, G=1 (4K granularity)
        0,       // base_high
    );

    /// Create a 64-bit user code segment descriptor.
    ///
    /// Access byte: 0xFA = present, ring 3, code, executable, readable
    /// Flags: 0xA0 = L=1 (64-bit), G=1 (4K granularity)
    pub const USER_CODE: Self = Self::new(
        0, 0xFFFFF, 0xFA, // present | ring 3 | code | executable | readable
        0xA0, // L=1, G=1
        0,
    );

    /// Create a 64-bit user data segment descriptor.
    ///
    /// Access byte: 0xF2 = present, ring 3, data, writable
    /// Flags: 0xC0 = D/B=1, G=1
    pub const USER_DATA: Self = Self::new(
        0, 0xFFFFF, 0xF2, // present | ring 3 | data | writable
        0xC0, // D/B=1, G=1
        0,
    );
}

/// A 16-byte TSS descriptor (GDT entry for the Task State Segment).
///
/// In 64-bit mode, the TSS descriptor is 16 bytes (not 8 like other
/// descriptors) because the base address is 64 bits.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TssDescriptor {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    limit_high_flags: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssDescriptor {
    /// Create a TSS descriptor for a given TSS base address.
    pub const fn new(base: u64, limit: u32) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access: 0x89, // present | ring 0 | available 64-bit TSS
            limit_high_flags: ((limit >> 16) & 0x0F) as u8,
            base_high: ((base >> 24) & 0xFF) as u8,
            base_upper: ((base >> 32) & 0xFFFF_FFFF) as u32,
            reserved: 0,
        }
    }
}

/// The x86-64 Task State Segment (104 bytes).
///
/// In 64-bit mode, the TSS contains:
/// - RSP0, RSP1, RSP2: stack pointers for privilege level changes
/// - IST1-IST7: interrupt stack table pointers
/// - I/O map base address (mostly unused in 64-bit)
///
/// The TSS is used by the hardware for stack switching on privilege
/// transitions (e.g., ring 3 → ring 0 on interrupt/syscall).
///
/// The layout must match the hardware exactly — RSP0 is at offset 0x04,
/// not 0x08. We use `packed` to prevent the compiler from inserting
/// alignment padding between `reserved0` and `rsp0`.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Tss {
    reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    reserved1: u64,
    pub ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    pub io_map_base: u16,
}

impl Tss {
    /// Create a TSS with the I/O bitmap disabled. `init` installs the permanent
    /// single-BSP RSP0 stack before loading TR; IST slots remain unconfigured.
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            // Offset beyond the inclusive TSS limit: no I/O permission bitmap.
            io_map_base: core::mem::size_of::<Self>() as u16,
        }
    }
}

/// The Global Descriptor Table.
///
/// Contains up to 8192 entries (16-bit selector limit), but we only
/// need a handful: null, kernel code, kernel data, user code, user data,
/// and the TSS descriptor.
#[repr(C, align(8))]
pub struct Gdt {
    entries: [GdtEntry; 5],
    tss_descriptor: TssDescriptor,
}

impl Gdt {
    /// A placeholder for the permanent GDT storage; init() fills its TSS base.
    const EMPTY: Self = Self {
        entries: [GdtEntry::NULL; 5],
        tss_descriptor: TssDescriptor::new(0, (core::mem::size_of::<Tss>() - 1) as u32),
    };

    /// Create a new GDT with the standard Vibrix layout.
    ///
    /// Layout:
    /// - 0x00: null descriptor (required)
    /// - 0x08: kernel code
    /// - 0x10: kernel data
    /// - 0x18: user code
    /// - 0x20: user data
    /// - 0x28: TSS descriptor (16 bytes, spans 0x28-0x37)
    pub fn new(tss: &Tss) -> Self {
        Self {
            entries: [
                GdtEntry::NULL,
                GdtEntry::KERNEL_CODE,
                GdtEntry::KERNEL_DATA,
                GdtEntry::USER_CODE,
                GdtEntry::USER_DATA,
            ],
            tss_descriptor: TssDescriptor::new(
                tss as *const _ as u64,
                (core::mem::size_of::<Tss>() - 1) as u32,
            ),
        }
    }

    /// Get the GDT pointer for the LGDT instruction.
    pub fn pointer(&self) -> GdtPointer {
        GdtPointer {
            limit: (core::mem::size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        }
    }

    /// Get the kernel code segment selector.
    pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
    /// Get the kernel data segment selector.
    pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
    /// Get the user code segment selector.
    pub const USER_CODE_SELECTOR: u16 = 0x1b; // descriptor 0x18 | ring-3 RPL
    /// Get the user data segment selector.
    pub const USER_DATA_SELECTOR: u16 = 0x23; // descriptor 0x20 | ring-3 RPL
    /// Get the TSS segment selector (the offset of the actual 16-byte descriptor).
    pub const TSS_SELECTOR: u16 = core::mem::offset_of!(Self, tss_descriptor) as u16;
}

/// GDTR structure (loaded by LGDT).
#[repr(C, packed)]
pub struct GdtPointer {
    limit: u16,
    base: u64,
}

/// Load the GDT and reload segment selectors.
///
/// # Safety
///
/// `gdt` must point to permanently allocated, mapped, writable, valid GDT
/// storage; descriptor loads may set Accessed bits and LTR sets the TSS busy
/// bit in this memory. Do not keep an immutable Rust reference to the GDT
/// across hardware descriptor accesses. Call only once on the boot CPU with
/// interrupts disabled, before other CPUs and privilege transitions.
pub unsafe fn load_gdt(gdt: *mut Gdt) {
    let ptr = GdtPointer {
        limit: (core::mem::size_of::<Gdt>() - 1) as u16,
        base: gdt as u64,
    };

    unsafe {
        // Load the GDT
        asm!("lgdt [{}]", in(reg) &ptr, options(nostack));

        // Reload segment selectors
        // CS is reloaded via a far return (or far jump)
        asm!(
            // Reload DS, ES, FS, GS, SS
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            in("ax") Gdt::KERNEL_DATA_SELECTOR,
            options(nostack)
        );

        // Reload CS via far return
        asm!(
            "push {selector}",
            "lea {addr}, [rip + 2f]",
            "push {addr}",
            "retfq",
            "2:",
            selector = in(reg) Gdt::KERNEL_CODE_SELECTOR as u64,
            addr = out(reg) _,
            // push/push/retfq uses the stack and restores RSP to its original value.
            // Do NOT declare nostack; the output scratch register is overwritten.
        );
    }
}

/// Load the Task Register (TR) with the TSS selector.
///
/// # Safety
///
/// The TSS and GDT must remain permanently mapped. LTR writes the TSS busy
/// bit into the writable GDT descriptor; caller must have initialized the
/// descriptor's base/limit and disabled interrupts/other CPUs before loading.
pub unsafe fn load_tss() {
    unsafe {
        asm!("ltr ax", in("ax") Gdt::TSS_SELECTOR, options(nostack));
    }
}

/// Kernel-lifetime storage for the TSS and GDT, independent of init()'s stack.
/// UnsafeCell is needed for one-time initialization; after LGDT/LTR these
/// objects may not move or be mutated by uncoordinated software. The CPU may
/// set descriptor Accessed/busy bits in GDT storage, so it stays writable.
/// This is a single-CPU bootstrap contract.
struct StaticTss(UnsafeCell<Tss>);
struct StaticGdt(UnsafeCell<Gdt>);

// SAFETY: init() is called once with interrupts disabled, before other CPUs or
// privilege transitions; only the CPU's descriptor bit writes are allowed
// after init, until a future explicitly synchronized update protocol exists.
unsafe impl Sync for StaticTss {}
// SAFETY: identical single-writer/one-time initialization invariant as StaticTss.
unsafe impl Sync for StaticGdt {}

struct StaticRing0Stack(UnsafeCell<Ring0Stack>);

// SAFETY: one BSP owns the privilege-transition stack. It is not exposed as a
// Rust slice/reference while hardware may use RSP0.
unsafe impl Sync for StaticRing0Stack {}

static TSS: StaticTss = StaticTss(UnsafeCell::new(Tss::new()));
static GDT: StaticGdt = StaticGdt(UnsafeCell::new(Gdt::EMPTY));
static RING0_STACK: StaticRing0Stack =
    StaticRing0Stack(UnsafeCell::new(Ring0Stack([0; RING0_STACK_BYTES])));

fn ring0_stack_top(stack: *mut Ring0Stack) -> u64 {
    stack as u64 + RING0_STACK_BYTES as u64
}

/// Initialize the permanent GDT and TSS and load GDTR and TR.
///
/// # Safety
/// Must be called exactly once on the boot CPU before interrupts, other CPUs,
/// or ring-3 entry. GDT/TSS and the dedicated RSP0 stack stay mapped at fixed
/// addresses for the kernel lifetime. IST slots remain zero and MUST be
/// initialized before any interrupt gate which selects an IST stack.
pub unsafe fn init() {
    let tss = TSS.0.get();
    let gdt = GDT.0.get();
    let ring0_stack = RING0_STACK.0.get();

    // SAFETY: one boot CPU initializes dedicated, static UnsafeCell backing.
    // No references/interrupts to these objects exist before this point.
    unsafe {
        core::ptr::write(tss, Tss::new());
        // Tss is packed, so write RSP0 without creating an unaligned reference.
        core::ptr::addr_of_mut!((*tss).rsp0).write_unaligned(ring0_stack_top(ring0_stack));
        core::ptr::write(gdt, Gdt::new(&*tss));
        // Pass a raw pointer: LGDT/LTR may change GDT Accessed/busy bits.
        load_gdt(gdt);
        load_tss();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn production_gdt_layout_and_selector() {
        assert_eq!(size_of::<GdtEntry>(), 8);
        assert_eq!(size_of::<TssDescriptor>(), 16);
        assert_eq!(size_of::<Gdt>(), 56);
        assert_eq!(align_of::<Gdt>(), 8);
        assert_eq!(offset_of!(Gdt, tss_descriptor), 0x28);
        assert_eq!(Gdt::TSS_SELECTOR, 0x28);
        assert_eq!(Gdt::KERNEL_CODE_SELECTOR, 0x08);
        assert_eq!(Gdt::KERNEL_DATA_SELECTOR, 0x10);
        assert_eq!(Gdt::USER_CODE_SELECTOR, 0x1b);
        assert_eq!(Gdt::USER_DATA_SELECTOR, 0x23);
    }

    #[test]
    fn ring0_privilege_stack_has_aligned_nonzero_top() {
        let mut stack = Ring0Stack([0; RING0_STACK_BYTES]);
        let base = &mut stack as *mut Ring0Stack as u64;
        let top = ring0_stack_top(&mut stack);
        assert_eq!(size_of::<Ring0Stack>(), RING0_STACK_BYTES);
        assert_eq!(align_of::<Ring0Stack>(), 16);
        assert_eq!(top - base, RING0_STACK_BYTES as u64);
        assert_eq!(top & 0xf, 0);
        assert_ne!(top, 0);
    }

    #[test]
    fn production_tss_layout_no_bitmap() {
        assert_eq!(size_of::<Tss>(), 104);
        assert_eq!(offset_of!(Tss, rsp0), 4);
        assert_eq!(offset_of!(Tss, ist), 36);
        assert_eq!(offset_of!(Tss, io_map_base), 102);
        let tss = Tss::new();
        let rsp0 = tss.rsp0;
        let rsp1 = tss.rsp1;
        let rsp2 = tss.rsp2;
        let ist = tss.ist;
        let io_map_base = tss.io_map_base;
        assert_eq!((rsp0, rsp1, rsp2), (0, 0, 0));
        assert_eq!(ist, [0; 7]);
        assert_eq!(io_map_base, size_of::<Tss>() as u16);
    }

    #[test]
    fn production_descriptor_base_limit_and_selectors() {
        let tss = Tss::new();
        let gdt = Gdt::new(&tss);
        let ptr = gdt.pointer();
        let gdtr_limit = ptr.limit;
        let gdtr_base = ptr.base;
        assert_eq!(size_of::<GdtPointer>(), 10);
        assert_eq!(gdtr_limit, (size_of::<Gdt>() - 1) as u16);
        assert_eq!(gdtr_base, &gdt as *const _ as u64);
        let desc = gdt.tss_descriptor;
        let base = desc.base_low as u64
            | ((desc.base_mid as u64) << 16)
            | ((desc.base_high as u64) << 24)
            | ((desc.base_upper as u64) << 32);
        let limit = desc.limit_low as u32 | (((desc.limit_high_flags & 0x0f) as u32) << 16);
        assert_eq!(base, &tss as *const _ as u64);
        assert_eq!(limit, (size_of::<Tss>() - 1) as u32);
        assert_eq!(desc.access, 0x89);
        let reserved = desc.reserved;
        assert_eq!(reserved, 0);
        assert_eq!(Gdt::TSS_SELECTOR as usize, offset_of!(Gdt, tss_descriptor));
    }

    #[test]
    fn production_entry_access_bytes() {
        let tss = Tss::new();
        let gdt = Gdt::new(&tss);
        assert_eq!(gdt.entries[0].access, 0);
        assert_eq!(gdt.entries[1].access, 0x9a);
        assert_eq!(gdt.entries[2].access, 0x92);
        assert_eq!(gdt.entries[3].access, 0xfa);
        assert_eq!(gdt.entries[4].access, 0xf2);
    }

    #[test]
    fn permanent_storage_is_in_static_objects() {
        assert_eq!(TSS.0.get(), TSS.0.get());
        assert_eq!(GDT.0.get(), GDT.0.get());
        assert_ne!(TSS.0.get() as usize, GDT.0.get() as usize);
        assert_eq!(size_of::<Gdt>(), 56);
    }
}
