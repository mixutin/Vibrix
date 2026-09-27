# Current unsafe boundaries

This is an inventory of code **on `main`**, not a design for future drivers or
proof that every firmware pointer is safe. The UEFI loader owns all current
`unsafe` blocks; the kernel does not yet dereference `BootInfo` or initialize
an IDT, GDT, frame allocator, heap, page tables, or device I/O. `#[unsafe(no_mangle)]`
on each entry point is an exported-symbol requirement, not a pointer access.
The separate initial-page-table PR is not included until it lands on `main`.

## Loader: firmware lifetime and protocol pointers

| Owner | Boundary and required invariant | Checks and remaining trust |
| --- | --- | --- |
| `boot/src/main.rs::efi_main`, `uefi.rs::Console` | Firmware enters with the UEFI x86-64 calling convention and a live, correctly laid-out `SystemTable`; `con_out` and its `output_string` function remain valid until this loader stops using Boot Services. | Null table/output pointers are rejected. The firmware, not a Rust type check, guarantees alignment, structure layout, function pointers and lifetime. `Console::write` passes a local NUL-terminated UTF-16 pair synchronously. |
| `uefi.rs::load_kernel`, `KernelFile::as_slice` | Loaded-image, filesystem, root/file protocol pointers and callbacks belong to live firmware. The `allocate_pool(EfiLoaderData)` result must be initialized by `read`, remain mapped and exclusive to this loader while a slice exists, and not be freed early. | Status/null checks and nonzero/representable file size precede use; failed reads close handles and free the pool, successful reads close handles but retain the pool for the loader lifetime. Firmware is trusted not to report a read count larger than the supplied buffer or return misaligned/invalid pointers. A Rust slice cannot independently validate firmware memory. |
| `uefi.rs::find_rsdp` | The configuration-table array and the matching ACPI vendor-table pointer must remain mapped and readable for every byte inspected. | Rejects null table, zero or >4096 entries, null vendor pointer, invalid signature/checksum, and invalid extended length (36..=4096). Reading the initial 20/36 bytes and declared extended length still assumes firmware supplies accessible backing; checksum is not a memory-validity proof. The returned number is an address, not a kernel mapping. |
| `uefi.rs::discover_framebuffer` | `LocateProtocol` returns a live, properly aligned GOP interface, mode and information structure. The framebuffer address itself is **not** dereferenced here. | Checks status/null pointers, mode-info size, linear pixel format, geometry, checked byte count and framebuffer extent. Firmware supplies pointer validity and backing; geometry validation does not map the framebuffer for the kernel. |
| `uefi.rs::allocate_loader_pages`, `free_loader_pages` | The SystemTable/Boot Services callbacks remain live; a successful allocation is loader-owned until freed once or explicitly reserved for handoff. Freeing must use the original address and page count before `ExitBootServices`. | Rejects null table, zero page count and null services; rejects a returned zero physical address and attempts to free it. Firmware is trusted to allocate page-aligned, accessible `EfiLoaderData` pages and to honor ownership. |

UEFI protocol structures and callbacks are defined locally with `#[repr(C)]`
and `extern "efiapi"` in `boot/src/uefi.rs`. These definitions are the FFI
boundary: the loader cannot prove firmware-origin pointer provenance, memory
extent or concurrent mutation merely by checking for null. No Boot Services
call is permitted after a future successful `ExitBootServices` handoff; the
current loader never reaches that transition.

## Loader: physical staging and debug output

| Owner | Boundary and required invariant | Checks and remaining trust |
| --- | --- | --- |
| `boot/src/loader.rs::stage_kernel`, `copy_segments`, `verify_file_bytes`, `verify_bss` | The firmware allocation spans the computed page-rounded virtual image size and is identity-accessible to the loader before any CR3 switch. Conversion of its physical base to a pointer is valid only under that pre-handoff mapping. `write_bytes`, `copy_nonoverlapping`, pointer arithmetic and `from_raw_parts` require initialized, in-bounds, nonoverlapping storage; source ELF bytes stay immutable during copying. | Staging bounds the span to 256 MiB, uses checked virtual/file arithmetic and conversions, validates PT_LOAD metadata, zeroes the entire span, copies file-backed bytes and verifies them and BSS. Allocation failures and copy/verification errors release pages. The firmware allocation/mapping guarantee is still necessary; byte verification alone cannot prove memory safety. |
| `uefi.rs::debug_write` | When `qemu-debugcon` is enabled, x86 `out dx, al` writes bytes to QEMU's debug I/O port `0xE9`; inline assembly has no memory/stack effects and preserves flags. | Build-time feature gates the port operation; it is not compiled into the ordinary bare-metal build. This is diagnostic output, not evidence of native serial, memory-mapped I/O or kernel logging. |

The ELF parser (`boot/src/elf.rs`) works on checked byte slices and contains
no `unsafe` blocks. Its metadata validation is necessary before staging but
does not make arbitrary firmware-returned pointers safe. The loader currently
stages kernel bytes, prints `VIBRIX: kernel segments staged`, then spins; it
does **not** activate new mappings, populate `BootInfo`, call
`ExitBootServices`, or jump to the kernel on `main`.

## Kernel: present boundary, not future promises

`kernel/src/main.rs` declares `#[repr(C)] BootInfo` and exports
`vibrix_kernel_entry(_boot_info: *const BootInfo)` with
`#[unsafe(no_mangle)]`. It does not dereference the pointer. The caller will
eventually need to provide a mapped, aligned, live and version-validated
argument under [ADR 0001](decisions/0001-bootinfo-address-spaces.md); that is
not implemented. `kernel/src/arch/x86_64/cpuid.rs` calls the **safe** official
Rust `core::arch::x86_64::__cpuid_count` intrinsic, gates optional leaves by
the advertised maxima and only records feature bits. It does not execute an
`unsafe` block or enable NX, SMEP, SMAP or APIC. There is no kernel inline
assembly on current `main`.

## Evidence and maintenance

The current CI compiles/Clippy-checks loader and kernel, runs host ELF and
CPUID tests (plus the independent GPT inspector tests), and uses OVMF/QEMU to
check loader discovery and kernel segment staging. Host CPUID tests exercise
the decoder and host instruction, **not** kernel entry. QEMU staging exercises
loader firmware calls and physical copying, **not** the post-firmware kernel
or bare metal. These checks do not prove all unsafe invariants, especially
malformed firmware pointer backing or later use after Boot Services exit.

When a new unsafe operation lands, its owner must document alignment, extent,
ownership, lifetime, mapping and synchronization at the operation, update
this inventory against the merged code, and provide relevant build/test/QEMU
evidence without marking a roadmap item complete prematurely.

References: [UEFI Specification](https://uefi.org/specifications) (System
Table, Boot Services, protocols and GOP), [ACPI Specification](https://uefi.org/specifications)
(RSDP layout/checksums), [Rust pointer safety rules](https://doc.rust-lang.org/core/ptr/index.html),
[Rust `from_raw_parts` safety requirements](https://doc.rust-lang.org/core/slice/fn.from_raw_parts.html),
and [Rust x86-64 CPUID intrinsic](https://doc.rust-lang.org/core/arch/x86_64/fn.__cpuid_count.html).
