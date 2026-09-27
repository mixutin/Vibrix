//! Independent host layout checks against the actual production GDT types.
//! The previous standalone constant-only checks could pass while the kernel
//! pointed LTR at the wrong descriptor; importing gdt.rs avoids that gap.

#[path = "../kernel/src/arch/x86_64/gdt.rs"]
mod gdt;

#[test]
fn tss_descriptor_starts_at_actual_selector() {
    assert_eq!(gdt::Gdt::TSS_SELECTOR as usize, 0x28);
    assert_eq!(core::mem::size_of::<gdt::Gdt>(), 56);
    assert_eq!(core::mem::size_of::<gdt::GdtPointer>(), 10);
}

#[test]
fn tss_hardware_layout_is_not_native_rust_padded() {
    assert_eq!(core::mem::offset_of!(gdt::Tss, rsp0), 4);
    assert_eq!(core::mem::offset_of!(gdt::Tss, ist), 36);
    assert_eq!(core::mem::offset_of!(gdt::Tss, io_map_base), 102);
    assert_eq!(core::mem::size_of::<gdt::Tss>(), 104);
}
