# ADR 0009: BootInfo v3 and the early kernel mapping window

- Status: Accepted; QEMU verified
- Scope: M3 virtual-memory groundwork

The physical allocator issues owned frame numbers, but the kernel cannot
dereference new frames under the loader's narrow mappings. A general page-table
allocator would itself need a mapped scratch frame. This bootstrap step instead
reserves one empty, loader-allocated PT for 512 supervisor 4 KiB pages at
`0xffffc00000000000` (2 MiB). No other virtual ranges are writable through this
API. It does not create a blanket identity map or expose recursive page tables.

The loader validates that all leaves in this window are empty, retains the PT
as EfiLoaderData and maps only the PT itself at its physical identity address
as supervisor RW/NX. All allocations and mappings precede the final memory-map
refresh and ExitBootServices. The PT address is included in physical-width
preflight, and the kernel frame allocator explicitly protects its page.

BootInfo **v3** appends `kernel_window_table: u64` at offset 88; total size is
96 bytes, alignment 8, with the original 88-byte prefix unchanged. This field
is a physical address whose single page is deliberately identity mapped by
this loader. It must be nonzero, 4096-aligned and below the low canonical
48-bit limit. The kernel checks version in the common prefix before reading
the v3 tail, and then checks CPUID physical width before touching the table.
Version 1/2 handoffs are rejected. Deploy matching loader and kernel together;
an old kernel also rejects v3. This is an explicit incompatible boot ABI update,
not an interpretation of previously reserved fields.

The kernel owns the leaf table exclusively on the boot CPU with IRQs disabled.
The API maps only vacant slots, translates, changes write permission, unmaps,
and allows remapping a slot. It checks slot indices, page alignment and CPU
physical width. Leaves are supervisor-only and always NX, using WB RAM; callers
must own the frames and maintain compatible cache aliases. MMIO mappings are
not provided. Every PTE update uses a volatile store followed by local INVLPG
with a compiler memory barrier. CR0.WP is explicitly enabled so supervisor
writes respect read-only leaves. Hardware accessed/dirty bits are preserved
when changing permissions. Unmapping does not free a physical frame.

Safe translation queries return numbers only. Mapping mutations are unsafe:
callers must retire all incompatible references before protect/unmap, initialize
new RAM before reading, and retain frame ownership until all aliases are gone.
The API must be replaced or synchronized before interrupt/SMP use. No TLB
shootdown, new address space, huge page, user page or dynamic PT allocation is
claimed. The overall virtual-memory roadmap checkbox stays open: this is a
bounded early mapping interface, not the completed general manager.

Validation includes production loader collision/empty-window tests, BootInfo
layout and version tests, production mapper malformed-input/state tests and
QEMU volatile reads/writes across two newly claimed physical frames. The boot
probe changes permissions, unmaps and remaps the same virtual address to the
other physical frame, detecting a stale TLB translation. Separate QEMU kernels
attempt a supervisor write to a read-only page and an access after unmap; each
must produce the expected CR2 and #PF present/write bits on COM1. Existing
normal, xHCI discovery, panic, breakpoint and page-fault boots remain required.

References: Intel SDM Volume 3A, four-level paging, CR0.WP and section 4.10.4
(TLB invalidation); existing Vibrix ADRs 0003/0004/0006/0007. Original code by
OpenAI Codex; no third-party implementation or new dependency.

## Observed validation

[Actions run 36343397525](https://github.com/mixutin/Vibrix/actions/runs/36343397525)
passed on head `343d72abd73d45a1a3209d397bc60083e20d3acc`: production host
checks, formatting, both target Clippy/builds and seven QEMU boots. Normal
and all probe kernels reported real map/protect/unmap/remap RAM success.
At CR2 `0xffffc00000000000`, the supervisor-write probe observed error `0x3`
(P=1,W=1,U=0), and the unmap-read probe observed error `0x0` (P=0,W=0,U=0).
These are actual CPU #PFs logged on native COM1. Target 001 is untested.

## Later UC PCI configuration-window extension (M4)

The temporary v3 leaf window's separate `map_mmio_readonly` operation
maps explicitly validated PCI ECAM pages supervisor-only, NX and PWT+PCD
for UC PAT index 3, without changing `map`'s WB RAM policy.
The caller validates that firmware's map labels the entire physical page
`EfiReservedMemoryType` or `EfiMemoryMappedIO` and permits UC, and that
the CPU's actual IA32_PAT
entry 3 is UC; it uses volatile aligned dword reads and unmaps immediately.
Mapping a page does not convey ownership of PCI devices, safe register
writes, bus mastering, DMA or interrupt routing. No new global alias to
WB RAM is accepted. This early primitive retains the single CPU/IF=0
restriction and depends on future general MMIO resource coordination.

[PR #67 exact-head run 36346144132](https://github.com/mixutin/Vibrix/actions/runs/36346144132)
observed actual MCFG-selected ECAM vendor/class reads under QEMU q35
in seven configurations. The firmware described the PCIe aperture as
reserved memory (type 0) with UC capability, and virtual xHCI increased
bus-zero function-zero devices from 4 to 5, with one native xHCI
class/prog-if match. No Target 001 or interrupt evidence is implied.
