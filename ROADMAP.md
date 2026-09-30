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
- [x] Virtual memory manager
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

**Verified M3 bounded runtime virtual-memory manager (PR #122):**
[Managed VM run 36437875475](https://github.com/mixutin/Vibrix/actions/runs/36437875475)
and [CI run 36437875790](https://github.com/mixutin/Vibrix/actions/runs/36437875790)
passed on the exact implementation head. Building on the earlier managed-VM
slices, the production kernel dynamically creates and reclaims owned page-table
levels and data mappings, enforces W^X-representable supervisor permissions,
provides guarded allocations, and keeps a dedicated 96-frame managed pool alive
after APIC activation. Runtime mutation owns one scratch mapping slot, masks and
restores local interrupts, and was demonstrated by holding a guarded mapping
across a real PIT interrupt, validating its contents afterward, unmapping it and
recovering every pool frame.

This checks the M3 **single-BSP kernel virtual-memory manager** boundary. It is
one kernel CR3 and does not claim Ring 3/user address spaces, demand paging, a
growing general heap, global physical-frame reclamation, SMP locking or TLB
shootdowns. Those remain M5/M12 work; Target 001 remains untested.

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
- [x] MSI/MSI-X
- [x] Device/driver model
- [x] Driver binding

**Verified M4 MSI/MSI-X programming and QEMU delivery (PRs #107–#112, #117):**
The production conventional-capability walker validates bounded MSI/MSI-X
layouts before any activation. The MSI path programs one physical xAPIC
message with width-correct PCI configuration writes, checked readback and
enable-last ordering; the MSI-X path validates independently supplied memory
BAR extents, masks the function and every table entry before programming,
readback-verifies the selected entry and unmasks only after publication.

[PR #112](https://github.com/mixutin/Vibrix/pull/112) provided actual QEMU EDU
MSI evidence: [run 36416398619](https://github.com/mixutin/Vibrix/actions/runs/36416398619)
raised two real device interrupts, acknowledged the device and LAPIC, disabled
MSI with readback, proved a new pending event stayed silent across real PIT
ticks, cleared the source and bus-master permission, then continued to the
kernel console.

[PR #117](https://github.com/mixutin/Vibrix/pull/117) adds the matching MSI-X
proof with QEMU `ivshmem-doorbell`. A separate host process supplies one real
eventfd through QEMU's ivshmem protocol and triggers it only after guest-origin
state markers. [Run 36421244213](https://github.com/mixutin/Vibrix/actions/runs/36421244213)
passed production contracts, ordinary inventory/MSI regressions and the native
MSI-X job: exactly two eventfd-driven interrupts were observed on vector
`0x51`; MSI-X was then disabled with checked readback, bus mastering was
cleared, a third host eventfd trigger produced no guest interrupt across five
PIT ticks, and normal console boot continued.

This checkbox means **bounded MSI and MSI-X discovery, safe programming
contracts and real single-BSP QEMU delivery/disable evidence**. It does not
claim a generic vector allocator, hotplug teardown, SMP/x2APIC interrupt
remapping, IOMMU isolation, production device drivers, physical Target 001
interrupt delivery or arbitrary hardware MSI-X support.

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
- [x] Kernel threads
- [x] Context switching
- [x] Preemptive scheduler
- [x] Ring 3 userspace
- [x] Userspace address spaces
- [x] Vibrix syscall ABI v1
- [x] syscall/sysret
- [x] Native Rust syscall library
- [x] PID/process lifecycle
- [x] Executable loading
- [x] argv/environment
- [x] wait/exit

**Verified M5 cooperative kernel threads and context switching (PR #118):**
[Actions run 36422864624](https://github.com/mixutin/Vibrix/actions/runs/36422864624)
passed production host tests and a real post-ExitBootServices QEMU execution
using the first-party x86-64 context-switch routine. Two independent 16 KiB
kernel stacks executed guest-origin markers in the exact required order
`A1 → B1 → A2 → B2`, then returned to the saved boot context. The proof
requires **five real RSP/callee-saved context switches** (boot→A, A→B, B→A,
exited A→B, exited B→boot), rejects missing/duplicate/reordered markers, and
COM1 reports `switches=5 completed=2`.

The scheduler is fixed-capacity and cooperative: up to four kernel threads,
static supervisor RW/NX stacks zeroed before reuse, one shared kernel CR3,
one BSP and IF=0. Public scheduler entry points reject IF=1; no Rust scheduler
reference survives the raw assembly stack switch. This completes the **kernel
threads** and **context switching** items only. Timer preemption, IRQ-safe
scheduler synchronization, SMP, RSP0/IST privilege stacks, Ring 3, userspace
address spaces, syscalls and process lifecycle remain unchecked.

**Verified M5 timer-driven preemptive scheduler (PR #119):**
[Actions run 36424156362](https://github.com/mixutin/Vibrix/actions/runs/36424156362)
ran both the cooperative context-switch proof and a dedicated preemptive timer
job on the exact implementation head. Two kernel threads make progress without
calling `yield_now()`; the guest-origin sequence requires PIT delivery followed
by `A1 → B1 → A2 → B2`, at least three timer-attributed preemptions, at least
six total context switches, exactly two completed threads, and a working kernel
console afterward. The timer handler records the tick and sends LAPIC EOI before
switching away, and resumption returns through the suspended interrupt handler
and its normal IRETQ epilogue.

This completes the **single-BSP kernel preemptive scheduler** item only. Threads
still share one kernel CR3 and there are no priorities, sleep/wakeup queues,
SMP run queues, FPU/XSAVE ownership, Ring 3, userspace address spaces, syscall
ABI or process lifecycle yet.

**Verified M5 bounded Ring 3 entry and kernel-stack return (PR #125):**
[Actions run 36449700126](https://github.com/mixutin/Vibrix/actions/runs/36449700126)
passed the full repository CI on implementation head `a30ba8ad4c71373c8adb1a1b86d6f4900ff4fb31`,
including a dedicated real-QEMU CPL3 proof. The kernel creates guarded user RW
mappings, writes a fixed `int 0x80; ud2` probe, changes the code page to RX,
enters CPL3 with CS=`0x1b` and SS=`0x23` through an `IRETQ` frame, then
takes the DPL3 diagnostic interrupt gate back to CPL0 on the dedicated TSS RSP0
stack. QEMU requires independent debugcon and COM1 markers proving the user
selectors and RSP0 trap path.

This checks the bounded **Ring 3 userspace execution foundation** only. The
probe still shares the kernel CR3 and managed VM arena; userspace address
spaces, Vibrix syscall ABI, `syscall/sysret`, copy-in/out, process lifecycle,
normal return-to-user scheduling, SMP and Target 001 remain separate unchecked
work.

**Verified M5 independent userspace address-space foundation (PR #126):**
[Actions run 36456501893](https://github.com/mixutin/Vibrix/actions/runs/36456501893)
passed the full repository CI on implementation head
`d62f470854232e26f5dd7f9b2ecfede222be0556`, including the dedicated
post-ExitBootServices QEMU proof. The kernel reserves a distinct PML4 root and
private managed-VM frame pool, copies only supervisor kernel root mappings,
builds guarded user code/stack mappings beneath a private arena slot, switches
CR3 with local interrupts masked, and enters CPL3. The DPL3 diagnostic trap
returns through the permanent TSS RSP0 stack while QEMU verifies that the exact
private CR3 remains active. The same exact-head run also passed the existing
Ring 3, write-protection, unmap-fault, host, Clippy, build and supply-chain
regressions.

This checks the bounded **userspace address-space foundation** only: one
diagnostic single-BSP address space with shared supervisor kernel mappings. It
does not yet provide a process address-space allocator, scheduler-integrated
CR3 switching, PCID/SMP TLB shootdowns, copy-in/copy-out, the Vibrix syscall
ABI, `syscall/sysret`, executable loading, or PID lifecycle.

**Verified M5 Vibrix syscall ABI v1 contract (PR #129):**
[Actions run 36458069736](https://github.com/mixutin/Vibrix/actions/runs/36458069736)
passed the full repository CI on implementation head
`a7611798928f8af9737a3cf3537d93a5a72512e4`. The shared contract freezes ABI
version 1, RAX syscall-number/result semantics, the six x86-64 argument
registers, syscall numbers 0 through 8, the bounded negative errno window, and
lower-half userspace pointer-range validation. Canonical CI formats, compiles
with warnings denied, and executes the exact shared ABI source as a host test.

This checks the **ABI contract** only. It does not by itself provide the
instruction transport, dispatcher, copy-in/copy-out, native userspace wrappers,
PID lifecycle, executable loading, file descriptors, or PID 1.

**Verified M5 x86-64 SYSCALL/SYSRETQ transport (PR #131):**
[Actions run 36468101302](https://github.com/mixutin/Vibrix/actions/runs/36468101302)
passed the full repository CI on implementation head
`1f0a8ea9a836845e91b4620af89c21aa2b5afb0d`. Production-linked host tests
bind STAR selector derivation to the live GDT, and the dedicated real-QEMU proof
programs EFER/STAR/LSTAR/FMASK, enters through a real CPL3 `SYSCALL`, switches
to a dedicated 16 KiB CPL0 syscall stack, returns ABI v1 `NotSupported`, then
executes `SYSRETQ` back to CPL3 under the same private userspace CR3. The final
DPL3 diagnostic trap requires the observed syscall entry and reports the
SYSRET-compatible user selectors CS=`0x23`, SS=`0x1b`.

This completes the bounded **syscall/sysret transport** item only. It remains a
single-BSP proof with no general dispatcher, copy-in/copy-out, per-process
kernel stacks, scheduler-integrated process address spaces, PID lifecycle,
executable loading, or PID 1.

**Verified M5 native Rust syscall library (PR #133):**
[Actions run 36472077028](https://github.com/mixutin/Vibrix/actions/runs/36472077028)
passed the full repository CI on implementation head
`bf2e6c082d35e73bb5ef63ad42074ccbb9504868`. The `no_std`
`vibrix-syscall` crate consumes the shared ABI v1 source of truth, emits the
x86-64 `syscall` instruction with the exact RAX/RDI/RSI/RDX/R10/R8/R9
register contract, declares RCX/R11 clobbers, and provides named wrappers for
exit, yield, getpid, read, write, open, close, wait and exec. Safe wrappers use
Rust slices/references where pointer lifetime can be represented; nested
argv/envp pointers remain explicitly unsafe. CI host-tests wrapper policy
without invoking the host OS ABI and checks the crate for
`x86_64-unknown-none`.

This checks the **native Rust syscall library** item only. General syscall
semantics, copy-in/copy-out, PID/process lifecycle, executable loading,
argv/environment handling, wait/exit process behavior and PID 1 remain
separate work.

**Verified M5 bounded PID/process lifecycle (PR #148):**
[Actions run 36480021255](https://github.com/mixutin/Vibrix/actions/runs/36480021255)
passed the full repository CI on implementation head
`9e00006842aac08acb0728305b45dd4c030c3698`. The fixed-capacity process
table reserves PID 1, allocates monotonically increasing PIDs, validates
parent/child relationships, retains zombie exit status, supports wait-any and
wait-specific selection, reparents orphans to a live PID 1, and reuses storage
without reusing a PID. The exact-head QEMU kernel executes the production
self-test after ExitBootServices and requires the independent process-lifecycle
marker; host tests cover capacity/PID exhaustion, invalid transitions,
reparenting, pending waits and stale/reaped identities.

This checks the bounded **PID/process lifecycle model** only. Scheduling a
general user process, loading its ELF into a private lower-half CR3, syscall
copy-in/copy-out, and syscall-visible wait/exit remain separate work.

**Verified M5 argv/environment entry-stack contract (PR #144):**
[Actions run 36478650105](https://github.com/mixutin/Vibrix/actions/runs/36478650105)
passed the full repository CI on implementation head
`81d41624926e688f2ad80f050d18e1ea8ccd4f83`. The bounded userspace-entry
builder validates argument/environment counts and byte budgets, copies strings
into the userspace stack image, constructs null-terminated argv/envp pointer
vectors, aligns the final stack, and rejects overflow or out-of-range layouts
before exposing an entry frame. Host tests exercise success and fail-closed
cases on the production implementation.

This checks the **argv/environment construction contract** only. It does not
claim a completed executable loader, persistent process image replacement,
general userspace allocator, or PID 1.

**Verified M5 bounded PID/process lifecycle (PR #148):**
[Actions run 36480021255](https://github.com/mixutin/Vibrix/actions/runs/36480021255)
passed the full repository CI on implementation head
`9e00006842aac08acb0728305b45dd4c030c3698`. The kernel owns a bounded process
table with stable PIDs, parent identity, lifecycle states, exit status,
generation-safe slot reuse and fail-closed transition rules. Production-linked
host tests and the QEMU self-test exercise creation, running/exited transitions,
lookup, reuse and invalid-state rejection.

This checks the **bounded PID/process lifecycle model** only. Scheduler-integrated
process execution, executable image replacement, wait/exit syscall semantics
and PID 1 execution remain separate work.

**Verified M5 validated ELF execution (PR #160):**
[Actions run 36490303799](https://github.com/mixutin/Vibrix/actions/runs/36490303799)
passed the full exact-head matrix on
`ef780dbc298a33bf92eb40d9d7f7db75c6cd969b`. The production userspace ELF
validator stages PT_LOAD segments into the independently owned lower-half
address space, zeroes BSS, commits final W^X permissions, enters the validated
ELF entry at CPL3 under the private CR3, and still completes the real
SYSCALL/SYSRETQ round trip. The same run passed the existing protection,
unmap, host, Clippy, build and dependency gates.

This checks **Executable loading** as the bounded x86-64 ELF64 loading
foundation. Filesystem-backed exec, general process replacement and normal
multi-process scheduling remain later work.

**Verified M5 wait/exit syscall lifecycle (PR #161):**
[Actions run 36492985708](https://github.com/mixutin/Vibrix/actions/runs/36492985708)
passed the full repository CI on implementation head
`c5941b53689aa6fafdb03e052cb15edd907c51ef`. The real CPL3 syscall proof
executes `getpid`, waits on a zombie child, copies the exit status back through
the validated private userspace CR3, reaps only after successful copy-out, then
terminates PID 1 through the real `exit` syscall. QEMU requires exact debugcon
and COM1 evidence for getpid, wait/reap and terminal exit behavior.

This completes the bounded single-BSP **wait/exit** milestone. General
multi-process scheduling, fork/clone semantics, signals and SMP process
coordination remain later work.

**Exit:** PID 1 executes in userspace and makes Vibrix syscalls.

## M6 — VFS and early userspace
- [x] File descriptors
- [x] VFS
- [x] In-memory bootstrap filesystem
- [x] /dev
- [x] pipes
- [x] TTY
- [x] Rust init
- [x] Rust shell
- [x] Core utilities: cat, echo, ls, pwd, cd, mkdir, cp, mv, rm, ps, kill

**Verified M6 bounded bootstrap filesystem and streams (PR #135):**
[Actions run 36474454756](https://github.com/mixutin/Vibrix/actions/runs/36474454756)
passed the full repository CI on implementation head
`3c590d2797f9542f2a284b788acd5f6f6893e5e9`, plus the independent nightly
torture workflow. Production safe Rust now supplies a bounded VFS/mount
namespace, volatile RAM files/directories, shared open descriptions and file
descriptors, mounted `/dev/null` and `/dev/zero`, and nonblocking bounded
pipes. Host tests cover stale handles, path traversal, capacity rollback,
shared/independent offsets, descriptor reuse, device semantics and pipe
wrap/full/EOF/broken-reader behavior. The exact-head QEMU proof uses real PS/2
keyboard input to create and read retained RAM data, enumerate `/dev`, perform
a pipe round trip, remove the file, observe NotFound, and reboot. The slash
scan-code path required by absolute filesystem paths is production-tested too.

These check **File descriptors, VFS, In-memory bootstrap filesystem, /dev and
pipes** only. The implementation is volatile and kernel-owned; it is not a
userspace TTY or shell, has no file-syscall dispatcher/process ownership, and
does not mount VibrixFS or write the boot USB. TTY, Rust init/shell, core
userspace utilities and persistent storage remain separate work.

**Verified M6 bounded /dev/tty line discipline (PR #147):**
[Actions run 36480159539](https://github.com/mixutin/Vibrix/actions/runs/36480159539)
passed the full repository CI on implementation head
`ea24f4b780fedc92ca1da16216db777b82210393`. The bootstrap devfs now exposes
a stateful `/dev/tty` with canonical line buffering, Backspace/Delete and
Ctrl-U editing, bounded input/output queues, fail-before-partial-write capacity
handling, normal descriptor-facing reads/writes, and kernel-only device hooks.
The real post-ExitBootServices PS/2 path injects QEMU keyboard bytes into the
same TTY, and the QEMU keyboard proof requires both the hardware-input and
line-discipline markers.

This checks the **bounded TTY foundation** only. It does not yet provide
termios, job control, sessions/process groups, signals, scheduler-blocking
wakeups, UTF-8 editing, a userspace shell, or PID 1 terminal ownership.

**Verified M6 compiled Rust init runtime (PR #162):**
[Actions run 36494885774](https://github.com/mixutin/Vibrix/actions/runs/36494885774)
passed the full exact-head repository matrix on implementation head
`1a06caa7f9499e966f15f2e0077bed767992da8e`. The build produces the real
`no_std` `vibrix-init` ELF at the fixed lower-half userspace layout, the
production userspace ELF loader validates and stages that exact binary into the
private process CR3 with separated guarded mappings and final W^X permissions,
and QEMU enters its compiled Rust entry at CPL3. The program reaches the real
SYSCALL path, observes PID 1 through `getpid`, and terminates PID 1 through
the real `exit(0)` syscall; CI requires independent kernel markers and COM1
evidence for both transitions.

This checks the bounded **Rust init runtime** item. It does not by itself
provide an interactive userspace shell, core utilities, persistent root, USB
storage, general multi-process scheduling or a full service manager.

**Verified M6 compiled Rust shell runtime (PR #176):**
[CI run 36527750888](https://github.com/mixutin/Vibrix/actions/runs/36527750888)
passed the full exact-head matrix on implementation head
`c029e68221ba0b5a4a81f9b3e7ff9d62dff1fd59`. The build produces the
real no_std `vibrix-sh` ELF, the kernel stages it into the private userspace
CR3, pre-opens `/dev/tty` as fd 0/1/2, and routes userspace `read`/`write`
through the real SYSCALL path, validated copy-in/copy-out and the VFS TTY.
QEMU injects actual virtual keyboard input after the shell blocks in its TTY
read; `help`, `echo hi` and `exit` execute in CPL3 and PID 1 terminates
through the real exit syscall.

This checks the **interactive Rust shell runtime** only. The remaining roadmap
core utilities are not complete merely because their command names parse:
`cat`, `ls`, `pwd`, `cd`, `mkdir`, `cp`, `mv`, `rm`, `ps`
and `kill` still need real userspace/kernel backends and proofs.

**Verified M6 core utility backends (PR #180):**
[CI run 36553591892](https://github.com/mixutin/Vibrix/actions/runs/36553591892)
passed the full exact-head repository matrix on implementation head
`a35dc931d432c89d3d0a4f57ea6a21511a616109`, including nightly torture and
the real-keyboard QEMU utility transcript. The compiled Rust shell executes
`cat`, `echo`, `ls`, `pwd`, `cd`, `mkdir`, `cp`, `mv`, `rm`,
`ps` and `kill` through real userspace syscalls backed by the bounded VFS
and process table. QEMU proves path creation/copy/move/removal, working-directory
changes, process listing, and PID 2 transitioning from running to zombie after
`kill`. Four deliberately invalid path/self-copy operations fail while source
data remains readable. The exact keyboard proof also production-tests the PS/2
minus scan code required by `echo utilities-ok`.

This completes the **bounded M6 core utility set** and therefore the M6 exit
criterion: Vibrix boots to an interactive Rust userspace shell with the listed
utility backends. The filesystem remains volatile RAM, process/job control is
minimal, and none of this implies persistent USB root, signals, atomic rename,
recursive copy semantics or POSIX conformance.

**Exit:** boot to an interactive Vibrix userspace shell.

## M7 — USB platform
- [x] xHCI initialization
- [x] USB device enumeration
- [x] USB hub support
- [x] USB HID keyboard
- [x] USB HID mouse
- [x] USB mass-storage transport
- [x] SCSI transparent command subset for mass storage
- [x] Block-device abstraction
- [ ] Detect the boot USB device robustly
- [ ] Read/write blocks on the Vibrix USB device

**Verified M7 native USB mass-storage BOT + SCSI transport (PR #253):**
All 23 exact-head workflows passed implementation head
`f9e14b6ec60996cdc90e78216eda9175079f07bd`, including the dedicated USB
mass-storage evidence. The native xHCI path configures the device's bulk IN/OUT
endpoints and carries the shared BOT CBW/CSW and SCSI contracts through real
QEMU USB storage. Evidence exercises TEST UNIT READY, INQUIRY, REQUEST SENSE,
READ CAPACITY(10), READ(10), WRITE(10), and SYNCHRONIZE CACHE(10), performs a
reversible one-block write/flush/read-back at the bounded test LBA, restores the
original contents, and requires the disposable backing image hash to match
before and after the proof.

This checks **USB mass-storage transport** and the bounded **SCSI transparent
command subset for mass storage** only. It does not yet identify the firmware
boot USB robustly, expose long-lived USB block-device ownership, mount a
persistent root, cover hub-routed storage/reset recovery/multiple LUNs, or prove
physical Target 001 behavior.

**Exit:** Vibrix can access the same removable USB device it booted from after leaving firmware services.

**Verified M7 native xHCI initialization (PR #168):**
[CI run 36519983485](https://github.com/mixutin/Vibrix/actions/runs/36519983485)
passed formatting, host contracts, target Clippy/builds and the full QEMU
regression matrix on implementation head `ec6aabe8a27f9e9fc07e8dc952bbb7c3a4dcf63c`.
The dedicated post-firmware probe discovered the QEMU xHCI PCI function,
validated its MMIO capability header, reset the controller, provisioned bounded
DCBAA/command/event/ERST structures, programmed operational/runtime registers
and observed the controller transition to Running. Independent kernel output
reported `VIBRIX: kernel xHCI reset and running` and
`kernel xHCI: 00:03.0 ... version=0x100 slots=64 ports=8`.

This checks **controller initialization only**. USB device enumeration, hubs,
HID, mass-storage transfers, SCSI, robust boot-USB identity, persistent USB I/O
and Target 001 hardware proof remain separate unchecked work.

**Verified M7 direct USB device enumeration (PR #175):**
[CI run 36526713750](https://github.com/mixutin/Vibrix/actions/runs/36526713750)
passed the exact implementation head
`7bf73f1169f09b338e09461c34c9f4fd0c06fc92`, including the full QEMU
regression matrix and the dedicated xHCI enumeration proof. The kernel reset a
real emulated root port, submitted Enable Slot and Address Device commands,
constructed the required slot/EP0 input context, consumed command-completion
events, issued a real endpoint-zero GET_DESCRIPTOR(Device, 18) transfer, then
validated and reported the returned USB device descriptor. QEMU attached an
actual `usb-kbd` device behind `qemu-xhci`; the proof rejects zero VID/PID,
malformed descriptors, command/transfer failures and unsupported context
layouts.

This checks **one directly attached root-port device enumeration path**. Hub
traversal, HID report/configuration handling, mass-storage/SCSI, robust boot-USB
identity and native USB block I/O remain separate unchecked M7 work.

**Verified M7 USB2 hub control and downstream-port management (PR #178):**
[CI run 36551513173](https://github.com/mixutin/Vibrix/actions/runs/36551513173)
and all companion workflows passed exact implementation head
`c697e76db7aeba5ff82959dd6aa533c9d2dbb3a8`. The native xHCI path addresses
a real QEMU USB2 hub, reads and validates its class hub descriptor, powers the
advertised downstream ports, locates a connected child, issues PORT_RESET and
polls class GET_STATUS until the child is connected, enabled and powered with
reset clear. The real topology `qemu-xhci -> usb-hub -> usb-kbd` reported
root port 5, slot 1, eight downstream ports, child port 1 and status `0x0103`.

This checks **bounded USB2-compatible hub support** only. It does not yet
address the downstream child as a HID device through the hub, implement
SuperSpeed hub semantics, hotplug, multiple-hub arbitration, mass storage,
persistent boot-USB access or physical Target 001 validation.

**Verified M7 native USB HID interrupt input (PR #230):**
[Actions run 36614664509](https://github.com/mixutin/Vibrix/actions/runs/36614664509)
passed the full exact-head CI matrix on implementation head
`2660a874b0fe9590a5d7cea3874f575aa39b0cb8`. The xHCI path configures real
interrupt-IN endpoints for bounded HID boot keyboard and mouse interfaces,
validates descriptor geometry and completion identity/residue, observes real
keypress/modifier/release and mouse motion/button transitions, and disables the
slot cleanly after the proof sequence.

This checks the current **USB HID keyboard** and **USB HID mouse** roadmap
items for directly attached boot-protocol devices. It does not claim arbitrary
report descriptors, hub-routed HID, hotplug, desktop/TTY integration, LED/layout
support, persistent USB storage, or physical Target 001 validation.

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
- [x] formatter + recovery tool

**Verified M8 journal-enabled formatter and host recovery (PR #121):**
[Actions run 36442635348](https://github.com/mixutin/Vibrix/actions/runs/36442635348)
passed the exact synchronized head, including format checks, the production
VibrixFS wire/journal/image tests, target lint/build checks and the complete QEMU
regression matrix. Fresh regular-file images now reserve an allocated 66-block
redo journal and advertise the incompatible journal feature. The recovery tool
validates the two superblocks independently, selects the highest compatible
checkpoint, validates a committed transaction as a unit before any home write,
replays complete after-images idempotently, checkpoints secondary then primary,
and retires the journal only afterward. Tests destroy a metadata home block and
prove committed replay repairs it; corrupt committed payloads fail before home
replay, dirty checkpoints without a recoverable journal fail closed, and an
uncommitted transaction is retired only after a clean checkpoint is published.

This checkbox is **host regular-file formatter/recovery tooling**. It is not a
kernel VFS driver, native USB storage path, writable USB-root mount, hardware
flush/barrier proof, power-loss validation or Target 001 filesystem evidence.

- [x] VFS driver

**Verified M8 read-only VibrixFS VFS driver (PR #170):**
[Actions run 36524057848](https://github.com/mixutin/Vibrix/actions/runs/36524057848)
passed the exact implementation head `d58ba901482d10a4255d495a03580528cd724e02`,
including shared wire-format tests, production kernel VFS tests, Clippy, target
builds, the full QEMU regression matrix and dependency gates. The kernel can
mount an already-selected partition-sized `BlockDevice` read-only, require two
compatible clean superblocks, validate allocation metadata and checksummed
inodes/directories on access, walk directories, report metadata, and read
regular-file extents through the existing VFS contract. Mutation fails
`ReadOnly`, and production-block-device fixtures cover both 512-byte and
4096-byte logical-sector-compatible geometry.

This checks the **read-only VibrixFS VFS backend** only. It does not identify
the boot USB device, issue USB/SCSI storage I/O, replay or commit the journal in
kernel, prove write barriers, or mount a persistent USB root; those remain
separate M7/M8/M9 work.

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
or physical provisioning. The later PR #121 recovery evidence above supersedes
only that original formatter/recovery limitation; kernel VFS and USB persistence
remain separate work.

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
- [x] RAM-backed /tmp and runtime state
- [ ] Flash-write reduction
- [x] Hardware rediscovery every boot
- [x] Portable configuration policy
- [ ] Safe USB provisioning/imaging tool
- [ ] Recovery partition/environment
- [x] System update + rollback strategy
- [ ] Target 001 real USB boot
- [ ] Move the same USB drive between two compatible machines

**Exit:** boot from USB, modify files/configuration/apps, power off, move or reboot the drive, and retain all state without touching an internal system disk.

**Verified M9 volatile runtime-state foundation (PR #157):**
[Actions run 36489168560](https://github.com/mixutin/Vibrix/actions/runs/36489168560)
passed the full exact-head matrix on
`7ea18b078f0d091b14dd30abdb95327dd308ef13`. The retained bootstrap VFS
creates bounded RAM-backed `/tmp` and `/run` directories and the
production-linked self-test writes and reads volatile `/run/boot-state`
without any persistent block-device writes.

This checks **RAM-backed /tmp and runtime state** only. Persistent root/home,
package state, native USB storage and flash-write policy remain separate.

**Verified M9 hardware rediscovery every boot (PR #251):**
[Full CI run 36621258357](https://github.com/mixutin/Vibrix/actions/runs/36621258357)
and dedicated [hardware-rediscovery run 36621257944](https://github.com/mixutin/Vibrix/actions/runs/36621257944)
passed on exact implementation head `a25e020627ce67d861ea0a0d7286e3a27f067796`.
The same built kernel image was booted twice: once with the default q35 PCI
inventory and once with an added qemu-xhci function. The native post-firmware
scanner reported zero then one xHCI controller, proving the device inventory is
rebuilt from the hardware presented on each boot rather than loaded from stale
persistent state.

This checks **Hardware rediscovery every boot** for the current ACPI/PCI QEMU
path. Runtime hotplug, USB child-device rediscovery, persistent configuration
reconciliation and physical Target 001 hardware remain separate work.

## M10 — Networking
- [x] NIC abstraction
- [ ] RTL8168-family driver
- [x] Ethernet + ARP
- [x] IPv4 + ICMP
- [x] UDP
- [x] DHCP
- [x] DNS
- [ ] TCP
- [ ] sockets
- [ ] network utilities

**Verified bounded M10 IPv4 + ICMP (PR #195):**
[Actions run 36575304733](https://github.com/mixutin/Vibrix/actions/runs/36575304733)
passed full exact-head CI on implementation head
`9553657c07960522a9d2ed1ba26b5b48bc0e7593`. The production kernel includes
strict IPv4 v4/IHL=5 parsing/encoding, Internet checksum validation, ICMPv4 Echo
request/reply handling, odd-length checksum support and a post-ExitBootServices
QEMU self-test requiring `VIBRIX: kernel IPv4 ICMP echo verified` on the
network evidence path. Options, fragmentation/reassembly, routing, external
hardware traffic and Target 001 remain outside this checkbox.

**Verified bounded M10 UDP datagrams (PR #196):**
[Actions run 36576980721](https://github.com/mixutin/Vibrix/actions/runs/36576980721)
passed full exact-head CI on synchronized implementation head
`b1907c2fababc237b0af0cd1e9fd98088443a46e`. The kernel validates UDP lengths
inside IPv4 payloads, implements IPv4 pseudo-header checksums including
odd-length payloads and zero-checksum semantics, and exercises the production
path after firmware exit. This is the UDP datagram layer only: no sockets,
port-allocation API, DHCP/DNS, TCP or external NIC claim follows.

**Verified bounded M10 DHCPv4 acquisition (PR #211):**
[Actions run 36586598165](https://github.com/mixutin/Vibrix/actions/runs/36586598165)
passed the full exact-head CI/QEMU matrix on implementation head
`7348d34ac368c98af61f6481fe6eda30d4207790`. The production kernel implements
the initial RFC 2131/2132 `DISCOVER → OFFER → REQUEST → ACK` client exchange
with bounded BOOTP/TLV parsing, transaction/client/server validation and lease
parameter extraction. QEMU requires the production DHCP marker on debugcon and
COM1. External DHCP traffic, retransmission, renewal/rebinding and persistence
still depend on later runtime/NIC work.

**Verified bounded M10 DNS A resolver (PR #212):**
[Actions run 36586898449](https://github.com/mixutin/Vibrix/actions/runs/36586898449)
passed the full exact-head CI/QEMU matrix on implementation head
`c94a78334c52f71cff23cb567818a281ca366ba8`. The production kernel composes
and parses bounded RFC 1035 recursive IN/A transactions through the Vibrix UDP
path, validates transaction/header/question identity, bounds compressed-name
walks, rejects truncation/RCODE/malformed loops and returns up to four A records
with the minimum TTL. QEMU requires the resolver marker on both independent
kernel outputs. Caching, retry policy, CNAME chains, DNSSEC/EDNS and TCP fallback
remain later work.

**Verified bounded M10 Ethernet + ARP runtime (PR #197):**
[Actions run 36580005188](https://github.com/mixutin/Vibrix/actions/runs/36580005188)
passed the exact-head host, target, QEMU, dependency and aggregate CI gates on
implementation head `1e8c4a0fa1a19738691d9fb7c4c8e75222c3d0fb`. The production
kernel has a fixed-capacity ARP neighbor table, RFC-826 update behavior,
local-address request replies, probe handling without caching `0.0.0.0`,
explicit local-address conflict detection and a deterministic full-table
replacement policy. QEMU requires the kernel ARP responder evidence marker.
Cache aging, DHCP, routing, physical RTL8168 I/O and Target 001 networking are
still separate work.

## M11 — Security and multi-user
- [ ] users/groups/credentials
  - [ ] persistent user/group account database
  - [ ] login/session authentication
  - [x] real/effective/saved-ID transition syscalls
  - [x] supplementary-group management policy
  - [ ] set-user-ID/set-group-ID execution semantics
- [ ] permissions
  - [ ] enforce owner/group/other DAC during VFS path traversal
  - [ ] enforce DAC on open/create/remove/rename
  - [ ] chmod/chown policy and syscalls
  - [ ] umask and default creation modes
  - [ ] sticky-directory semantics
- [x] secure random
- [x] W^X
- [ ] userspace ASLR
- [x] stack protections
- [ ] IOMMU
- [ ] secure updates
- [x] optional USB system encryption design

**Verified M11 real/effective/saved-ID transition syscalls (PR #254):**
[Actions run 36626673773](https://github.com/mixutin/Vibrix/actions/runs/36626673773)
passed the full exact-head repository matrix on implementation head
`1d7f3f934b20feba9edc0dba2f9127e1021b7896`. ABI v1 now includes
`getresuid`, `setresuid`, `getresgid` and `setresgid`; non-root callers
may select only IDs already present in their current real/effective/saved tuple,
effective UID 0 may choose arbitrary non-sentinel IDs, tuple validation is
complete before mutation, and native Ring 3 evidence drops and regains saved
root identity through the real syscall path.

This checks only the bounded **real/effective/saved-ID transition syscalls**
sub-item. Persistent accounts, login/authentication, set-ID executable
semantics and VFS DAC enforcement remain separate unchecked work.

**Verified M11 fail-closed secure random (PR #217):**
[Actions run 36588340214](https://github.com/mixutin/Vibrix/actions/runs/36588340214)
passed the full exact-head CI matrix on implementation head
`944ce8ffad8985733a604b7d98c2ac4316f61be9`, with the dedicated secure-random
evidence workflow also succeeding in
[run 36588339666](https://github.com/mixutin/Vibrix/actions/runs/36588339666).
The production kernel gates RDSEED with CPUID, checks the carry flag on every
sample, uses bounded retries, applies a serialized continuous duplicate-word
health check, fills output transactionally, and fails closed without silently
falling back to a weaker entropy source. The post-ExitBootServices QEMU path
executes the same production implementation used by host policy tests.

This checks the bounded **secure random** roadmap item for the current x86-64
RDSEED-backed design. It does not claim entropy from unsupported CPUs, a DRBG,
persistent entropy pools, userspace random-device APIs, multi-source mixing,
physical Target 001 validation, or cryptographic key-management completion.

**Adopted optional encrypted-volume design (ADR 0023):** the Vibrix system
partition may later expose an authenticated encrypted block layer beneath
VibrixFS. The design uses a random volume master key, Argon2id passphrase
keyslots, HKDF-separated keys, AES-256-GCM-SIV data blocks, external
authentication tags/tree metadata, redundant authenticated roots and explicit
crash-ordering/fail-closed rules. It also documents the unavoidable lack of
whole-device rollback freshness without an external trusted anchor.

This is a **design checkbox only**. No cryptographic implementation, secure
random source, encrypted USB I/O, unlock UI, key management, boot-chain
authentication or recovery support is claimed. Those remain separate work.
See [ADR 0023](docs/decisions/0023-usb-system-encryption.md).

**Verified M11 guarded userspace stack protection (PR #224):**
[Actions run 36592792740](https://github.com/mixutin/Vibrix/actions/runs/36592792740)
passed the full exact-head CI matrix on implementation head
`d730b43ac338359bbdf36d124656495a8a59c512`. The production userspace address
space uses the existing guarded stack layout with an RW/NX payload page and
unmapped lower/upper guard pages. The dedicated QEMU fault probe enters CPL3,
writes exactly below the lower guard boundary and requires a real user-mode
page fault decoded as non-present, write, user, non-reserved and non-execute.

This checks the bounded **stack protections** item for guard-page-backed
userspace stacks. It does not claim compiler canaries, CET/shadow stacks, ASLR,
kernel/IST guard stacks, automatic stack growth, signals, SMP shootdowns or
physical Target 001 validation.

**Verified M11 W^X enforcement (existing implementation, PR #160):**
[Actions run 36490303799](https://github.com/mixutin/Vibrix/actions/runs/36490303799)
passed the full exact-head matrix on implementation head
`ef780dbc298a33bf92eb40d9d7f7db75c6cd969b`. The production userspace ELF
planner rejects any PT_LOAD segment that is both writable and executable before
the mapping sink is mutated. The shared managed-VM permission type makes RWX
unrepresentable: mappings are only read-only, read-write or read-execute.
During ELF loading, segment pages are staged writable only while bytes/BSS are
copied, then non-writable segments are protected to their final RO or RX state
before the entry point is validated and CPL3 execution begins. The QEMU proof
executed the validated ELF under the private CR3 and completed the real syscall
round trip after final permissions were committed.

This checks the current bounded **W^X mapping policy and executable-load path**.
It does not claim ASLR, stack canaries, JIT support, executable shared-memory
policy, SMP TLB-shootdown hardening, IOMMU isolation or physical Target 001
validation.

**M11 credential groundwork:** [ADR 0024](docs/decisions/0024-credentials-and-dac.md)
defines a fixed-capacity Unix-style credential object with real/effective/saved
UIDs/GIDs, bounded supplementary groups, process inheritance and a fail-closed
owner/group/other DAC evaluator. The implementation is intentionally a
foundation only: no M11 top-level checkbox is complete until persistent
accounts, credential transitions and VFS enforcement have exact-head evidence.

**Verified M11 supplementary-group management policy (PR #250):**
[Actions run 36618504453](https://github.com/mixutin/Vibrix/actions/runs/36618504453)
passed the full exact-head repository matrix on implementation head
`24bc5c12881526f2e528a4773a50f7f218f497c6`. The fixed-capacity credential
object now allows supplementary-group replacement only while effective UID is
0, checks the eight-group capacity before mutation, and leaves the previous list
unchanged on denied or oversized updates. The production credential self-test
executes the same policy during the normal kernel boot path.

This checks the bounded **supplementary-group management policy** sub-item only.
Persistent accounts, login/authentication, ID-transition syscalls, set-ID
execution and VFS DAC enforcement remain separate work.

## M12 — SMP and performance
- [x] CPU enumeration
- [ ] AP startup
- [x] per-CPU structures
- [ ] SMP scheduler
- [x] synchronization
- [ ] TLB shootdowns
- [x] profiling
- [ ] Target 001 8C/16T validation

**Verified M12 publish-once per-CPU structures (PR #200):**
[Actions run 36582435601](https://github.com/mixutin/Vibrix/actions/runs/36582435601)
passed the full exact-head CI matrix on implementation head
`d3537f8a4437d901abf5c786856cf58ce577656e`. The kernel builds a fixed
publish-once CPU slot table from validated MADT processor identities, retains
firmware UID/APIC identity/availability and xAPIC/x2APIC origin, publishes the
table with release/acquire ordering, and binds the BSP slot to the actually
observed LAPIC ID. Host tests cover unavailable/duplicate/overflow cases, and
the production post-firmware path reports the bound BSP through the same
compiled implementation.

This checks the bounded **per-CPU identity/state foundation** only. Application
processor startup, per-CPU interrupt/syscall stacks, SMP scheduler/run queues,
cross-CPU synchronization, TLB shootdowns, CPU hotplug and Target 001 8C/16T
hardware validation remain separate unchecked work.

**Verified M12 bounded scheduler profiling (PR #296):**
all 37 exact-head workflows passed implementation head
`63c97c961d007af798cf872c706d36e2f581a0ee`, including dedicated
[scheduler-profiling run 36674427206](https://github.com/mixutin/Vibrix/actions/runs/36674427206).
The production scheduler now exposes an observational fixed-size snapshot of
cumulative switches, completions and preemptions plus ready/running/exited slot
counts. The existing cooperative QEMU smoke path validates the snapshot without
resetting counters or changing scheduling decisions.

This checks the bounded **profiling** item for the current single-BSP scheduler.
It does not claim whole-system profiling, sampling profilers, SMP aggregation,
userspace tooling, hardware performance counters or Target 001 validation.

**Verified M12 bounded synchronization primitives (PR #320):**
all 48 exact-head workflows passed implementation head
`ddb4d50c26e7ad3b25f42846ea2ec3986144c67a`, including dedicated
[synchronization evidence run 36689474841](https://github.com/mixutin/Vibrix/actions/runs/36689474841).
The kernel provides a fair ticket lock and a fixed-participant boot barrier with
explicit Acquire/Release ordering, host contention/ordering tests, and the normal
post-firmware self-test marker.

This checks **synchronization** for the current bounded primitives. It does not
claim an SMP scheduler, cross-CPU interrupt coordination, TLB shootdowns, lock
debugging, priority inheritance, or physical Target 001 multicore validation.

## M13 — Audio and graphics

Owner-requested early desktop slice (2026-09-29, GPT-6 Astra Pro): `./tools/run-qemu.sh --desktop` boots a real native Ring 3 session with checked copy-based graphics/input syscalls, a terminal using the existing shell, read-only RAM file browsing, buffered polled PS/2 keyboard/mouse input, title-bar dragging and maximize/restore. The dedicated [desktop workflow](.github/workflows/userspace-desktop.yml) checks actual guest pixels and input, including a terminal-created file opened in the viewer. See [DESKTOP.md](docs/DESKTOP.md) for exact limits and [CHROMIUM_PORT.md](docs/CHROMIUM_PORT.md) for the unimplemented browser gates.

This is a single-process foreground desktop preview, **not completion of M13**, not Chromium, not GPU/USB desktop qualification and not persistent USB-root operation. It is opt-in; the early USB-root priorities and existing kernel/shell profiles remain intact.

- [ ] HDA + basic PCM
- [ ] USB audio
- [x] graphics architecture
- [x] framebuffer userspace API
- [x] compositor/display-server design
- [x] Navi 23 modesetting research
- [ ] native modesetting
- [ ] acceleration
- [x] GUI toolkit

**Verified M13 bounded userspace framebuffer/display API (PR #199):**
[Full CI run 36578586659](https://github.com/mixutin/Vibrix/actions/runs/36578586659)
and [dedicated userspace display run 36578586194](https://github.com/mixutin/Vibrix/actions/runs/36578586194)
passed on implementation head `6c7774da242303c071e229431ff4b91a8d02895d`.
The opt-in native Ring 3 desktop uses additive ABI v1 calls `DisplayInfo`,
`DisplayFill` and `DisplayBlit` through the real syscall path. User pointers
are validated by the checked copy layer, blits copy through a bounded kernel
buffer, malformed rectangles/counts/reserved arguments fail before drawing,
and no framebuffer/MMIO physical address is exposed to userspace. The dedicated
workflow boots the real userspace ELF and validates actual guest framebuffer
pixels and interactive input rather than a host-rendered mockup.

This checks the bounded **framebuffer userspace API** only. The retained GOP
framebuffer remains kernel-owned and software-rendered; native GPU modesetting,
acceleration, multi-client compositor isolation, physical Target 001 display
validation and the general GUI toolkit remain separate unchecked work.

**Adopted M13 graphics architecture and Navi 23 research (ADR 0022):**
Vibrix separates kernel modesetting/hardware ownership from a userspace
compositor/display server and application-owned rendering buffers. The kernel
side uses validated connector/timing/plane/buffer objects and atomic
validate-then-commit state; the compositor owns presentation, focus and input
routing through an asynchronous surface/buffer protocol. The initial path may
remain software-rendered and single-output.

Research against upstream AMD Display Core/DCN, DRM/KMS and AMDGPU sources
establishes the Navi 23 implementation path but does **not** provide evidence
for speculative raw register programming. Native modesetting therefore remains
unchecked until a source-traceable implementation is validated, and physical
Target 001 display claims still require real hardware evidence. See
[ADR 0022](docs/decisions/0022-graphics-stack-navi23.md).

**Verified M13 reusable native GUI toolkit (PR #241):**
[Actions run 36615443810](https://github.com/mixutin/Vibrix/actions/runs/36615443810)
passed the full exact-head repository matrix, including the dedicated GUI toolkit
evidence workflow, on implementation head
`92cdbcfbd8b19aa96847b5bfbc086a4ad1ee45b9`. The no_std `vibrix-ui` crate
provides reusable widget/layout/rendering primitives and the native Ring 3
desktop is migrated to those shared components while preserving the real
display/input QEMU/RFB proof path.

This checks the bounded **GUI toolkit** item only. Native GPU modesetting,
acceleration, multi-process compositor isolation and physical display hardware
validation remain separate work.

## M14 — Packages and development
- [x] Package format/database/dependencies
- [ ] package manager
- [ ] signed repositories
- [ ] ports/build recipes
- [ ] editor/developer tooling
- [x] compiler bootstrap plan

**Verified M14 Rust compiler bootstrap plan (PR #220):**
[Actions run 36590730685](https://github.com/mixutin/Vibrix/actions/runs/36590730685)
passed the full exact-head repository matrix on implementation head
`8c58ce2b9740ab78870693929456e3b95f641ce2`. The accepted plan defines the
transition from the current external cross-compiled bare-metal toolchain to a
hosted Vibrix target, an externally produced first native stage0, native program
compilation evidence, upstream-style stage0/stage1/stage2 rebuilding,
provenance/reproducibility metadata, explicit transitional linker/codegen trust
dependencies, and the USB-only failure boundary.

This checks only the **compiler bootstrap plan** deliverable. No hosted Vibrix
Rust target, native rustc/Cargo execution, self-hosted userspace/kernel build or
bootable-release production is claimed; those remain M15/M20 execution
milestones.

**Verified M14 package format/database/dependencies (PR #242):**
[Full CI run 36621263779](https://github.com/mixutin/Vibrix/actions/runs/36621263779)
and dedicated [package-metadata run 36621263629](https://github.com/mixutin/Vibrix/actions/runs/36621263629)
passed on exact implementation head `f6611270b51ff2bb61c3fe3e903913e24abccb94`.
A native x86_64-unknown-none Ring 3 ELF executes the production VPKG metadata
codec and fixed-capacity database through the real userspace/process path,
including encode/decode, reserved-byte rejection, dependency ordering,
transactional missing-dependency rejection and reverse-dependency removal
protection.

This checks the bounded **Package format/database/dependencies** item. Package
payload installation, persistent package state, package-manager UX and signed
repositories remain separate work.

## M15 — Self-hosting
- [ ] Compile a Rust userspace program on Vibrix
- [ ] Toolchain usable on Vibrix
- [ ] Build userspace on Vibrix
- [ ] Build kernel on Vibrix
- [ ] Produce a bootable Vibrix USB image from Vibrix

## M16 — Reliability, updates and recovery

Post-operational milestone: Vibrix already boots into persistent userspace before
this milestone begins.

- [ ] Signed system update manifests and artifacts
- [x] Stable / beta / nightly update channels
- [x] Transactional update staging
- [ ] Automatic rollback after failed boot/update
- [ ] Explicit `vpm update` / system-update workflow
- [x] Update history and rollback selection
- [ ] Known-good recovery environment on the Vibrix USB itself
- [ ] Recovery can inspect and repair Vibrix FS without another OS
- [ ] Recovery can restore previous system generation
- [ ] Recovery never selects internal disks as Vibrix system targets
- [ ] Offline signed update bundles
- [ ] USB health and write/endurance diagnostics
- [x] Power-loss/update interruption tests

**Verified M16 stable / beta / nightly update-channel policy (PR #265):**
all 23 exact-head workflows passed implementation head
`2a529807567ea6856d6ae348924dab4555e67cf4`, including dedicated
[update-channel run 36629207805](https://github.com/mixutin/Vibrix/actions/runs/36629207805).
The update state machine now carries an explicit stable/beta/nightly channel,
strictly rejects unknown channel names and channel-mismatched candidate
releases, and refuses a channel switch while a trial update is pending.

This checks the bounded **Stable / beta / nightly update channels** control-plane
item. Persistent channel configuration, artifact fetching, signatures and
automatic update execution remain separate work.

**Exit:** a failed system update can be diagnosed and rolled back from the same
Vibrix USB without another computer or operating system.

**Verified M16 update history and rollback selection (PR #285):**
the exact-head repository matrix passed implementation head
`762ff6fdd6f27d2d39b16636500110a3c721ea08`, including dedicated
update-history/rollback evidence. The fixed-capacity generation history records
candidate/healthy generations, enforces a security floor, chooses only eligible
healthy rollback targets, and fails closed when no valid target exists.

This checks **Update history and rollback selection** as a bounded control-plane
state machine. It does not claim persistent on-disk history, automatic reboot
rollback, signed artifacts, or a complete recovery environment.

**Verified M16 transactional update staging policy (PR #298):**
all 39 exact-head workflows passed implementation head
`8b6b2a31113531869655b9125cf65fb3fb751388`, including dedicated
[transactional-staging run 36674789687](https://github.com/mixutin/Vibrix/actions/runs/36674789687).
The fixed-capacity state machine assigns monotonic transaction IDs, requires
ordered manifest/payload/verification/durable transitions, preserves state on
failed transitions, discards every incomplete persisted phase after simulated
interruption, and hands only the latest exact durable candidate to the existing
trial-boot policy without promoting it to known-good.

This checks **Transactional update staging** as the control-plane durability
policy. It does not claim physical USB writes, filesystem flush semantics,
signature verification, power-loss-safe media behavior, or a complete updater.

**Verified M16 exhaustive update-interruption crash-cut tests (PR #307):**
the exact-head repository matrix passed implementation head
`0d248ba271de5e39556661b5e4d7fb5d83d1601b`, including dedicated
[update-interruption evidence run 36677996532](https://github.com/mixutin/Vibrix/actions/runs/36677996532).
The production transactional staging state machine is exercised at every
persistence cut: incomplete manifest/payload/verification stages are discarded,
only an exact durable candidate survives recovery, stale/rollback records are
rejected, transaction IDs advance without reuse, and sequence exhaustion fails
closed.

This checks **Power-loss/update interruption tests** at the bounded control-plane
crash-cut model. It does not claim physical USB flush/barrier semantics or
hardware power-cut durability.

## M17 — Package ecosystem and profiles

- [ ] `vpm` package manager UX
- [ ] `vpm search/install/remove/update/why/audit`
- [ ] Package provenance, license and signature display
- [x] Package dependency graph inspection
- [ ] Minimal profile
- [ ] Developer profile
- [ ] Server profile
- [ ] Recovery profile
- [ ] Optional security-lab profile
- [ ] Profile installation/removal is transactional
- [ ] Offline package cache
- [ ] Package repository mirrors cannot bypass signature verification

### Optional authorized security-lab profile

Security tooling is **not part of the default installation**. The optional
profile is intended for diagnostics, CTFs, labs and systems the operator is
authorized to test.

Possible package categories:

- packet capture and protocol inspection
- network discovery/scanning
- DNS/HTTP/TLS diagnostics
- web application testing
- binary inspection, reversing and debugging
- forensic image/file inspection
- cryptographic utilities
- traffic generation for owned lab environments
- exploit-development/debugging tooling where legally appropriate

- [ ] Security tools run in a clearly identified profile
- [ ] Disposable/sandboxed security workspaces
- [ ] Optional isolated network namespace
- [ ] Restricted host/persistent filesystem mounts by default
- [ ] Easy reset to a clean lab state
- [ ] Security profile never silently enables network-facing services

**Exit:** Vibrix can install signed package profiles without bloating or
weakening the default system.

**Verified M17 package dependency graph inspection (PR #318):**
all exact-head workflows passed implementation head
`5c9e0a01a872e933caf04f5a9f1650a4e3b4326c`, including dedicated
[package dependency graph run 36689258755](https://github.com/mixutin/Vibrix/actions/runs/36689258755).
The native package metadata path exposes bounded installed dependency and reverse-
dependency relationships using the same package database implementation exercised
inside Ring 3, with missing dependencies and removal of required packages still
failing transactionally.

This checks **Package dependency graph inspection** for the current bounded
package database. It does not claim a persistent package database, repository
resolution, package-manager installation UX, or signed remote repositories.

## M18 — Observability and troubleshooting

- [x] Structured kernel logging
- [ ] Persistent userspace journal
- [ ] Boot IDs and monotonic/wall-clock timestamps
- [x] Log levels and subsystem filtering
- [ ] `vlog` query/follow interface
- [ ] Previous-boot log access
- [ ] Flash-aware log rotation and retention
- [ ] Panic/crash record persisted across reboot where safe
- [ ] Symbolized kernel stack traces
- [x] Register/fault context in crash diagnostics
- [x] `vibrix status` system overview
- [x] `vibrix doctor` automated diagnostics
- [x] Driver binding/missing-driver diagnostics
- [x] Filesystem/network/update health checks
- [x] Privacy-reviewed `vibrix doctor --bundle` support bundle
- [x] Verbose boot mode while normal boot remains clean
- [x] Hardware compatibility/quirk reporting
- [x] Optional anonymized compatibility reports only with explicit opt-in

**Verified M18 structured kernel logging and filtering (PR #219):**
[Actions run 36591286297](https://github.com/mixutin/Vibrix/actions/runs/36591286297)
passed the full exact-head repository matrix on implementation head
`fb375cdedf04a6fdae1a9a9deda46388f2601f43`. The kernel now owns a bounded
128-record structured event buffer with boot-local sequence, severity,
subsystem, stable event code and numeric values. Unique atomic slot reservation
plus Release/Acquire publication makes published records immutable without a
spin lock; full capacity fails explicitly rather than overwriting old records.
A packed atomic filter applies one minimum severity and subsystem mask without
torn configuration reads, and filtered events do not consume capacity.
Production host tests cover record structure, sequence, bounds, saturation and
filter behavior, while the normal post-ExitBootServices QEMU path requires the
same compiled self-test marker on debugcon and COM1.

This checks **Structured kernel logging** and **Log levels and subsystem
filtering** only. Persistent journal storage, timestamps, `vlog`, previous-boot
access, rotation/retention, crash persistence, symbolization and SMP/per-CPU
logging remain separate unchecked M18 work.

**Verified M18 userspace status/doctor commands (PR #233):**
[Actions run 36596122420](https://github.com/mixutin/Vibrix/actions/runs/36596122420)
passed the full exact-head repository matrix on implementation head
`67cee22698c4763d20f7920528f8e3f5f6017a52`. The native Ring 3 shell exposes
`vibrix status` and `vibrix doctor` through the existing syscall/VFS/process
paths, with read-only process, VFS and bootstrap-device diagnostics and explicit
labels for currently unavailable persistence/network/update state. The existing
userspace CLI/QEMU evidence exercises the same compiled shell rather than a host
facsimile.

This checks the bounded **vibrix status system overview** and **vibrix doctor
automated diagnostics** items only. It does not claim persistent history,
privileged repair actions, package/network management, or a complete support
bundle.

**Verified M18 driver binding/missing-driver diagnostics (PR #229):**
[Actions run 36595702118](https://github.com/mixutin/Vibrix/actions/runs/36595702118)
and dedicated driver-diagnostics
[run 36595701457](https://github.com/mixutin/Vibrix/actions/runs/36595701457)
passed on implementation head `10c787b825ee4c22bf178b4a91534369464960c8`.
The native post-firmware PCI path reports discovered devices, driver candidates,
successful bindings, devices with no registered driver and candidate binding
failures, with an independent debugcon readiness marker and host accounting
tests.

This checks **Driver binding/missing-driver diagnostics** for the current early
PCI/device model. A successful binding remains ownership bookkeeping, not proof
of BAR activation, DMA, interrupts, useful device I/O, USB child-device
coverage, hotplug, or physical Target 001 support.

**Verified M18 hardware compatibility/quirk reporting (PR #245):**
[Full CI run 36615272673](https://github.com/mixutin/Vibrix/actions/runs/36615272673)
and dedicated hardware-compatibility
[run 36615272434](https://github.com/mixutin/Vibrix/actions/runs/36615272434)
passed on implementation head `6dac5ed85b31a3a33db57ce3e25ce962bc74406f`.
The production diagnostic path reports discovered PCI devices, whether a current
driver candidate exists, and known implementation limitations such as the
existing 32-byte xHCI context boundary without conflating binding with working
hardware I/O.

This checks **Hardware compatibility/quirk reporting** for the current bounded
PCI/device model. It does not claim exhaustive hardware coverage, hotplug,
successful driver activation, or physical Target 001 compatibility.

**Verified M18 architectural fault context (PR #243):**
[Actions run 36618153044](https://github.com/mixutin/Vibrix/actions/runs/36618153044)
passed the full exact-head repository matrix and the dedicated
[fault-context run 36618152425](https://github.com/mixutin/Vibrix/actions/runs/36618152425)
on implementation head `b937f1cfae95d0eec70c97b36e684b284f70a039`.
Fatal x86-64 diagnostics now preserve the CPU-pushed RIP, CS, RFLAGS, RSP and SS
frame plus the architectural error code; page faults also retain CR2 and decoded
fault bits. The existing managed-VM page-fault oracle remains unambiguous.

This checks **Register/fault context in crash diagnostics** for the current x86-64
fatal exception path. General-purpose-register dumps, symbolized stacks and
persistent crash records remain separate work.

**Verified M18 privacy-bounded support bundle (PR #248):**
[Actions run 36616277532](https://github.com/mixutin/Vibrix/actions/runs/36616277532)
passed the full exact-head repository matrix on implementation head
`d06e3fadb1820cf4447954768760511e4c3f0cf4`. Native Ring 3
`vibrix doctor --bundle` emits only architecture, shell version, aggregate
process count and fixed diagnostic availability/pass states. It excludes file
contents, PID lists, memory addresses, hardware identifiers, environment data
and shell history, and performs no upload automatically.

This checks the bounded **privacy-reviewed support bundle** item only. Automatic
report submission remains absent by design; richer persistent/network/update
health requires those subsystems to exist.

**Verified M18 bounded filesystem/network/update health checks (PR #324):**
All 49 exact-head workflows completed successfully on implementation head
`68ae7f6e9104000b29c58c039355c1a0d72c2598`, including native userspace
CLI evidence. `vibrix doctor` now reports bootstrap filesystem health from real
Ring-3 syscall-backed root, welcome-file and devfs checks, while network and
update health remain explicitly LIMITED until those live/persistent states are
exposed to userspace. The privacy-bounded support bundle carries the same
distinction instead of inventing a PASS for unavailable state.

This checks **Filesystem/network/update health checks** at the current bootstrap
boundary. It does not claim a live userspace network-management API, persistent
update-state inspection, repair actions, or physical Target 001 storage/network
health validation.

**Exit:** common boot, driver, storage, update and network failures can be
diagnosed from Vibrix itself with useful logs and an exportable support bundle.

**Verified M18 explicit-opt-in anonymized compatibility report (PR #257):**
the exact-head repository matrix passed before merge. Native Ring 3 requires
`vibrix compat-report --anonymized` explicitly, emits a bounded local text
report with coarse support/limitation states, performs no upload, and excludes
device identifiers, serials, network addresses, PIDs, paths, file contents and
history.

This checks only the **optional anonymized compatibility report with explicit
opt-in** item. It does not create telemetry, remote submission, or an automatic
reporting service.

**Verified M18 explicit verbose boot mode (PR #280):**
all exact-head workflows passed implementation head
`4b51fccdd2b4d3b9b5f38676891d7f69dde535e1`, including dedicated two-mode
QEMU evidence. Normal boot contains no verbose markers, while an explicit
feature-enabled boot emits bounded BootInfo and PCI summaries over native
serial/debugcon.

This checks **Verbose boot mode while normal boot remains clean** only. It does
not add persistent logging, crash storage, or a general tracing facility.

## M19 — Isolation and security workstation

This extends the security roadmap; it does not replace the security gates that
must already exist before third-party software is trusted.

- [x] Application sandbox primitives
- [x] Package/application capability declarations
- [ ] Filesystem namespace/mount isolation
- [ ] Network namespace/isolation
- [ ] Device-access mediation
- [ ] Per-application resource limits
- [ ] Audit log for security-sensitive operations
- [ ] Security-lab disposable environment integration
- [ ] Read-only forensic mounting mode
- [ ] Package permission review before installation
- [ ] Hardened developer/debug mode separation

**Verified M19 package/application capability declarations (PR #306):**
all 45 exact-head workflows passed implementation head
`301040cbdc77ffe348cd2f9265d7ecb44bc2dfce`, including dedicated
[capability-declaration evidence run 36679673363](https://github.com/mixutin/Vibrix/actions/runs/36679673363).
The canonical `VCAPv001` sidecar binds a package name to a typed, fail-closed
authority bitset for filesystem read/write, network, device and process-control
access. Unknown authority bits and reserved fields are rejected, zero authority
is explicit, and the native package probe executes the same declaration codec
inside Vibrix Ring 3.

This checks **Package/application capability declarations** as a declaration
format/API. Package permission review and actual sandbox enforcement remain
separate M19 work.

**Exit:** optional engineering/security tooling can be used without automatically
receiving unrestricted access to the persistent Vibrix system.

**Verified M19 bounded application sandbox primitives (PRs #291, #295, #302 and #309):**
the exact-head workflow matrices for the component implementations passed before
merge. Vibrix process identity now combines a monotonic no-new-privileges flag,
monotonic process-operation promise reduction, per-descriptor rights reduction,
and a component-aware monotonic path visibility/access allow-list. Child
processes inherit the already-reduced process policy, and attempts to regain
discarded authority fail closed.

This checks **Application sandbox primitives** as the current composable
authority-reduction foundation. It does not claim filesystem or network
namespaces, device mediation, resource limits, service jails, complete pathname
enforcement for every future syscall, or a finished application sandbox
orchestrator. Those remain separate M19/M23 items.

## M20 — Self-hosted engineering workstation

M15 proves self-hosting fundamentals; M20 turns them into a sustainable
day-to-day development environment.

- [ ] Native Vibrix SDK
- [ ] Rust toolchain packaged through `vpm`
- [ ] Debugger and profiler packages
- [ ] Local API/manual documentation
- [ ] Reproducible package build environment
- [ ] Build recipes usable entirely on Vibrix
- [ ] Build and test third-party Rust applications on Vibrix
- [ ] Build signed Vibrix packages on Vibrix
- [ ] Build/update the Vibrix system from Vibrix
- [ ] Produce and verify a complete bootable USB release from Vibrix

**Exit:** a developer can boot Vibrix, diagnose it, write software, build
packages and produce a verifiable Vibrix release without depending on a
different host operating system.

## M21 — BSD-class base system and administration

Long-term goal: make Vibrix usable as an independent security-focused Unix
system in the same problem space as the BSDs. This is **not** a fork of OpenBSD,
does not copy OpenBSD implementation code, and does not promise binary or source
compatibility unless a later item explicitly says so.

- [ ] signals with per-process masks and default/ignored/caught actions
- [ ] process groups, sessions and controlling terminals
- [ ] PTYs and termios-style terminal control
- [ ] general multi-process exec/spawn and shell job control
- [ ] symlinks, hard links and filesystem link-count semantics
- [ ] mount table and persistent mount configuration
- [ ] persistent account/group database and password-hash policy
- [ ] getty/login/session lifecycle
- [ ] least-privilege administrative command broker
- [ ] service manager with enable/disable/start/stop/reload/status
- [ ] ordered boot/service dependency policy
- [ ] periodic job scheduler
- [x] sysctl-like runtime/query interface
- [ ] complete base-system manual pages
- [ ] coherent /etc-style system configuration with atomic updates
- [ ] rescue/single-user administrative mode

**Verified M21 bounded sysctl-like runtime/query interface (PR #305):**
all 45 exact-head workflows passed implementation head
`b75fe21093df66e1d10bdefe412c0e5db8082912`, including dedicated
[sysctl evidence run 36679668384](https://github.com/mixutin/Vibrix/actions/runs/36679668384).
The native Ring 3 shell exposes a read-only `sysctl` built-in for documented
identity, live PID/process-count and current root/dev mount state. `sysctl -a`
enumerates only the bounded namespace; unknown names and assignment syntax fail
closed. Syscall-backed values are collected before line emission so shared
kernel serial diagnostics cannot corrupt machine-readable output.

This checks **sysctl-like runtime/query interface** for the current bounded
read-only namespace. Mutable tunables, persistence and privileged configuration
remain separate work.

**Exit:** an administrator can boot, log in, manage users/services/filesystems,
inspect system state and perform routine maintenance without another OS.

## M22 — Network security and administration

- [x] IPv6 core, neighbor discovery and ICMPv6
- [x] routing table and route-selection policy
- [x] loopback and Unix-domain sockets
- [ ] poll/select/event-notification API for network daemons
- [x] stateful packet filter with default-deny policy option
- [ ] NAT and port redirection
- [x] anti-spoofing and fragment/resource limits
- [ ] interface configuration utility
- [ ] route and neighbor inspection utilities
- [x] resolver configuration and local caching resolver option
- [x] NTP client with clock-discipline policy
- [ ] SSH client
- [ ] privilege-separated SSH server
- [x] network services disabled by default unless explicitly enabled
- [ ] per-service user, filesystem and network sandbox policy
- [x] packet-filter ruleset validation before activation

**Verified M22 bounded loopback and Unix-domain datagram sockets (PR #330):**
all 52 exact-head workflows passed implementation head
`986310f8b433c71a3e416e9b535303a8f01db18a`. The kernel provides a fixed-capacity
loopback datagram queue plus pathname-bound Unix-domain datagram endpoints with
bounded addressing and deterministic failure behavior.

This checks **loopback and Unix-domain sockets** for the current datagram-only
primitive. It does not claim Unix stream sockets, ancillary data, credential
passing, namespace isolation, poll/select readiness, or full POSIX socket
semantics.

**Verified M22 bounded IPv6/ICMPv6 and neighbor discovery core (PR #321):**
all 52 exact-head workflows passed implementation head
`a22d515e85f1d61eca8f393717dc8535fcc25a2b`, including dedicated
[IPv6 and neighbor discovery run 36692761724](https://github.com/mixutin/Vibrix/actions/runs/36692761724).
The fixed-header IPv6/ICMPv6 path validates lengths and pseudo-header checksums,
handles Echo Request/Reply, validates Neighbor Solicitations with the required
hop-limit 255, and generates solicited Neighbor Advertisements only for the exact
local target.

This checks the bounded **IPv6 core, neighbor discovery and ICMPv6** item. It
does not claim IPv6 routing, SLAAC/DHCPv6, extension headers, fragmentation,
multicast listener discovery, live NIC integration, or physical-network
interoperability.

**Verified M22 packet-filter ruleset activation validation (PR #266):**
the exact-head repository matrix passed before merge. The fixed-capacity control
plane rejects malformed prefixes, invalid port ranges, invalid protocol/port
combinations, empty rulesets and capacity overflow before publication, and a
failed validation leaves the previously active generation unchanged.

This checks **packet-filter ruleset validation before activation** only. It does
not claim that packet filtering, NAT, forwarding, or a firewall dataplane is
implemented yet.

**Verified M22 bounded IPv4 routing and ingress protection (PRs #289 and #290):**
the complete exact-head workflow matrix passed on implementation heads
`1cfb65e3a0779bce6695afc9410d7aaf16e682c4` and
`f64d4295898f5401fdf3ee126703e612c5b30688`. The ingress guard applies
bounded source-address anti-spoofing and fragment/resource admission policy,
while the routing table provides fixed-capacity longest-prefix route selection
with validated entries and deterministic lookup.

These check **anti-spoofing and fragment/resource limits** and **routing table
and route-selection policy** for the current bounded IPv4 control/data-policy
layer. They do not claim IPv6, forwarding/NAT, a complete stateful firewall,
physical NIC routing, or administrator-facing route utilities.

**Verified M22 bounded stateful IPv4 packet filter (PR #297):**
all 39 exact-head workflows passed implementation head
`63d3040a5875800494b5baf81576b628b35d0c58`, including dedicated
[stateful-filter run 36674550154](https://github.com/mixutin/Vibrix/actions/runs/36674550154).
The validated control plane now feeds a fixed-capacity first-match IPv4
dataplane with explicit default pass/block policy and bounded bidirectional
TCP/UDP flow state. Successful policy replacement flushes remembered state,
while failed validation preserves both the active ruleset and state.

This checks the bounded **stateful packet filter with default-deny policy
option** item. It does not claim NIC-hook integration, forwarding/NAT, IPv6,
timeouts, logging, administrator UX, or physical firewall throughput.

**Verified M22 bounded NTPv4 client and clock-discipline policy (PR #304):**
all exact-head workflows passed implementation head
`c1e3d3f59140b968b7ae100228a354b634b68aa4`, including dedicated
[NTP evidence run 36676636895](https://github.com/mixutin/Vibrix/actions/runs/36676636895).
The bounded RFC 5905 client validates response/version/mode/origin fields,
computes four-timestamp offset and delay, and applies explicit Ignore/Slew/Step
policy thresholds with malformed-response coverage.

This checks **NTP client with clock-discipline policy** as a protocol and policy
primitive. It does not claim persistent wall-clock state, hardware RTC
synchronization, authenticated NTS, or live external-network service operation.

**Exit:** Vibrix can act as a defensible workstation or small server with
auditable network configuration and no surprise listening services.

**Verified M22 network administration policy slices (PRs #281 and #284):**
all exact-head workflows passed both implementation heads
`5b4faf05b824e239204be2469435619a3937c3f4` and
`beb7a659cff04debe2715f556fb70c52da328e45`. PR #281 provides a
deny-by-default service-start policy for SSH/resolver/NTP slots; PR #284 adds
bounded resolver configuration and a fixed-capacity DNS A cache with strict
name validation, TTL expiry and deterministic replacement.

These check **network services disabled by default unless explicitly enabled**
and **resolver configuration and local caching resolver option** as bounded
policy/control-plane items. They do not claim running SSH/NTP daemons, DNSSEC,
persistent resolver state, or a complete network service manager.

## M23 — Process hardening and sandboxing

- [x] monotonic process-operation promise API inspired by capability reduction
- [x] path visibility/access allow-list API
- [x] descriptor-rights restriction
- [x] no-new-privileges process flag
- [ ] privilege-separated daemon patterns in the base system
- [ ] chroot/service-jail style filesystem roots
- [x] immutable and append-only file flags
- [ ] per-process CPU/memory/file/socket resource limits
- [x] core-dump policy that excludes secret material
- [ ] stack canaries for supported userspace toolchains
- [ ] PIE/ASLR for base-system executables
- [x] kernel address randomization design and threat model
- [ ] RELRO-like relocation hardening when dynamic linking exists
- [x] exploit-mitigation regression suite
- [x] fuzz and hostile-input tests for every privileged daemon/parser

**Verified M23 exploit-mitigation regression suite (PR #261):**
the exact-head repository matrix passed before merge. The dedicated suite
regresses mitigations Vibrix already implements: userspace ELF W^X admission,
guarded user stacks, fail-closed secure-random policy, a real supervisor
write-protection page fault, and a real CPL3 stack-guard page fault.

This checks the current **exploit-mitigation regression suite** item. It does not
claim unimplemented mitigations such as userspace ASLR, KASLR execution,
compiler canaries, RELRO, IOMMU enforcement, or broad fuzzing.

**Verified M23 monotonic descriptor-rights restriction (PR #302):**
all exact-head workflows passed implementation head
`6f09d2c5e001857b49047efefdaada388f294350`, including dedicated
[descriptor-rights evidence run 36677091836](https://github.com/mixutin/Vibrix/actions/runs/36677091836).
Each descriptor carries an independent read/write/seek rights mask, duplication
inherits the already-reduced mask, close clears rights before reuse, and the new
ABI-v1 query/restrict operation is monotonic: rights can be removed but not
regained.

This checks **descriptor-rights restriction** for the current file-descriptor
model. It does not claim path allow-lists, namespaces, service jails or a general
capability object system.

**Verified M23 monotonic path visibility/access allow-list (PR #309):**
all 48 exact-head workflows passed implementation head
`71b9f49eded8dadf76391f0ca2c2304c4ab6dcb0`, including dedicated
[path-visibility evidence run 36689891400](https://github.com/mixutin/Vibrix/actions/runs/36689891400).
The fixed-capacity per-process policy uses component-aware path matching,
permits restrictions only to shrink, and is inherited by child processes.

This checks the bounded **path visibility/access allow-list API** primitive.
It does not claim pathname-syscall enforcement across every VFS operation,
mount namespaces, chroot/service jails, or complete application sandboxing.

OpenBSD documents monotonic syscall restriction with `pledge(2)` and
path visibility restriction with `unveil(2)`. Vibrix may adopt comparable
security goals, but the API and implementation must be independently designed
for Vibrix rather than copied.

**Verified M23 monotonic no-new-privileges flag (PR #291):**
the complete exact-head workflow matrix, including dedicated
`No-new-privileges evidence`, passed on implementation head
`492a7b3c991a0fb67cb4083b87f8f110a88c2be0`. Process identity now carries a
monotonic no-new-privileges state that can transition only from false to true,
is inherited by children, and rejects attempts to regain privilege through the
bounded credential transition model.

This checks **no-new-privileges process flag** only. It does not claim pledge-
style syscall promises, path allow-lists, descriptor-right reduction, service
jails, namespaces, or complete privilege-separated daemon integration.

**Verified M23 monotonic process-operation promises (PR #295):**
all 37 exact-head workflows passed implementation head
`a4ffc62e90413618f143f45863fa9c2fe16259b3`, including dedicated
[process-promises run 36674421844](https://github.com/mixutin/Vibrix/actions/runs/36674421844).
The additive ABI v1 promise syscall starts processes with four bounded operation
classes (I/O, filesystem, process and credentials), permits only monotonic mask
reduction, inherits the reduced mask across child creation, rejects attempts to
regain removed authority, and enforces the mask before process-facing syscall
actions are constructed.

This checks the bounded **monotonic process-operation promise API** primitive.
It does not claim path visibility, per-descriptor rights, network/device
namespaces, service jails, resource limits, or complete daemon sandboxing; those
remain separate M19/M23 work.

**Verified M23 privileged-parser hostile-input coverage (PR #292):**
all exact-head workflows passed implementation head
`7aa687cd6b8fcbba42f193fb54c157bfe12e032e`, including dedicated
[hostile-input run 36666696289](https://github.com/mixutin/Vibrix/actions/runs/36666696289).
The maintained inventory covers every privileged parser/state-machine class
currently present in the tree: ELF/BootInfo/ACPI/PCI, USB/SCSI/VibrixFS,
kernel networking parsers, package metadata and the native shell parser, and
the umbrella workflow reuses their production-linked hostile/malformed-input
tests while requiring the inventory to stay explicit as new parser classes are
added.

This checks the current **fuzz and hostile-input tests for every privileged
daemon/parser** roadmap item for parser classes that exist today. It does not
claim coverage for future daemons/parsers, formal verification, exhaustive
state-space exploration, or memory-unsafe external components.

**Verified M23 immutable and append-only VFS flags (PR #319):**
all exact-head workflows passed implementation head
`b659b0f14cef754533592a3da3b69449719a9529`, including dedicated
[VFS file flags run 36689251608](https://github.com/mixutin/Vibrix/actions/runs/36689251608).
The production VFS enforces immutable and append-only state at its mutation
boundary, preserves the flags in the tested file-node lifecycle, and rejects
disallowed writes/removal instead of treating the flags as descriptive metadata.

This checks **immutable and append-only file flags** for the current in-memory
VFS model. Persistent VibrixFS flag encoding, privileged flag-changing syscalls,
and physical USB persistence remain separate work.

**Exit:** ordinary applications and daemons can permanently discard ambient
authority, and the base system uses those mechanisms by default where practical.

**Verified M23 security-design deliverables (PR #259):**
[Full CI run 36627839509](https://github.com/mixutin/Vibrix/actions/runs/36627839509)
and dedicated design-document
[run 36627839251](https://github.com/mixutin/Vibrix/actions/runs/36627839251)
passed on implementation head `1ddaad65855b63801a6b0dda3560c03efd854bfb`.
The accepted documents define a kernel address-randomization threat model and a
core-dump policy that excludes secret-bearing memory/classes by default.

These check the **kernel address randomization design and threat model** and
**core-dump secret-exclusion policy** deliverables only. They do not claim KASLR
execution, a core-dump implementation, or secret classification enforcement.

## M24 — Unix compatibility and ports readiness

- [x] stable libc/system-call compatibility layer
- [x] documented POSIX compatibility target and conformance matrix
- [x] fork/exec or documented compatible process-creation semantics
- [ ] signals, pthreads and thread-local errno
- [ ] mmap/shared-memory primitives
- [ ] file locking and advisory locks
- [ ] event queue suitable for scalable servers
- [x] dynamic linker/loader design
- [x] shared-library ABI/versioning policy
- [ ] pkg-config/build-tool compatibility layer
- [ ] shell scripting sufficient for conventional build systems
- [ ] ports recipes for representative editors, shells and servers
- [ ] automated upstream-port patch tracking
- [ ] manual-page sections and installed developer documentation
- [ ] compatibility test suite against selected portable Unix software

**Verified M24 compatibility/design deliverables (PR #259):**
[Full CI run 36627839509](https://github.com/mixutin/Vibrix/actions/runs/36627839509)
and dedicated design-document
[run 36627839251](https://github.com/mixutin/Vibrix/actions/runs/36627839251)
passed on implementation head `1ddaad65855b63801a6b0dda3560c03efd854bfb`.
The accepted documents define Vibrix's POSIX compatibility target/conformance
matrix, a future dynamic-linker/loader architecture, and shared-library
ABI/versioning rules with explicit compatibility and security boundaries.

These check **documented POSIX compatibility target and conformance matrix**,
**dynamic linker/loader design**, and **shared-library ABI/versioning policy**
as design deliverables. They do not claim POSIX conformance, a working dynamic
loader, shared objects, or third-party binary compatibility.

**Exit:** a documented subset of portable Unix software can be built, packaged,
updated and operated on Vibrix without Linux emulation.

**Verified M24 stable system-call compatibility layer (existing M5 ABI/library evidence):**
[ABI v1 run 36458069736](https://github.com/mixutin/Vibrix/actions/runs/36458069736)
passed the full repository matrix on implementation head
`a7611798928f8af9737a3cf3537d93a5a72512e4`, freezing syscall ABI version 1,
existing syscall numbers, result encoding and the x86-64 register contract.
[Native Rust syscall-library run 36472077028](https://github.com/mixutin/Vibrix/actions/runs/36472077028)
passed on implementation head `bf2e6c082d35e73bb5ef63ad42074ccbb9504868`,
including host wrapper-policy tests and compilation of the real
`x86_64-unknown-none` assembly backend. Current canonical CI continues to test
the shared ABI contract and the native userspace wrapper crate.

This checks the **system-call compatibility layer** half of the roadmap item:
Vibrix has a versioned, stable userspace/kernel syscall contract plus a native
Rust compatibility wrapper. It does **not** claim a C/POSIX libc, glibc/musl
compatibility, source compatibility with arbitrary Unix software, dynamic
linking, pthreads or complete POSIX semantics; those remain separate M24 work.

**Verified M24 documented compatible process-creation semantics (PR #279):**
the exact-head repository matrix passed implementation head
`0f1f7a7819383542ff0c45d585a14d2b20597a1e`, including the dedicated
process-creation-semantics contract. The accepted contract defines Vibrix's
transactional spawn/exec direction, descriptor and credential inheritance,
security-state handling, and explicitly documents that POSIX `fork()` is not
currently provided.

This checks **fork/exec or documented compatible process-creation semantics**
as the roadmap's documented-compatible alternative. It does not claim a working
general-purpose spawn/exec userspace API or POSIX fork semantics.

## Optional later storage support

Internal NVMe/SATA disks may be supported as user-accessible **data devices**. They are not Vibrix root/system installation targets.

## Future
- [x] aarch64 design
- [ ] aarch64 UEFI boot
- [x] architecture-independent driver boundaries

**Accepted AArch64 platform architecture (ADR 0025):** Vibrix keeps UEFI as
the first firmware interface, defines EL0 userspace / EL1 kernel ownership,
handles either EL2 or EL1 UEFI entry explicitly, uses a 4 KiB translation
granule, GICv3, Arm Generic Timer, PSCI CPU startup, architecture-specific
page-table/TLB/cache backends, and a shared semantic syscall space transported
through `svc #0` with x8 as the syscall number and x0..x5 as arguments.
The first validation target is QEMU `virt` with AArch64 UEFI.

This is a **design completion only**. No AArch64 binary, boot proof, GIC/PSCI
driver, page-table implementation, physical ARM support, Device Tree parser or
architecture-independent driver implementation is claimed. See
[ADR 0025](docs/decisions/0025-aarch64-platform-design.md).


**Verified architecture-independent driver boundary (PR #258):**
all exact-head workflows passed implementation head
`6406c1765bd49068e1091019c5a87de76f38e6c3`, including a no_std AArch64
harness compiling the same transport-neutral `kernel/src/device.rs` matching
and ownership policy used on x86-64. Source guards reject architecture/MMIO/CPU
mechanism leakage from that shared boundary.

This checks **architecture-independent driver boundaries** only. It does not
claim AArch64 boot, ARM device backends, or working hardware drivers on ARM.

## Early non-goals

Do not prioritize internal-disk installation, desktop polish, browsers, GPU acceleration, broad hardware support, POSIX completeness or Linux binary compatibility before the USB-root kernel foundation is reliable.
