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

use core::arch::asm;

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
        0,
        0xFFFFF,
        0xFA, // present | ring 3 | code | executable | readable
        0xA0, // L=1, G=1
        0,
    );

    /// Create a 64-bit user data segment descriptor.
    ///
    /// Access byte: 0xF2 = present, ring 3, data, writable
    /// Flags: 0xC0 = D/B=1, G=1
    pub const USER_DATA: Self = Self::new(
        0,
        0xFFFFF,
        0xF2, // present | ring 3 | data | writable
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
    /// Create a zeroed TSS.
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
            io_map_base: 0,
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
    entries: [GdtEntry; 8],
    tss_descriptor: TssDescriptor,
}

impl Gdt {
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
                GdtEntry::NULL, // reserved for future use
                GdtEntry::NULL, // reserved for future use
                GdtEntry::NULL, // reserved for future use
            ],
            tss_descriptor: TssDescriptor::new(
                tss as *const _ as u64,
                core::mem::size_of::<Tss>() as u32,
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
    pub const USER_CODE_SELECTOR: u16 = 0x18;
    /// Get the user data segment selector.
    pub const USER_DATA_SELECTOR: u16 = 0x20;
    /// Get the TSS segment selector.
    pub const TSS_SELECTOR: u16 = 0x28;
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
/// The GDT must be valid and remain loaded for the lifetime of the kernel.
/// This function is typically called once during early kernel initialization.
pub unsafe fn load_gdt(gdt: &Gdt) {
    let ptr = gdt.pointer();

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
            options(nostack)
        );
    }
}

/// Load the Task Register (TR) with the TSS selector.
///
/// # Safety
///
/// The TSS must be valid and remain loaded for the lifetime of the kernel.
pub unsafe fn load_tss() {
    unsafe {
        asm!("ltr ax", in("ax") Gdt::TSS_SELECTOR, options(nostack));
    }
}

/// Initialize the GDT and TSS.
///
/// This is the main entry point for GDT/TSS setup. It creates a TSS,
/// builds the GDT, loads it, and loads the TR.
///
/// # Safety
///
/// Must be called exactly once during early kernel initialization,
/// before any interrupts are enabled.
pub unsafe fn init() {
    let tss = Tss::new();
    let gdt = Gdt::new(&tss);

    unsafe {
        load_gdt(&gdt);
        load_tss();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdt_entry_size() {
        assert_eq!(core::mem::size_of::<GdtEntry>(), 8);
    }

    #[test]
    fn test_tss_descriptor_size() {
        assert_eq!(core::mem::size_of::<TssDescriptor>(), 16);
    }

    #[test]
    fn test_tss_size() {
        assert_eq!(core::mem::size_of::<Tss>(), 104);
    }

    #[test]
    fn test_gdt_size() {
        // 8 entries * 8 bytes + 16 bytes for TSS descriptor = 80 bytes
        assert_eq!(core::mem::size_of::<Gdt>(), 80);
    }

    #[test]
    fn test_gdt_pointer_size() {
        assert_eq!(core::mem::size_of::<GdtPointer>(), 10);
    }

    #[test]
    fn test_selectors() {
        assert_eq!(Gdt::KERNEL_CODE_SELECTOR, 0x08);
        assert_eq!(Gdt::KERNEL_DATA_SELECTOR, 0x10);
        assert_eq!(Gdt::USER_CODE_SELECTOR, 0x18);
        assert_eq!(Gdt::USER_DATA_SELECTOR, 0x20);
        assert_eq!(Gdt::TSS_SELECTOR, 0x28);
    }

    #[test]
    fn test_tss_new_is_zeroed() {
        let tss = Tss::new();
        assert_eq!(tss.rsp0, 0);
        assert_eq!(tss.rsp1, 0);
        assert_eq!(tss.rsp2, 0);
        assert_eq!(tss.ist, [0; 7]);
        assert_eq!(tss.io_map_base, 0);
    }

    #[test]
    fn test_gdt_layout() {
        let tss = Tss::new();
        let gdt = Gdt::new(&tss);
        let ptr = gdt.pointer();
        assert_eq!(ptr.limit, 79); // 80 - 1
        assert_eq!(ptr.base, &gdt as *const _ as u64);
    }
}
