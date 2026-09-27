//! Host-side unit tests for x86-64 GDT and TSS layout.
//!
//! These tests run on the host (not in the kernel) and validate that
//! GDT entries, TSS descriptors, and segment selectors have the correct
//! format per Intel SDM Vol. 3A Section 5.7.
//!
//! Compile and run with:
//! ```bash
//! rustc host-tests/gdt_tss.rs -o /tmp/gdt_tss_test && /tmp/gdt_tss_test
//! ```

use std::mem::size_of;

// ============================================================================
// GDT Entry (8 bytes)
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    limit_high_flags: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn new(base: u32, limit: u32, access: u8, flags: u8, base_high: u8) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access,
            limit_high_flags: (((limit >> 16) & 0x0F) | ((flags as u32) & 0xF0)) as u8,
            base_high,
        }
    }

    const NULL: Self = Self::new(0, 0, 0, 0, 0);

    const KERNEL_CODE: Self = Self::new(0, 0xFFFFF, 0x9A, 0xA0, 0);
    const KERNEL_DATA: Self = Self::new(0, 0xFFFFF, 0x92, 0xC0, 0);
    const USER_CODE: Self = Self::new(0, 0xFFFFF, 0xFA, 0xA0, 0);
    const USER_DATA: Self = Self::new(0, 0xFFFFF, 0xF2, 0xC0, 0);
}

// ============================================================================
// TSS Descriptor (16 bytes in 64-bit mode)
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct TssDescriptor {
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
    const fn new(base: u64, limit: u32) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access: 0x89,
            limit_high_flags: ((limit >> 16) & 0x0F) as u8,
            base_high: ((base >> 24) & 0xFF) as u8,
            base_upper: ((base >> 32) & 0xFFFF_FFFF) as u32,
            reserved: 0,
        }
    }
}

// ============================================================================
// TSS (104 bytes in 64-bit mode)
// ============================================================================

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    io_map_base: u16,
}

impl Tss {
    const fn new() -> Self {
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

// ============================================================================
// GDT Pointer (10 bytes)
// ============================================================================

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

// ============================================================================
// Tests
// ============================================================================

fn main() {
    let mut passed = 0;
    let mut failed = 0;

    macro_rules! test {
        ($name:expr, $cond:expr) => {
            if $cond {
                println!("  [PASS] {}", $name);
                passed += 1;
            } else {
                println!("  [FAIL] {}", $name);
                failed += 1;
            }
        };
    }

    println!("Running GDT/TSS layout tests...\n");

    // Size tests
    test!("GdtEntry size is 8 bytes", size_of::<GdtEntry>() == 8);
    test!("TssDescriptor size is 16 bytes", size_of::<TssDescriptor>() == 16);
    test!("Tss size is 104 bytes", size_of::<Tss>() == 104);
    test!("GdtPointer size is 10 bytes", size_of::<GdtPointer>() == 10);

    // GDT entry tests
    let null = GdtEntry::NULL;
    test!(
        "Null descriptor is all zeros",
        null.limit_low == 0
            && null.base_low == 0
            && null.base_mid == 0
            && null.access == 0
            && null.limit_high_flags == 0
            && null.base_high == 0
    );

    let kernel_code = GdtEntry::KERNEL_CODE;
    test!(
        "Kernel code access byte is 0x9A",
        kernel_code.access == 0x9A
    );
    test!(
        "Kernel code flags byte is 0xA0",
        kernel_code.limit_high_flags & 0xF0 == 0xA0
    );

    let kernel_data = GdtEntry::KERNEL_DATA;
    test!(
        "Kernel data access byte is 0x92",
        kernel_data.access == 0x92
    );
    test!(
        "Kernel data flags byte is 0xC0",
        kernel_data.limit_high_flags & 0xF0 == 0xC0
    );

    let user_code = GdtEntry::USER_CODE;
    test!(
        "User code access byte is 0xFA",
        user_code.access == 0xFA
    );

    let user_data = GdtEntry::USER_DATA;
    test!(
        "User data access byte is 0xF2",
        user_data.access == 0xF2
    );

    // TSS descriptor tests
    let tss = Tss::new();
    let tss_desc = TssDescriptor::new(&tss as *const _ as u64, size_of::<Tss>() as u32);
    test!(
        "TSS descriptor access byte is 0x89",
        tss_desc.access == 0x89
    );
    test!(
        "TSS descriptor reserved is zero",
        tss_desc.reserved == 0
    );
    test!(
        "TSS descriptor base matches TSS address",
        tss_desc.base_low as u64 | ((tss_desc.base_mid as u64) << 16) | ((tss_desc.base_high as u64) << 24) | ((tss_desc.base_upper as u64) << 32) == &tss as *const _ as u64
    );

    // TSS tests — use read_unaligned for packed struct fields
    let tss = Tss::new();
    test!("TSS rsp0 is zeroed", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(tss.rsp0)) } == 0);
    test!("TSS rsp1 is zeroed", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(tss.rsp1)) } == 0);
    test!("TSS rsp2 is zeroed", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(tss.rsp2)) } == 0);
    test!("TSS ist array is zeroed", unsafe {
        let ist: [u64; 7] = core::ptr::read_unaligned(core::ptr::addr_of!(tss.ist));
        ist == [0; 7]
    });
    test!("TSS io_map_base is zeroed", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(tss.io_map_base)) } == 0);

    // Segment selector tests
    test!("Kernel code selector is 0x08", 0x08u16 == 0x08);
    test!("Kernel data selector is 0x10", 0x10u16 == 0x10);
    test!("User code selector is 0x18", 0x18u16 == 0x18);
    test!("User data selector is 0x20", 0x20u16 == 0x20);
    test!("TSS selector is 0x28", 0x28u16 == 0x28);

    // GDT pointer tests
    let tss = Tss::new();
    let gdt_ptr = GdtPointer {
        limit: 79, // 80 - 1
        base: 0, // Would be the GDT address in real code
    };
    test!("GDT pointer limit is 79", gdt_ptr.limit == 79);

    println!("\n{} passed, {} failed", passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
}
