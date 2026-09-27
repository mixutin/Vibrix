//! Host tests import the production kernel GDT/TSS implementation directly.
//! The module's own tests assert the actual descriptor bytes, offsets,
//! selectors, backing lifetime and inclusive TSS bounds.

#[path = "../kernel/src/arch/x86_64/gdt.rs"]
mod gdt;

#[test]
fn real_gdt_gdtr_bounds_and_selector() {
    let tss = gdt::Tss::new();
    let table = gdt::Gdt::new(&tss);
    let pointer = table.pointer();
    let limit = pointer.limit;
    let base = pointer.base;

    assert_eq!(core::mem::size_of::<gdt::Gdt>(), 56);
    assert_eq!(limit, 55);
    assert_eq!(base, &table as *const _ as u64);
    assert_eq!(gdt::Gdt::TSS_SELECTOR, 0x28);
}

#[test]
fn real_tss_bitmap_is_outside_descriptor_limit() {
    let tss = gdt::Tss::new();
    let io_map_base = tss.io_map_base;
    assert_eq!(core::mem::size_of::<gdt::Tss>(), 104);
    assert_eq!(io_map_base, 104);
}
