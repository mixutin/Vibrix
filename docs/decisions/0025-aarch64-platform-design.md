# ADR 0025 — AArch64 platform architecture

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decision owners:** Vibrix project
- **Roadmap:** Future / aarch64 design

## Context

Vibrix is currently an x86-64 UEFI system. A future AArch64 port must preserve
the project's architecture-independent contracts where that is useful without
pretending that x86 paging, privilege, interrupt, timer, PCI or syscall
mechanisms are portable.

The design is based on the AArch64 UEFI entry contract and Arm exception model,
not on another operating system's implementation.

## Decision

### Firmware and loader

The first AArch64 target remains UEFI. The Rust loader will build for
`aarch64-unknown-uefi` and retain the same high-level phases as x86-64:
locate the kernel image, validate ELF metadata, obtain framebuffer and firmware
tables, construct a final memory-map handoff, call `ExitBootServices`, and
transfer to the kernel without using firmware services afterward.

UEFI 2.11 permits AArch64 firmware to hand off at the highest available
non-secure 64-bit exception level, EL2 or EL1, with the MMU and caches enabled,
little-endian execution, a 4 KiB translation granule and identity-mapped RAM.
The loader therefore records the entry exception level and must not assume EL1.

The existing x86-specific `BootInfo v3` tail is not silently reused. A future
shared handoff revision will preserve architecture-neutral fields such as
framebuffer and memory-map metadata while moving page-table/platform-specific
state into an explicit architecture extension.

### Exception-level transition

Vibrix userspace runs at EL0 and the kernel at EL1.

If firmware enters at EL2, a small audited transition establishes EL1 state and
returns to an EL1 kernel entry. It must explicitly configure the EL2 controls
needed for 64-bit EL1, kernel stack state, exception return state and EL1
translation/system-register state before `ERET`.

If firmware already enters at EL1, the kernel does not manufacture an EL2
dependency.

EL3 remains firmware/secure-monitor territory and is never owned by Vibrix.

### Virtual memory

The initial port uses a 4 KiB translation granule. Kernel and userspace mappings
continue the Vibrix rules already enforced on x86-64:

- kernel mappings are privileged;
- userspace mappings are EL0-accessible only when explicitly requested;
- writable and executable permissions are never combined;
- device memory uses device memory attributes, not normal cacheable RAM;
- user address spaces have separate translation roots from the kernel-facing
  process view.

AArch64 page-table descriptors, MAIR/TCR configuration, ASIDs and TLB
maintenance are implemented in an architecture backend rather than leaking into
the shared VMM API.

### Interrupts, timer and SMP

The first interrupt-controller target is GICv3. Exception vector installation,
IRQ acknowledgement/deactivation and priority configuration live behind the
architecture interrupt boundary.

The monotonic timer uses the Arm Generic Timer rather than emulating the x86
PIT/LAPIC model.

Application processors are started through PSCI `CPU_ON` when firmware
provides PSCI. Per-CPU structures remain architecture-independent above a
platform startup layer. Cross-CPU TLB invalidation uses the architected
AArch64 barrier/TLBI sequence and is not shared with x86 INVLPG/IPI code.

### Syscall transport

Vibrix keeps one semantic syscall-number space across architectures.

The AArch64 ABI maps:

- `x8`: syscall number
- `x0..x5`: arguments 0..5
- `x0`: return value / encoded errno
- `svc #0`: userspace-to-kernel transition

This is an architecture transport mapping only; existing syscall meanings and
numbers do not change merely because the CPU architecture changes.

### Hardware discovery and buses

The first virtual validation platform is QEMU `virt` with AArch64 UEFI.

ACPI remains the preferred standardized firmware description where supplied,
including PCI ECAM discovery. A later firmware-table abstraction may admit
Device Tree without making drivers depend directly on either representation.

PCI identities, driver descriptors, VFS, process state, network protocols and
block abstractions remain shared Rust policy. MMIO access, interrupt routing,
DMA/IOMMU plumbing and cache maintenance are architecture/platform backends.

### Boot and display

UEFI GOP remains the bootstrap display path. Native GPU modesetting is not a
prerequisite for the first AArch64 boot proof.

The first implementation milestone is successful UEFI -> loader ->
`ExitBootServices` -> EL1 kernel entry under QEMU, with serial/debug evidence
and no host OS runtime dependency.

## Compatibility boundaries

This design does **not** claim:

- an AArch64 loader or kernel currently builds;
- QEMU AArch64 boot works;
- GICv3, PSCI, Generic Timer or AArch64 page tables are implemented;
- Device Tree is parsed;
- any physical ARM machine is supported;
- x86-64 `BootInfo v3` can be consumed unchanged by AArch64;
- drivers are already architecture-independent.

Those are separate roadmap implementation items.

## Rejected approaches

### Reuse x86-64 low-level modules with conditional register substitutions

Rejected because page-table descriptors, exception entry, interrupt
controllers, timer facilities, cache maintenance and SMP startup have different
architectural contracts. Shared high-level traits are useful; shared unsafe
mechanism code is not.

### Require firmware to enter EL1

Rejected because the UEFI AArch64 contract permits non-secure EL2 or EL1.

### Define a second incompatible syscall number space

Rejected because architecture-specific transport does not require
architecture-specific syscall semantics.

## Primary references checked 2026-09-29

- UEFI Specification 2.11, section 2.3.6, AArch64 Platforms:
  https://uefi.org/sites/default/files/resources/UEFI_Spec_Final_2.11.pdf
- UEFI specification index, confirming UEFI 2.11 as the current release:
  https://uefi.org/specifications
- Arm AArch64 exception model, EL0/EL1/EL2/EL3 privilege model:
  https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Exception%20model.pdf

Implementation work must consult the current Arm Architecture Reference Manual,
GIC architecture specification and PSCI specification for exact system-register,
barrier and interrupt-controller programming before code is written.
