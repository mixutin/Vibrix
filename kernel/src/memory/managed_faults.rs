//! Opt-in CPU faults using the exact production mapper and native adapter.
//! Deliberately faulting accesses use assembly, not invalid Rust references.
use super::*;

fn arm(probe: &str, address: u64) {
    match probe {
        "write" => crate::debugcon::write("VIBRIX: managed VM write fault armed\r\n"),
        "unmap" => crate::debugcon::write("VIBRIX: managed VM unmap fault armed\r\n"),
        "guard" => crate::debugcon::write("VIBRIX: managed VM guard fault armed\r\n"),
        "nx" => crate::debugcon::write("VIBRIX: managed VM nx fault armed\r\n"),
        _ => panic!("unknown managed VM probe"),
    }
    crate::println!("managed VM probe={probe} address={address:#x}");
}

/// # Safety
/// Same matched-loader, sole-BSP, IF=0 and empty-arena contract as smoke_test.
/// This is a terminal opt-in test: an expected #PF never returns to startup.
pub(super) unsafe fn run(info: &BootInfo) -> Result<(), Error> {
    let probe = if cfg!(feature = "managed-write-probe") {
        "write"
    } else if cfg!(feature = "managed-unmap-probe") {
        "unmap"
    } else if cfg!(feature = "managed-guard-probe") {
        "guard"
    } else {
        "nx"
    };
    // SAFETY: the normal native test has torn down all mappings and released
    // its scratch owner. This acquires a fresh disjoint pool and private owner.
    unsafe {
        let (root, bits) = cpu_configuration()?;
        let mut frames = Frames::<POOL_FRAMES>::new(bits)?;
        let mut reserved = [0; POOL_FRAMES];
        for slot in &mut reserved {
            let frame = crate::memory::allocate_frame().ok_or(Error::OutOfFrames)?;
            if !crate::memory::firmware_descriptor_at(frame).is_some_and(|(kind, attributes)| {
                kind == 7 && attributes & 8 != 0 && attributes & (1 << 63) == 0
            }) {
                return Err(Error::InvalidFrame);
            }
            frames.register(frame)?;
            *slot = frame;
        }
        let backend = NativeMemory {
            window: runtime::from_boot_info(info).map_err(|_| Error::InvalidRoot)?,
            root,
            reserved,
        };
        let mut vm = Vm::new(root, backend, frames)?;
        let page = Page::new(ARENA_BASE)?;
        if probe == "guard" {
            let mut guarded = GuardedVm::<_, POOL_FRAMES, 1>::new(vm)?;
            let layout = GuardedLayout::new(page, 1)?;
            let id = guarded.allocate(layout)?;
            let payload = guarded.layout(id)?.payload().page(0).ok_or(Error::InvalidRange)?;
            fill(payload, 0x7788);
            check(payload, Some(0x7788))?;
            let address = layout.upper_guard().address();
            if guarded.query(layout.upper_guard())?.is_some() {
                return Err(Error::CorruptEntry);
            }
            arm(probe, address);
            // SAFETY: controlled terminal CPU test. Raw assembly intentionally
            // reads an absent guard; no invalid Rust reference/read is formed.
            asm!("mov rax, qword ptr [{}]", in(reg) address, out("rax") _, options(nostack, preserves_flags));
        } else {
            vm.map_zeroed(page, Permissions::ReadWrite)?;
            fill(page, 0x1234);
            check(page, Some(0x1234))?;
            match probe {
                "write" => {
                    vm.protect(page, Permissions::ReadOnly)?;
                    check(page, Some(0x1234))?;
                }
                "unmap" => {
                    vm.unmap(page)?;
                    if vm.query(page)?.is_some() {
                        return Err(Error::CorruptEntry);
                    }
                }
                "nx" => {
                    // SAFETY: this byte is in live owned RW/NX initialized RAM.
                    // RET would safely return only if the expected NX protection
                    // were missing, in which case the terminal panic fails CI.
                    (page.address() as *mut u8).write_volatile(0xc3);
                }
                _ => return Err(Error::InvalidRange),
            }
            arm(probe, page.address());
            // SAFETY: controlled terminal fault probes against our own arena.
            // Assembly prevents Rust from assuming the deliberately invalid
            // access is valid; no nomem permits reordering before preparation.
            match probe {
                "write" => {
                    asm!("mov qword ptr [{}], 0x1234", in(reg) page.address(), options(nostack, preserves_flags));
                }
                "nx" => {
                    asm!("call {}", in(reg) page.address(), clobber_abi("C"));
                }
                _ => {
                    asm!("mov rax, qword ptr [{}]", in(reg) page.address(), out("rax") _, options(nostack, preserves_flags));
                }
            }
        }
    }
    panic!("managed VM fault probe returned without expected exception")
}
