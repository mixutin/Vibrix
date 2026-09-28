# Vibrix Roadmap

> **Security develops in parallel with functionality.** See [SECURITY_ROADMAP.md](SECURITY_ROADMAP.md) for the security gates covering supply chain, kernel memory protection, interrupts, userspace isolation, DMA/drivers, persistent storage, networking, packages and verified boot. Functional completion does not imply a security property unless the corresponding security gate has evidence.

Vibrix is an independent Rust-native Unix-like operating system that **lives on persistent USB storage**.

There is no internal-disk edition. A checkbox is completed only when functionality is implemented and demonstrated on its stated target.

## Verified abstraction and policy batch — PRs #92–#96

Authoring AI: **GPT-6 Astra Pro**. These five checkboxes have deliberately
bounded meanings; they do not imply USB persistence, real networking, SMP
execution, persisted configuration, an operational updater or Target 001 support.
The implementation evidence below precedes this documentation-only roadmap
update. Exact implementation heads and full regression results are retained in
the PR descriptions. No independent review is claimed.

| Roadmap item | Observed evidence | Completion boundary |
| --- | --- | --- |
| M7 block-device abstraction, [PR #92](https://github.com/mixutin/Vibrix/pull/92) | [Run 36408760964](https://github.com/mixutin/Vibrix/actions/runs/36408760964): five production host tests, bare-metal Clippy and real QEMU kernel write/read comparison, neighboring-sector preservation and invalid-operation rejection. | Checked synchronous block API plus exclusive RAM backend. RAM never claims durable flush. No USB/SCSI or physical block driver. See [contract](docs/BLOCK_DEVICE.md). |
| M10 NIC abstraction, [PR #93](https://github.com/mixutin/Vibrix/pull/93) | [Run 36409221073](https://github.com/mixutin/Vibrix/actions/runs/36409221073): five production host tests, target Clippy and a real post-firmware kernel trait-based frame round trip with short-buffer retry. | Bounded software loopback and NIC interface, not RTL8168, Ethernet/ARP/IP or external network traffic. See [contract](docs/NIC_ABSTRACTION.md). |
| M12 CPU enumeration, [PR #94](https://github.com/mixutin/Vibrix/pull/94) | [Run 36409539352](https://github.com/mixutin/Vibrix/actions/runs/36409539352): production MADT tests and QEMU with exactly 1, 4 and 16 distinct enabled firmware CPU records; timer IRQ and console readiness also required. | Up to 64 xAPIC/x2APIC firmware identities and availability states. Only the BSP executes Vibrix; AP startup and SMP remain unchecked. See [contract](docs/CPU_ENUMERATION.md). |
| M9 portable configuration policy, [PR #95](https://github.com/mixutin/Vibrix/pull/95) | [Run 36409812567](https://github.com/mixutin/Vibrix/actions/runs/36409812567): five host tests and QEMU kernel execution of bounded schema parsing, hardware-key rejection and current-boot network-consent policy. | Accepted [ADR 0014](docs/decisions/0014-portable-configuration.md) and executable policy. No settings are loaded from USB or applied to real devices yet. |
| M9 system update + rollback strategy, [PR #96](https://github.com/mixutin/Vibrix/pull/96) | [Run 36409989644](https://github.com/mixutin/Vibrix/actions/runs/36409989644): seven host tests, including all 32 prerequisite combinations and stale/re-staged trial tickets; QEMU kernel executes failed-trial fallback and healthy-promotion simulations. | Accepted [ADR 0015](docs/decisions/0015-update-rollback-strategy.md) and executable state model only. No real signature verifier, disk update, persistent boot selector, recovery environment or rollback reboot. |

Every QEMU proof requires independent kernel COM1 and debugcon output. Policy
model assertions are not authentication or durability evidence. Future native
storage, networking, userspace and security milestones still require their own
runtime tests; none of their checkboxes are changed by this batch.

## M0 — Bootstrap
- [x] Project identity and independence policy
- [x] Rust-native system policy
- [x] Define persistent USB-only product scope
- [x] Repository structure
- [x] Hardware inventory tooling
- [x] Sanitized Target 001 profile
- [x] AI/agent contribution policy
- [x] GitHub Actions CI definition
- [x] CI green on loader + kernel + QEMU smoke test
- [x] Adopt project license
- [x] Architecture decision record process
- [x] Preserve executable bits for scripts

## M1 — First QEMU boot
- [x] Rust UEFI application
- [x] Dependency-free UEFI console output
- [x] OVMF/QEMU development environment
- [x] First successful QEMU boot
- [x] Separate Rust kernel crate
- [x] Built-in bare-metal x86-64 Rust target
- [x] Initial linker layout
- [x] BootInfo v1 design

## M2 — Firmware-to-kernel handoff
- [x] Loader filesystem access
- [x] Locate /vibrix/kernel.elf
- [x] Vibrix-owned ELF64 parser
- [x] Validate ELF class/machine/endianness
- [x] Parse PT_LOAD headers
- [x] Allocate/copy kernel segments and zero BSS
- [x] Establish initial kernel mappings
- [x] Discover GOP framebuffer
- [x] Discover ACPI RSDP
- [x] Capture final UEFI memory map
- [x] Populate BootInfo
- [x] ExitBootServices
- [x] Transfer to vibrix_kernel_entry
- [x] Kernel framebuffer output without UEFI
- [x] Kernel panic output

**Verified M2 QEMU/OVMF handoff (PR #49):** [Actions run 36337520346](https://github.com/mixutin/Vibrix/actions/runs/36337520346) executed the UEFI loader, refreshed the final map, populated BootInfo v2, successfully exited boot services, switched to verified kernel mappings and dedicated stack, and entered the standalone higher-half kernel. The **kernel's own** debugcon markers confirm BootInfo validation, GDT/TSS initialization, native COM1 output and uncached GOP framebuffer pixel writes; the separate QEMU serial log contains `Vibrix kernel started.`. [Run 36337648665](https://github.com/mixutin/Vibrix/actions/runs/36337648665) also booted an optional panic-probe kernel and observed real post-firmware panic messages over QEMU debugcon **and** COM1. These are QEMU observations, not Target 001 physical boot or a native USB storage/filesystem/interrupts/userspace milestone. At that M2 checkpoint, full ACPI table mappings, native memory/exception subsystems, removable USB reacquisition and persistence were pending. **Since then**, PRs #52/#54 have added and independently QEMU-tested a limited single-CPU IDT, page-fault diagnostics and monotonic physical-only frame allocator; see M3 below. Full ACPI SDT/MCFG mapping, virtual memory, native USB and persistence remain pending.

**Exit:** standalone kernel prints after ExitBootServices without firmware boot services.

## M3 — x86-64 kernel foundations
- [x] Architecture module layout
- [x] CPUID discovery
- [x] Serial/debug console
- [x] GDT + TSS
- [x] IDT + exception handlers
- [x] Page-fault diagnostics
- [x] Physical frame allocator
- [ ] Virtual memory manager
- [x] Kernel heap
- [x] Local APIC + I/O APIC
- [x] Timer + interrupt routing
- [x] Explicit unsafe-code boundaries

**Verified M3 APIC/timer IRQ path and M4.5 uptime (PR #82):**
[Actions run 36380292170](https://github.com/mixutin/Vibrix/actions/runs/36380292170)
passed production MADT/APIC/IRQ host tests, both target builds/Clippy, the full
QEMU regression matrix, real keyboard console injection and supply-chain gates.
The ordinary post-firmware QEMU kernel validated MADT-selected LAPIC and I/O
APIC addresses, mapped them UC, masked the legacy PIC, programmed a PIT timer,
installed an external IRQ vector, routed IRQ0 through the I/O APIC, enabled
interrupts on the BSP, observed a native timer interrupt and issued LAPIC EOI.
The atomic tick counter then remained live while the bounded console accepted
real injected `uptime` keystrokes and returned a nonzero monotonic tick count.
This checks the **single-BSP QEMU APIC/timer foundation** and console timer
availability only. PS/2 keyboard input remains polled; there is no SMP,
preemptive scheduling, general IRQ subsystem, MSI/MSI-X, Target 001 hardware
proof, or calibrated high-resolution clock.

**Verified M3 mapping groundwork (PR #62):** [Actions run 36343397525](https://github.com/mixutin/Vibrix/actions/runs/36343397525) passed seven QEMU configurations, including actual supervisor-write and post-unmap page faults. BootInfo v3 provides one bounded 2 MiB mapping window; the kernel maps newly owned RAM frames, changes write permissions, unmaps and remaps with local TLB invalidation. CR0.WP is enabled. The general virtual-memory manager checkbox stays **unchecked**: dynamic page tables, address-space management, frame reuse and SMP shootdowns remain unfinished. See [ADR 0009](docs/decisions/0009-early-mapping-window.md).

**Verified M3 bounded early heap (PR #60):** [Actions run 36342588023](https://github.com/mixutin/Vibrix/actions/runs/36342588023) passed production heap host tests, formatting, target Clippy/builds and five QEMU boots (normal, virtual xHCI, panic, breakpoint and page fault). The real kernel allocates aligned spans, writes/reads their RAM backing, frees and reuses an allocation, and reports success independently over debugcon and COM1. This is a **64 KiB fixed-capacity early kernel heap** over reserved, already mapped BSS, with free/reuse. It is single-CPU/IRQs-off, has no global Rust allocator and cannot grow from physical frames. Virtual memory, general-purpose/SMP allocation and physical Target 001 tests remain separate work. See [early heap contract](docs/EARLY_HEAP.md).

**Verified M3 early frame allocator (PR #54):** [Actions run
36339836880](https://github.com/mixutin/Vibrix/actions/runs/36339836880)
compiled the production memory-map allocator, ran malformed-map/ownership
host tests and booted the real kernel after ExitBootServices. Its
kernel-only markers confirm initialization from the retained final UEFI v1
descriptor map and issuance of two different, nonzero, page-aligned
`EfiConventionalMemory` physical frames; both normal and panic-probe
QEMU boots passed. The global allocator is deliberately monotonic,
single-boot-CPU and interrupts-disabled. **This checkbox does not imply**
new physical frames are identity-mapped, zeroed, releasable, SMP-safe, or
available to a heap/userspace: those are separate uncompleted memory tasks.


**Verified M3 early exception handling (PR #52):**
[QEMU run 36340579141](https://github.com/mixutin/Vibrix/actions/runs/36340579141)
synchronized the IDT work with the real kernel's M2 handoff, RSDP and
physical-frame allocator and exercised four separate QEMU builds:
normal kernel, native panic probe, actual CPU `int3` breakpoint
(returning from #BP), and an actual unmapped page read triggering #PF.
The real kernel recorded CR2 = `0x10000000000`, RIP, raw error code and
decoded P/W/U/RSVD/I bits on independent COM1 output, with separate
debugcon proof. The permanent 256-gate IDT handles synchronous #BP,
#DF, #GP and #PF; only #BP and #PF were fault-injected in QEMU.
**Interrupt flag remains cleared** and unconfigured IRQ vectors, APIC,
double-fault IST/privilege stacks, ring-3 exceptions, timer routing
and SMP handling are separate unchecked work. Target 001 is untested.

## M4 — Device discovery
- [x] ACPI parser
- [x] MCFG/ECAM
- [x] PCI enumeration
- [x] BAR parsing
- [ ] MSI/MSI-X
- [x] Device/driver model
- [x] Driver binding

**Verified M4 driver binding registry (PR #90):**
[Actions run 36388347347](https://github.com/mixutin/Vibrix/actions/runs/36388347347)
passed both required jobs on the synchronized post-M4.5 head. Host tests exercised
exclusive device ownership, duplicate-binding rejection, unknown-device
rejection, and fixed-capacity overflow. The native post-firmware PCI scan then
committed every matched device identity to the bounded driver registry. Normal
QEMU required the independent binding-registry-ready marker, while the explicit
virtual-xHCI boot required at least one live xHCI binding and **zero binding
failures** through the same discovered PCI identities.

This checkbox means **device-to-driver ownership association** is implemented;
it does not activate any device. Binding performs no PCI configuration writes,
BAR MMIO, bus mastering, DMA, MSI/MSI-X programming, USB transactions,
Ethernet I/O, interrupt setup, or Target 001 hardware access. Native xHCI and
RTL8168 initialization remain later driver milestones.

**Verified M4 device/driver candidate model (PR #87):**
[Actions run 36384062012](https://github.com/mixutin/Vibrix/actions/runs/36384062012)
passed the production device-model host tests, both target builds/Clippy,
timer-enabled QEMU regressions, real keyboard console probes and supply-chain
gates on the synchronized implementation head. Native post-firmware PCI
enumeration populated immutable PCI identities and a fixed driver-descriptor
registry; normal QEMU required the independent kernel device-model marker, and
the explicit virtual-xHCI boot observed at least one **live xHCI driver
candidate** through the same discovered PCI data. The model also contains an
exact RTL8168 identity rule for future Target 001/network work.

This checkbox means Vibrix now has a bounded **discovery and driver-candidate
model**, not driver ownership or hardware activation. No PCI configuration
writes, BAR MMIO, bus mastering, DMA, MSI/MSI-X, USB transactions or Ethernet
I/O occur through this model. **Driver binding stays unchecked**, as do native
xHCI/RTL8168 drivers and every Target 001 hardware item.

**Verified M4 read-only ECAM bootstrap (PR #67):**
[Actions run 36346144132](https://github.com/mixutin/Vibrix/actions/runs/36346144132)
passed formatting, production UEFI-map, ACPI, ECAM and virtual-mapping
host tests, both target Clippy/builds and seven post-firmware QEMU boots.
The native kernel used a checksummed real MCFG allocation for
**segment-zero bus-zero function-zero configuration reads**, mapping each
4 KiB ECAM page supervisor-read-only/NX/UC for aligned volatile vendor
and class dwords, then unmapping. It verified the CPU's actual PAT index 3
and accepted only UEFI reserved or MMIO pages advertising UC capability,
not conventional/loader/ACPI RAM or runtime memory. QEMU OVMF labels this
ECAM aperture `EfiReservedMemoryType` (type 0), UC attribute 1.
QEMU's native COM1 reported **4 function-zero devices** on bus zero
without a virtual xHCI, or **5 devices and 1 xHCI controller** when
`qemu-xhci` was added; seven debugcon boot configurations passed.
This checkbox is **bounded MCFG-selected ECAM read-only discovery**,
not full 256-bus/multifunction/multisegment scanning, PCIe extended
register enumeration, resource sizing, configuration writes, MSI,
native xHCI activation, DMA, interrupts or physical Target 001 support.

**Verified M4 real ACPI SDT parsing (PR #64):** [Actions run
36344773356](https://github.com/mixutin/Vibrix/actions/runs/36344773356)
passed production ACPI/firmware-map host tests, formatting, both target
Clippy/builds and seven post-firmware QEMU configurations. The real
kernel mapped retained WB ACPI RAM as supervisor-read-only/NX in BootInfo
v3's temporary mapping window, checked the full XSDT and child SDT
checksums and parsed **one actual MCFG allocation** in QEMU q35.
Independent kernel debugcon and COM1 confirmed it on normal, virtual
xHCI, panic, breakpoint, page fault and VM protection/unmap probes.
Each table is bounded to 1 MiB; at most 64 XSDT/RSDT entries are read
in this early bootstrap. This is real, limited ACPI firmware table
discovery, not AML interpretation or general ACPI namespace support.
**MCFG/ECAM stays unchecked:** the discovered ECAM memory is only a
physical number, not mapped PCI configuration MMIO or a driver.
No MSI, APIC routing, SMP or Target 001 claims follow from this.

**Verified M4 native PCI segment-zero scan (PR #59):** [Actions run
36341931987](https://github.com/mixutin/Vibrix/actions/runs/36341931987)
compiled the production read-only PCI mechanism-#1 scanner and BAR decoder,
passed host fixtures for multifunction buses, absent devices and 32-/64-bit
BAR pairs, and ran the actual post-ExitBootServices kernel under QEMU q35.
Independent kernel debugcon markers and COM1 observed **6 PCI functions,
9 assigned BARs, 0 xHCI controllers** on that particular QEMU setup, in
normal and exception-probe regression boots. A second QEMU boot with an
explicit `qemu-xhci` PCI controller exercised the same native scanner and
observed **7 PCI functions, 10 assigned BARs and 1 xHCI controller** in
[run 36342144203](https://github.com/mixutin/Vibrix/actions/runs/36342144203).
This M4 enumeration checkbox
is **legacy PCI segment zero on x86-64**, not other PCI segments or extended
configuration space: ACPI MCFG/ECAM, MSI/MSI-X, driver binding, BAR resource
sizing/MMIO activation, a native xHCI driver and Target 001 remain separate
unchecked tasks. Reading an assigned BAR is not using its memory or
writing an internal disk.


## M4.5 — Interactive kernel console

This is an intentionally small bridge between device discovery and the real
M5/M6 userspace stack. It exists so Vibrix becomes directly operable during
kernel development; it does **not** replace Ring 3, syscalls, VFS, TTY or the
future userspace shell.

- [x] Hardware interrupt path usable in QEMU
- [x] Monotonic timer source available to the console
- [x] QEMU keyboard input reaches the kernel without UEFI Boot Services
- [x] Kernel console input buffer and line editing
- [x] Command parser and dispatch table
- [x] `help`
- [x] `clear`
- [x] `info` / build information
- [x] `mem` memory diagnostics
- [x] `pci` PCI discovery output
- [x] `acpi` ACPI discovery output
- [x] `uptime`
- [x] `reboot`
- [x] Unknown-command and malformed-input handling
- [x] QEMU smoke test proves prompt → input → command → output

**Verified M4.5 QEMU keyboard input (PR #70):**
[Actions run 36347623002](https://github.com/mixutin/Vibrix/actions/runs/36347623002)
compiled and host-tested the first-party PS/2 scan-code decoder and built
the production post-firmware keyboard polling path. QEMU's host monitor
sent **real virtual `h` and Return keypresses** only after the independent
kernel-side `VIBRIX: kernel PS2 polling ready` marker; the kernel's
native COM1 logged two *distinct exact lines*,
`kernel PS2 ascii 104` and `kernel PS2 ascii 10`.
The initial test incorrectly accepted `104` as a prefix match for
`10`, and that false-positive was corrected before claiming this item.
The successful kernel/QEMU job also passed normal/xHCI boots and all fault
probes. This is a **polled QEMU i8042 set-one ASCII subset** on the single
CPU with IF=0, not USB HID, IRQ-driven keyboard input, line editing,
a command parser, TTY, userspace shell or Target 001 PS/2 hardware.

**Verified M4.5 bounded interactive console (PR #80):**
[Actions run 36375400040](https://github.com/mixutin/Vibrix/actions/runs/36375400040)
passed the synchronized production console host tests, target Clippy/builds,
supply-chain checks, the full QEMU regression matrix, and a feature-specific
real-keyboard interaction. QEMU waited for the kernel-origin `vibrix> `
readiness marker, injected `helx`, Backspace, `p`, Return through HMP,
and the kernel independently reported the accepted backspace, exact `help`
dispatch, and COM1 output `commands: help info`. The fixed 80-byte ASCII
line editor rejects overflow without writing past its buffer, ignores unsupported
control bytes, resets after submission, and the parser matches whole commands;
`info` and unknown-command behavior are host-tested through the same production
module. These checkboxes mean a bounded **polled PS/2 development console**,
not a TTY, IRQ keyboard, framebuffer terminal, userspace shell, USB HID path,
or Target 001 console. The later control/diagnostic completion below is
separately evidenced and does not expand this original PR #80 claim.

**Verified M4.5 remaining diagnostic/control commands (PR #89):**
[Actions run 36386907847](https://github.com/mixutin/Vibrix/actions/runs/36386907847)
passed formatting, production host tests, target Clippy/builds, the supply-chain
gate and the complete post-firmware QEMU regression matrix. The feature-specific
probe waited for the real kernel prompt and injected edited `help`, `clear`,
`mem`, `pci`, `acpi`, `uptime` and `reboot` through QEMU's virtual
keyboard. Independent debug markers proved exact dispatch; COM1 contained the
literal ANSI clear-screen/home bytes plus bounded immutable memory, PCI and
ACPI snapshots. The reboot proof ran QEMU with `-no-reboot` and required it to
terminate before the timeout after the kernel issued the i8042 reset pulse, so
a log-only reset stub would fail. The reset mechanism is a q35 development
path only. These checkboxes do **not** claim Target 001 reset support, USB HID,
a TTY/userspace shell, live post-IRQ mutation of the early allocator/mapping
window, driver activation, filesystem access or persistent USB root.

**Exit:** after `ExitBootServices`, QEMU reaches a `vibrix>` prompt, accepts
real keyboard input and executes diagnostic commands entirely in the Vibrix
kernel. The console is a development milestone only; the M6 Rust shell remains
the first real userspace CLI.


### Target 001
- [ ] AMD xHCI 1022:43ee
- [ ] AMD xHCI 1022:149c
- [ ] Realtek Ethernet 10ec:8168
- [ ] Navi 23 GPU 1002:73ff
- [ ] Samsung NVMe 144d:a808 as optional data device
- [ ] AMD AHCI 1022:43eb as optional data device

## M5 — Processes and syscalls
- [ ] Kernel threads
- [ ] Context switching
- [ ] Preemptive scheduler
- [ ] Ring 3 userspace
- [ ] Userspace address spaces
- [ ] Vibrix syscall ABI v1
- [ ] syscall/sysret
- [ ] Native Rust syscall library
- [ ] PID/process lifecycle
- [ ] Executable loading
- [ ] argv/environment
- [ ] wait/exit

**Exit:** PID 1 executes in userspace and makes Vibrix syscalls.

## M6 — VFS and early userspace
- [ ] File descriptors
- [ ] VFS
- [ ] In-memory bootstrap filesystem
- [ ] /dev
- [ ] pipes
- [ ] TTY
- [ ] Rust init
- [ ] Rust shell
- [ ] Core utilities: cat, echo, ls, pwd, cd, mkdir, cp, mv, rm, ps, kill

**Exit:** boot to an interactive Vibrix userspace shell.

## M7 — USB platform
- [ ] xHCI initialization
- [ ] USB device enumeration
- [ ] USB hub support
- [ ] USB HID keyboard
- [ ] USB HID mouse
- [ ] USB mass-storage transport
- [ ] SCSI transparent command subset for mass storage
- [x] Block-device abstraction
- [ ] Detect the boot USB device robustly
- [ ] Read/write blocks on the Vibrix USB device

**Exit:** Vibrix can access the same removable USB device it booted from after leaving firmware services.

## M8 — Vibrix filesystem
- [x] On-disk specification
- [x] Superblock/allocation metadata
- [x] files/directories
- [x] permissions/timestamps

**Verified M8 files/directories wire behavior (PR #83):**
[Actions run 36375799757](https://github.com/mixutin/Vibrix/actions/runs/36375799757)
passed the shared directory-record codec tests, regular-file formatter/inspector
round trips for both 512- and 4096-byte logical-sector models, all existing
metadata-corruption checks, target builds/Clippy and the full QEMU regression
matrix. The host formatter creates root inode 1 with exact `.`, `..` and
`welcome.txt` records, allocates inode 2 as a regular file on a distinct
bitmap-owned data block, and writes a bounded payload. The independent inspector
decodes the directory framing through the shared wire codec, follows the inode
reference, requires file-type and allocation agreement, verifies the distinct
extent and exact payload/zero tail, and rejects malformed UTF-8/name, slash/NUL,
record alignment, reserved padding, inode-range and type fields. This checkbox
means **VibrixFS v1 on-disk file/directory representation is implemented and
demonstrated in bounded regular-file images**. It does not claim a kernel VFS,
runtime mutation, crash consistency/recovery, USB block I/O, persistent root or
Target 001 filesystem behavior.

**Verified M8 permissions/timestamps metadata (PR #85):**
[Actions run 36379311357](https://github.com/mixutin/Vibrix/actions/runs/36379311357)
passed the shared VibrixFS wire tests, regular-file formatter/inspector tests,
both target builds/Clippy, the complete QEMU regression matrix and supply-chain
gates on the synchronized implementation head. For both 512- and 4096-byte
logical-sector models the formatter wrote a named regular file with mode
`0640`, uid/gid `1000:1000`, and distinct atime/mtime/ctime values including
nanoseconds; the inspector independently parsed the checksummed inode and
required every value, and CI asserted the exact reported metadata. Production
wire tests also checked the fixed little-endian offsets and round trip.
This checkbox means **VibrixFS on-disk permission/ownership/timestamp metadata
is encoded and validated**. It does not mean kernel credential enforcement,
wall-clock acquisition, multi-user security, VFS mutation or USB persistence.

- [x] crash-consistency design
**Adopted M8 crash-consistency design (ADR 0011):** VibrixFS writable
metadata will use a bounded full-block redo journal with one transaction owner,
ordered new-data writes, explicit durability barriers, a checksummed commit
record, idempotent home-block replay, and secondary-then-primary generation
checkpoints. Torn or incomplete committed journal state fails closed instead of
partially replaying. Freed blocks are not reusable until checkpoint retirement,
and a kernel writable mount is forbidden until the native block/USB path can
provide real cache-flush ordering. This checkbox records the **accepted design
contract only**. Journal record implementation, interrupted-write tests,
formatter/recovery tooling, VFS integration and USB persistence remain
unchecked.
- [ ] formatter + recovery tool
- [ ] VFS driver
- [ ] persistent root mounted from USB

**Verified M8 bounded on-disk metadata foundation (PR #81):**
[Actions run 36370905041](https://github.com/mixutin/Vibrix/actions/runs/36370905041)
passed the shared v1 wire-codec tests, the real regular-file formatter/inspector
round trip for both **512-byte and 4096-byte logical-sector models**, both target
Clippy/builds, supply-chain checks and the existing QEMU kernel matrix. The
formatter creates a new file only, refuses overwrite/raw-device-looking paths,
writes reciprocal 4 KiB superblocks plus block/inode bitmaps, a fixed inode
table, root inode 1 and its allocated root-directory block; the inspector
independently re-reads and validates checksums, externally supplied partition
identity, geometry, mandatory/padding allocation bits and the root inode/data
reference. CI also mutates reserved superblock bytes and requires rejection.
This verifies the byte-level **VibrixFS v1 on-disk specification** and its
initial superblock/allocation metadata on host regular files. It does **not**
establish general file/directory mutation, permissions enforcement, crash
consistency/recovery, a kernel VFS driver, native USB block I/O, persistent root,
or physical provisioning. The combined formatter + recovery item therefore
remains unchecked.

**Exit:** files created under the Vibrix root filesystem survive shutdown and reboot.

## M9 — Persistent USB operating system
- [x] GPT tooling

**GPT tooling evidence (host only):** The read-only `tools/inspect-gpt.rs`
validates both GPT copies, metadata, unique GUIDs and overlaps. The
new `tools/create-usb-image.rs` creates only a fresh **regular-file**
protective-MBR + reciprocal GPT image with distinct disk/partition GUIDs,
blank ESP and data partition placeholders, and no existing-file overwrite.
CI tests both 512/4096-byte sectors by passing each generated image through
the independent inspector. This checkbox means **offline GPT tooling**,
not a bootable ESP, formatted root, USB device provisioning or QEMU native
USB persistence. Those remain separate unchecked M9/M7 tasks.

- [x] EFI System Partition layout

**Verified M9 EFI System Partition layout (PR #91):**
[Actions run 36388232003](https://github.com/mixutin/Vibrix/actions/runs/36388232003)
passed both required jobs. Host tests created fresh regular-file GPT images for
both 512- and 4096-byte logical-sector models, populated the adopted 32 MiB ESP
as FAT16, independently re-read both FAT copies and the exact
`/EFI/BOOT/BOOTX64.EFI` plus `/VIBRIX/KERNEL.ELF` short-name tree, and
proved a second population attempt fails closed on a non-blank ESP. The runtime
proof populated a GPT image with the production Vibrix UEFI loader and kernel,
attached it **read-only** to QEMU q35/OVMF as virtual USB mass storage, observed
firmware entry through `BOOTX64.EFI`, `kernel.elf` discovery,
`ExitBootServices`, and standalone kernel entry, then required the image
SHA-256 to remain unchanged. This checkbox proves the regular-file **ESP
on-media layout and firmware boot path** only. It does not claim native
post-EBS USB mass-storage I/O, persistent root, physical-device provisioning,
Target 001 boot, update/recovery behavior or safe writes to a real USB device.
- [x] Vibrix USB system partition layout
- [ ] Persistent root

**Verified M9 removable system partition layout (PR #88):**
[Actions run 36385035224](https://github.com/mixutin/Vibrix/actions/runs/36385035224)
passed both required jobs on the synchronized implementation head. The
regular-file-only GPT creator generated 512- and 4096-byte-sector images with
exactly one standard 32 MiB ESP and one 1 MiB-aligned **Vibrix System**
partition using project type GUID
`2e4a0f3b-6a3d-4e96-b99a-553d7c0b1201`. The independent read-only GPT
inspector recognized exactly one of each type while retaining reciprocal
primary/backup CRC, unique-GUID, overlap and Protective MBR validation.

This checkbox records the outer removable-system **GPT partition role and
geometry only**. The Vibrix System partition is still unformatted, and the ESP
is not yet populated as a persistent disk image. There is no native USB I/O,
physical-device provisioner, persistent root, Target 001 boot or two-machine
portability claim. Runtime root selection must still use ADR 0005's disk/ESP/
root unique-GUID identity checks; a partition type alone never selects a disk.
- [ ] Persistent /home
- [ ] Persistent package database
- [ ] RAM-backed /tmp and runtime state
- [ ] Flash-write reduction
- [ ] Hardware rediscovery every boot
- [x] Portable configuration policy
- [ ] Safe USB provisioning/imaging tool
- [ ] Recovery partition/environment
- [x] System update + rollback strategy
- [ ] Target 001 real USB boot
- [ ] Move the same USB drive between two compatible machines

**Exit:** boot from USB, modify files/configuration/apps, power off, move or reboot the drive, and retain all state without touching an internal system disk.

## M10 — Networking
- [x] NIC abstraction
- [ ] RTL8168-family driver
- [ ] Ethernet + ARP
- [ ] IPv4 + ICMP
- [ ] UDP
- [ ] DHCP
- [ ] DNS
- [ ] TCP
- [ ] sockets
- [ ] network utilities

## M11 — Security and multi-user
- [ ] users/groups/credentials
- [ ] permissions
- [ ] secure random
- [ ] W^X
- [ ] userspace ASLR
- [ ] stack protections
- [ ] IOMMU
- [ ] secure updates
- [ ] optional USB system encryption design

## M12 — SMP and performance
- [x] CPU enumeration
- [ ] AP startup
- [ ] per-CPU structures
- [ ] SMP scheduler
- [ ] synchronization
- [ ] TLB shootdowns
- [ ] profiling
- [ ] Target 001 8C/16T validation

## M13 — Audio and graphics
- [ ] HDA + basic PCM
- [ ] USB audio
- [ ] graphics architecture
- [ ] framebuffer userspace API
- [ ] compositor/display-server design
- [ ] Navi 23 modesetting research
- [ ] native modesetting
- [ ] acceleration
- [ ] GUI toolkit

## M14 — Packages and development
- [ ] Package format/database/dependencies
- [ ] package manager
- [ ] signed repositories
- [ ] ports/build recipes
- [ ] editor/developer tooling
- [ ] compiler bootstrap plan

## M15 — Self-hosting
- [ ] Compile a Rust userspace program on Vibrix
- [ ] Toolchain usable on Vibrix
- [ ] Build userspace on Vibrix
- [ ] Build kernel on Vibrix
- [ ] Produce a bootable Vibrix USB image from Vibrix

## Optional later storage support

Internal NVMe/SATA disks may be supported as user-accessible **data devices**. They are not Vibrix root/system installation targets.

## Future
- [ ] aarch64 design
- [ ] aarch64 UEFI boot
- [ ] architecture-independent driver boundaries

## Early non-goals

Do not prioritize internal-disk installation, desktop polish, browsers, GPU acceleration, broad hardware support, POSIX completeness or Linux binary compatibility before the USB-root kernel foundation is reliable.
