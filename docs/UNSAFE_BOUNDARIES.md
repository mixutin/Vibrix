# Current unsafe boundaries

This inventory records the QEMU-verified post-firmware PR #49 handoff, live RSDP parsing in #51 and early physical frame allocation in #54. It records invariants and remaining trust at the actual raw-pointer, firmware, assembly and hardware boundaries; it does not assert those invariants are independently proven by Rust types or the smoke test. `#[unsafe(no_mangle)]` exports symbols and does not by itself dereference a pointer.

## Loader: firmware ABI, protocols and physical resources

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `boot/src/main.rs::efi_main`; `uefi.rs::Console` | Firmware enters via `extern "efiapi"` with a live, correctly laid-out and aligned `SystemTable`; its console and `OutputString` callback must remain available for each synchronous UTF-16 write. | Nulls/statuses are checked where used. Firmware still supplies the backing/lifetime. Console output can change the UEFI memory map: no `Console::write` after a successful final-map capture. |
| `uefi.rs::load_kernel`, `KernelFile::as_slice` | Live loaded-image, Simple File System and file protocol function pointers; `allocate_pool(EfiLoaderData)` returns exclusively owned, readable backing initialized by firmware reads before `from_raw_parts` creates the file slice. | File size/read counts, selected statuses, null pointers and conversions are checked; handles are closed and failed-read pools freed, while the successful buffer remains owned during staging. Null checks cannot establish arbitrary firmware pointer alignment/extent or safe concurrent mutation. |
| `uefi.rs::find_rsdp` | Configuration-table entries and ACPI vendor-table bytes are mapped for every probed 20/36/extended byte range. | Checks bounds on count and RSDP length, signature and checksums; these validate *contents*, not accessibility or provenance of firmware-provided pointers. Returned RSDP is a physical numeric address, not an entry-time kernel reference. |
| `uefi.rs::discover_framebuffer` | GOP interface, mode and info pointers remain readable during inspection. No framebuffer pixels are accessed here. | Status/null/mode-info-size, linear format, geometry and checked physical range tests precede use; the framebuffer is not mapped for future kernel access by this operation. |
| `uefi.rs::allocate_loader_pages`, `page_services`, `allocate_pages_with` | Correct live Boot Services function pointers and `AllocatePages(EfiLoaderData)` contract. Returned physical pages must be 4 KiB aligned, uniquely owned and directly accessible in the loader's pre-handoff mapping before dereference. | Rejects null SystemTable/services and zero page count. A successful allocation whose physical base is zero is policy-invalid and **attempted to be freed**, rather than silently accepted. If that FreePages call fails, its exact firmware status outranks the original policy error. |
| `uefi.rs::free_loader_pages`, `free_pages_with` | Exact still-owned physical base and page count, live firmware and exactly one valid release attempt. | Returns `Result<(), Status>`, rejects zero page count and bad wrapper pointers, preserves actual FreePages status. Physical address zero is allowed for cleanup of a firmware allocation that violates the loader's nonzero-base policy. A failed release **does not** establish that ownership was relinquished. |

Firmware ABI types and callback slots are first-party `#[repr(C)]` / `extern "efiapi"` definitions in `boot/src/uefi.rs`. The firmware, not a Rust declaration, guarantees that valid non-null pointers refer to properly aligned, accessible objects with adequate lifetime.

## Loader: staged ELF image and page-table construction

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `boot/src/loader.rs::stage_kernel_with`, `copy_segments`, `verify_segments` | Convert page-exclusive, identity-accessible loader-owned physical backing to raw byte pointers; `write_bytes`, `copy_nonoverlapping`, pointer arithmetic and `from_raw_parts` require live, in-bounds, disjoint source/destination ranges and unmodified ELF data/metadata. | Checked PT_LOAD file/memory sizes and address conversions, 4 KiB rounding, 256 MiB staging policy, whole-span zeroing and byte/BSS verification. On copy/verification/physical conversion failure, FreePages is attempted and **release error takes precedence** if ownership is uncertain. A passed byte check cannot independently prove a firmware pointer's validity. |
| `boot/src/paging.rs::TableAllocator`, `build_kernel_page_tables` | Allocate page-exclusive EfiLoaderData for PML4/PDPT/PD/PT frames; convert physical address to a directly accessible pointer, zero new frames, read/write 64-bit raw entries. Hierarchy must remain inactive while firmware mappings are assumed for raw access. | Checks 4 KiB alignment/physical PTE mask and canonical-48 virtual pages, page-table budget and span/bounds; supervisor-only W/NX leaf flags track ELF PT_LOAD flags, contradictory shared-page mappings fail closed, software walker verifies expected leaf pages. The later PR #49 preflights NX/LA57/physical width and activates this PML4 only after firmware exit (ADR 0007). A future explicitly PF_W+PF_X ELF segment is not categorically rejected by this builder; do not claim a universal W^X admission policy. |
| `boot/src/paging.rs::release_all` and invalid allocation cleanup | Free every tracked page in deterministic reverse allocation order while retaining failed-free ownership; no use of freed page-table frames afterward. | First failed firmware status in reverse order is returned; successful frees are cleared and failed addresses compacted into a dense owned prefix. If boot aborts and the local allocator is dropped after a rollback failure, ownership does **not** escape to a recoverable cross-function tracker: reporting the failure is not full reclamation. Successful staging and table construction keep their pages allocated. |

The linked kernel currently has separate page-granular text, read-only data, writable GOT and writable data PT_LOAD mappings. A GNU_RELRO header does not itself make the GOT read-only: the current initial mapper derives writability from PT_LOAD `PF_W`. ADR 0003 covers **construction and software verification of an inactive hierarchy**, not activation.

## Loader: final UEFI memory map and debug output

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `boot/src/memory_map.rs::capture`, `capture_with_services` | Read a live Boot Services table and retain `GetMemoryMap`/`AllocatePages`/`FreePages` callbacks; allocate page-exclusive EfiLoaderData, identity-access physical buffer, zero capacity and hand firmware valid writable pointers for map bytes and scalar outputs. Firmware must not write beyond supplied capacity even on error. | Bounds growth/headroom/retries, alignment and checked capacity arithmetic. Validates final nonzero byte length, returned descriptor stride/alignment, capacity limit and whole descriptor count. Frees rejected/sizing-stale allocations **before** reacquisition; failed FreePages status is surfaced. Retains full allocation `physical_base/pages/capacity`, plus one successful `byte_len/map_key/descriptor_size/descriptor_version` tuple. Firmware controls pointer validity and may still stale the key via unrelated events. |
| `boot/src/main.rs` final refresh / ExitBootServices | All transition page allocations finish **before** the final GetMemoryMap. The same map buffer/key/stride/version tuple is used for BootInfo and ExitBootServices; stale EFI_INVALID_PARAMETER retries refresh the same owned pages, update BootInfo metadata and call no other firmware services. | QEMU independently observes `ExitBootServices succeeded` and kernel entry. A too-small refresh or repeated stale key fails closed; no firmware console after acquiring the final key. Loader-owned map/stack/BootInfo allocations are not yet reclaimed by a native allocator. |
| `uefi.rs::debug_write` | With `qemu-debugcon`, `out dx, al` writes to QEMU debug port `0xE9` under the relevant x86 I/O privilege/platform assumption. | Inline assembly declares no memory/stack effects and preserves flags. Without the feature it does nothing; it is diagnostic output, not proof of native serial/kernel logging or real-hardware device access. |

Memory-map capture's standalone host tests use the production acquisition helper with injected EFI ABI callbacks, not a fabricated full Boot Services table. The raw firmware descriptor version is retained without pretending the kernel has validated/parses unknown revisions.

## Kernel: source boundaries executed after firmware exit in QEMU

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `kernel/src/arch/x86_64/gdt.rs::init` / `load_gdt` / `load_tss` | Early single-core/interrupts-disabled, one-time init; immutable addresses of dedicated static `UnsafeCell` GDT/TSS backing; writable mapped GDT for CPU-set descriptor accessed/TSS busy bits. `lgdt`, segment reload via stack-using far return and `ltr` need valid selectors/descriptor base/limit and a live stack. | Real GDT's TSS selector equals descriptor offset `0x28`; 104-byte TSS uses inclusive limit 103 and I/O bitmap offset 104; user selectors include RPL3. `push/push/retfq` does **not** claim `nostack`. `unsafe impl Sync` relies on one boot-CPU writer and no uncoordinated later accesses. RSP0/IST stacks remain zero/unconfigured: no privilege-changing interrupts/user entry are safe merely because descriptors are defined. |
| `kernel/src/arch/x86_64/serial.rs::outb` / `inb` | Port-I/O assembly requires accessible COM1 registers at ring 0. Initialization clears inherited DLAB before disabling IER, then configures divisor/8N1/FIFO; TX polling is bounded and reports timeout. | Immutable `SerialPort`/`SerialConsole` avoids unnecessary Rust `UnsafeCell`/manual Sync. Hardware transactions are not serialized: coherent early output assumes one CPU/interrupts disabled until a future lock. Production host tests and QEMU's separate COM1 serial capture prove single-core kernel output after firmware exit; Target 001 and interrupt-concurrent output remain untested. |
| `kernel/src/main.rs::vibrix_kernel_entry`, `read_boot_info` | Unsafe exported kernel entry requires an aligned live, identity-mapped, loader-owned 88-byte v2 BootInfo page. Read v1 common prefix/version **before** any v2-tail access, then copy and validate the full object; physical framebuffer/map/RSDP addresses are not Rust references. | QEMU observes separate `kernel entry after ExitBootServices` and `kernel BootInfo v2 validated` markers. The map and framebuffer are mapped; M3 early conventional physical-frame allocation is now live in QEMU, but new-frame mapping/zeroing and full ACPI SDT traversal are not. |

## Kernel: post-firmware ACPI and early physical frame ownership (PRs #51 and #54)

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `kernel/src/main.rs::parse_boot_rsdp` | Turn the loader-validated physical RSDP number into readable `&[u8]` via `from_raw_parts`. ADR 0006 must identity-map the entire 20-/36-byte window (including a boundary-crossing RSDP); immutable firmware table bytes must remain present after exit. | Kernel-only `VIBRIX: kernel ACPI RSDP parsed` passes on QEMU (PR #51). The parser rejects bad signatures, checksums and extended length; only the fixed validated 36-byte ACPI 2.0+ window is read. Root SDTs and MCFG remain **unmapped**; an RSDP physical integer is not a pointer to its child tables. |
| `kernel/src/arch/x86_64/acpi_runtime.rs::with_table`, `memory::acpi_span_is_reserved` | BootInfo v3's live mapped leaf table is exclusively owned by the boot CPU, with IF=0. Every whole 4 KiB page must be retained firmware type 9/10 ACPI RAM and WB-capable, never runtime or conventional memory. Only after header ownership and mapping may the code create a 36-byte slice; only after checked SDT length/ownership may it map/read the complete <=1 MiB SDT. The callback cannot retain a reference after unmap, all leaves are retired even after failure and no ACPI pages are reclaimed. | Exact-head [QEMU run 36344773356](https://github.com/mixutin/Vibrix/actions/runs/36344773356) observed seven successful post-ExitBootServices mappings of real XSDT and MCFG, one MCFG allocation on COM1 each time. Host tests cover missing and wrong-type firmware pages, runtime/WB attributes, boundary straddles and overflow. This does not map ECAM MMIO, parse AML, enable interrupts or prove Target 001. |
| `kernel/src/memory/mod.rs::init_from_boot_info` | Convert the loader-owned physical map base and checked length into a `&'static [u8]` under the active identity-mapped PML4. Firmware must not mutate it after EBS; the EfiLoaderData map backing stays reserved for this early allocator's lifetime. The shared BootInfo v2 is scalar-validated before the conversion. | The production frame allocator bounds map size and descriptor count, validates stride/overlap/extent before issuance, and returns only conventional type-7 page **numbers**. QEMU after EBS observes the two frame-allocator markers. No freshly issued physical page is dereferenced, mapped or zeroed by this API. |
| `kernel/src/memory/mod.rs::EarlyFrameState`, `init_from_boot_info`, `allocate_frame` | A sole mutable `UnsafeCell<Option<EarlyAllocator>>` and `unsafe impl Sync` are justified **only while one boot CPU executes with interrupts disabled**; both APIs are unsafe and require this. The one-time holder prevents separate frame allocators from issuing the same page; the map, RSDP and GOP ranges are explicitly rounded up and excluded even if firmware types are surprising. | [QEMU run 36339966455](https://github.com/mixutin/Vibrix/actions/runs/36339966455) observes two distinct nonzero 4-KiB conventional frames claimed after EBS. This is a monotonic allocator with no free/reuse or synchronization. Must replace `UnsafeCell` access with interrupt-safe multi-CPU ownership before IDT-driven allocation, IF=1 or SMP access; compiling an IDT alone does **not** make this safe in handlers. |

## Kernel: installed synchronous IDT and fault capture (PR #52)

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `kernel/src/arch/x86_64/idt.rs::PermanentIdt`, `init` | An `UnsafeCell<IdtTable>` and manual `Sync` hold the 256-gate, 4096-byte permanent static IDT. Only the boot CPU initializes it **once** with IF clear, after GDT/TSS and kernel COM1 are installed. `lidt` receives a 10-byte IDTR pointing to permanently mapped and initialized kernel storage; installed selectors must match the current 64-bit code segment. | Production and host gate-layout tests plus exact-head [QEMU run 36340579141](https://github.com/mixutin/Vibrix/actions/runs/36340579141) observe kernel-only `VIBRIX: kernel IDT installed` and real #BP/#PF delivery after EBS. Most vectors stay not present. No IF=1/IRQ, IST, RSP0 ring-3 stack, SMP/parallel mutation or Target 001 guarantees. |
| `kernel/src/arch/x86_64/idt.rs` exception handlers, `page_fault_handler` | Nightly Rust `extern "x86-interrupt"` ABI must produce the CPU-compatible stack-frame and IRETQ handling for returning #BP; #DF/#GP/#PF are deliberately non-returning. The page-fault handler reads CR2 **before** any logging might disturb it, then decodes the hardware error bits without dereferencing the faulting address. Debug output assumes one CPU and a live COM1. | Separate real QEMU `int3` and unmapped read probes report returning breakpoint RIP and #PF CR2=`0x10000000000` with P/W/U/RSVD/I bits on native serial, not just unit-test fixtures. #DF/#GP gates are installed but not fault-injected; #DF has **no IST emergency stack** and must not be advertised as safe for stack exhaustion. |
| `kernel/src/main.rs` optional `breakpoint-probe`, `page-fault-probe` inline assembly | Only explicitly selected QEMU test builds execute INT3 or load from a canonical unmapped address. The default production boot must not intentionally fault; the probe uses registers/asm rather than constructing an invalid Rust reference. | CI checks the real kernel's independent debugcon and COM1 logs, after all merged ACPI/frame allocator startup markers. Probe success is not timer delivery, a native driver or hardware fault recovery. |

## Kernel: native PCI configuration I/O (segment zero)

| Production owner | Unsafe operation and required invariant | Validation and residual limitation |
| --- | --- | --- |
| `kernel/src/arch/x86_64/pci.rs::read_legacy_dword` and `discover_legacy_segment_zero` | Ring-zero x86 I/O access to the shared PCI legacy configuration address/data pair (CF8/CFC) while one boot CPU runs with IF=0. Hardware config selection must not race a second CPU or interrupt handler. Only CF8 receives a selector write; CFC is read-only and no PCI device register or disk is written. | The pure address generator rejects out-of-range BDF functions/register alignment; the kernel scans all 256 legacy segment-zero buses and decodes only supported type-0/type-1 BAR layouts. Exact-head QEMU smoke requires separate kernel-originated PCI enumeration and assigned-BAR markers plus native COM1 summary. No segment >0, ACPI MCFG/ECAM, resource sizing, BAR MMIO mapping, bus-master setup, driver binding, xHCI activation or persistent boot USB is claimed. |

| `kernel/src/memory/virtual_memory.rs::Window::map_mmio_readonly`; `frame_allocator::covers_mmio_bytes` and `acpi_runtime::read_ecam_word` | The actual CPU's PAT index 3 must be UC and the validated MCFG describes segment-zero bus-zero device aperture. Each page is described in the immutable final firmware memory map as `EfiReservedMemoryType` or `EfiMemoryMappedIO` with UC capability and without EFI runtime flag; normal/loader/ACPI RAM is refused. The one CPU has IF=0, owns the temporary leaf PT, maps supervisor RO/NX/PWT+PCD and performs only aligned volatile 32-bit reads with no references escaping immediate unmap. No incompatible cached alias or concurrent PCI config consumer exists during this bootstrap. | [Exact-head QEMU CI 36346144132](https://github.com/mixutin/Vibrix/actions/runs/36346144132) observed 4 bus-zero function-zero devices, or 5 including 1 native xHCI when QEMU adds it; all seven QEMU variants, production MMIO descriptor and leaf PTE host tests passed. No config writes, BAR MMIO, DMA, activation, multifunction/other-bus scan, Target 001 or runtime CPU synchronization. |
| `kernel/src/arch/x86_64/apic.rs`, `Window::map_mmio_writable`, `frame_allocator::permits_external_mmio_page` | MADT and `IA32_APIC_BASE` independently identify architectural APIC device pages before pointer creation. The final UEFI map is used as a conflict detector: a missing APIC descriptor is allowed because QEMU OVMF omits these architectural pages, but any present descriptor must be non-runtime UC reserved/MMIO, never RAM/loader/ACPI memory. PAT index 3 must be UC. One BSP with IF=0 exclusively owns the temporary leaf window. LAPIC ID/version are volatile reads; I/O APIC reads write only IOREGSEL selectors 0/1 then read IOWIN. | QEMU pre-sync validation observed real LAPIC/I/O-APIC IDs/versions across the normal and fault-probe matrix. No redirection-table write, EOI, PIC masking, IRQ gate, STI, timer/keyboard delivery, SMP or Target 001 is claimed. This weaker "absent descriptor permitted" rule is APIC-specific and must not replace the stricter ECAM firmware-map ownership check. |

| `kernel/src/arch/x86_64/ps2.rs::poll_scancode` | Post-firmware boot CPU, CPL0 with I/O permission and IF=0; no other driver consumes i8042 output. Poll status port 0x64 and only read data port 0x60 when output-ready; drain auxiliary bytes without turning them into keyboard keys. No firmware call or port write; set-one make-code decoder ignores releases, extended and unsupported input. | Host tests cover actual production scan-code decoding. [CI run 36347623002](https://github.com/mixutin/Vibrix/actions/runs/36347623002) observed independent native COM1 ASCII 104 and 10 as separate exact lines following real HMP-injected keys after the kernel readiness marker. No hardware interrupt path, USB HID, physical Target 001 PS/2 existence or full console is claimed. |

## Activation, framebuffer and panic: new unsafe boundaries in PR #49

| Production owner | Required safety invariant | Validation and limitation |
| --- | --- | --- |
| `boot/src/paging.rs::map_identity_regions` | Root PML4 and subordinate tables are loader-owned EfiLoaderData, accessible under the firmware mapping during construction. All physical ranges are checked and software-walked; framebuffer MMIO leaves use PCD+NX, kernel pages retain ELF-derived W/NX. | Dedicated stack, loader PE, BootInfo, full map capacity, RSDP and framebuffer are mapped narrowly; does not map arbitrary RAM or all ACPI tables. The temporary loaded-PE identity interval is W+X and must be removed before claiming global W^X. |
| `boot/src/transition.rs::preflight` | UEFI long-mode CPL0 permits reading CR4. CPUID must advertise NX, CR4.LA57 must be clear and the CPU physical-address width must contain all submitted regions before exit. | Fails closed before irrevocable EBS; no 5-level hierarchy or hardware Target 001 validation. |
| `boot/src/transition.rs::enter_kernel` | Must be invoked **only** after successful EBS. The currently executing PE instruction pages remain identity-mapped over MOV CR3; verified higher-half ELF entry, BootInfo and 16-page stack are mapped. Enable EFER.NXE with RDMSR/WRMSR, CLI, switch CR3, replace RSP before other Rust instructions, place a fake return slot giving SysV RSP%16==8, pass BootInfo in RDI and jump via distinct RAX entry register. | Exact-head QEMU observed the independent higher-half kernel marker and native COM1 log. This is not general proof across all processors, UEFI firmware or Target 001. |
| `boot/src/uefi.rs::exit_boot_services_service` | Typed cached EFIABI function pointer is copied from live firmware **before** first EBS attempt, never dereferenced after exit. Retry invokes only cached GetMemoryMap, not any other firmware service; unexpected exit statuses fail-stop. | QEMU observed exit success; EFI_INVALID_PARAMETER refresh tested only via map-buffer host tests, not forced as a live QEMU firmware failure. |
| `kernel/src/framebuffer.rs::draw_boot_marker`, `PixelSurface::pixel` | BootInfo scalar dimensions, stride, base and byte extent are validated, every volatile 32-bit pixel offset and address checked. Loader-provided GOP BAR is supervisor-writable and PCD+NX, uniquely accessed on the boot CPU after firmware exit. A 352x112 banner is drawn only on sufficiently large RGB/BGR modes; small and unknown PixelBitMask modes retain the original bounded marker. | Production host tests check exact glyph/border pixels and guard bytes beyond the mapped GOP extent. QEMU's **kernel-originated** status-banner marker confirms the banner routine ran after EBS, but headless QEMU is not a human visual screenshot test. The two static labels are not a kernel text console, font library or shell. |
| `kernel/src/debugcon.rs` and `kernel/src/main.rs::panic` | Only the `qemu-debugcon` feature emits I/O port 0xe9 writes. The kernel panic handler requires the early COM1/single-CPU setup to report without UEFI or heap allocation. | Normal QEMU checks independent post-EBS debugcon + COM1. A separate panic-probe QEMU boot observes the real kernel panic handler on both channels, not a fabricated loader message. |

## Observed evidence and remaining scope

[Normal QEMU/OVMF run 36337520346](https://github.com/mixutin/Vibrix/actions/runs/36337520346)
exercised final memory-map refresh, ExitBootServices, NX/CR3/stack switch,
kernel BootInfo v2 validation, CPUID, GDT/TSS, COM1 and framebuffer writes.
[Separate panic QEMU run 36337648665](https://github.com/mixutin/Vibrix/actions/runs/36337648665)
also exercised the standalone kernel panic handler on debugcon and COM1.
[ACPI QEMU run 36338546382](https://github.com/mixutin/Vibrix/actions/runs/36338546382)
observed kernel-side RSDP validation, and
[early frame allocator run 36339966455](https://github.com/mixutin/Vibrix/actions/runs/36339966455)
observed two conventional physical frame claims by the real post-firmware
kernel. [Synchronized IDT QEMU run 36340579141](https://github.com/mixutin/Vibrix/actions/runs/36340579141)
additionally observed native #BP and #PF delivery after the earlier
kernel ACPI/frame checks. None of those establishes Target 001 real
hardware, **hardware IRQ routing/IST**, whole-ACPI parsing, freshly
mapped/zeroed frames, USB-storage reacquisition, a persistent filesystem
or userspace execution. The
loader-owned physical pages remain reserved under the type-7-only
allocator; any later reclaim requires an explicit ownership transfer.

New unsafe code must record alignment, allocation extent, provenance,
ownership, lifetime, mapping, cache policy and synchronization at the
actual operation. See [ADR 0007](decisions/0007-uefi-exit-kernel-entry.md),
[ADR 0006](decisions/0006-transition-mappings.md) and
[UEFI](https://uefi.org/specifications) / the Intel x86-64 SDM.

## BootInfo v3 and early mapping window (PR #62)

The historical v2 checkpoints above are superseded for the current entry
contract by a mapped, aligned **96-byte v3** BootInfo object. Version is
checked in the common prefix before the new tail is read. See ADR 0009.

`prepare_kernel_window` accesses only exclusively owned inactive loader
page tables, verifies empty leaves and retains all linked allocations.
The PT is narrowly identity-mapped RW/NX and never reclaimed by the frame
allocator. Failure aborts the attempt; no partial rollback is promised.

`memory::virtual_memory::Window` owns raw volatile access to 512 mapped,
u64-aligned entries on the sole boot CPU with IRQs disabled. Its unsafe
constructor requires the v3 parent hierarchy and exclusive lifetime;
index/physical-width checks do not independently prove those invariants.
Map/protect/unmap require caller-owned WB RAM and retirement of incompatible
references. Each store precedes INVLPG; the assembly has no `nomem` claim.
CR0.WP is set while preserving all other bits. Leaves are supervisor/NX.
No SMP, interrupt allocation, MMIO caching or physical-frame reuse is provided.
Two optional QEMU probes deliberately fault through the installed IDT.
The default boot only accesses mapped owned frames, then unmaps them.

The separate early heap from PR #60 owns a 64 KiB static mapped arena and
metadata in UnsafeCell under the same sole-CPU/IRQs-off restriction. See
[EARLY_HEAP.md](EARLY_HEAP.md) for allocation lifetime and free/reuse rules.
