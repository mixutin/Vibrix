//! Production-linked host checks for the x86-64 fast-syscall selector contract.

#[path = "../kernel/src/arch/x86_64/gdt.rs"]
mod gdt;
#[path = "../kernel/src/arch/x86_64/syscall.rs"]
mod syscall;

#[test]
fn star_derivations_match_live_gdt_layout() {
    assert_eq!(gdt::Gdt::KERNEL_CODE_SELECTOR, 0x08);
    assert_eq!(gdt::Gdt::KERNEL_DATA_SELECTOR, 0x10);
    assert_eq!(gdt::Gdt::USER_DATA_SELECTOR, 0x1b);
    assert_eq!(gdt::Gdt::USER_CODE_SELECTOR, 0x23);

    assert_eq!(
        syscall::derived_syscall_selectors(),
        (
            gdt::Gdt::KERNEL_CODE_SELECTOR,
            gdt::Gdt::KERNEL_DATA_SELECTOR
        )
    );
    assert_eq!(
        syscall::derived_sysret_selectors(),
        (gdt::Gdt::USER_CODE_SELECTOR, gdt::Gdt::USER_DATA_SELECTOR)
    );
}

#[test]
fn star_and_fmask_values_are_stable() {
    assert_eq!((syscall::star_value() >> 32) as u16, 0x08);
    assert_eq!((syscall::star_value() >> 48) as u16, 0x10);
    assert_eq!(syscall::fmask_value() & (1 << 9), 1 << 9);
}
