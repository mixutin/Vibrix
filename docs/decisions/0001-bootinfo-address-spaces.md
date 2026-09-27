# ADR 0001: BootInfo address spaces across ExitBootServices

- **Status:** Proposed
- **Date:** 2026-09-27
- **Roadmap:** [M2 — Firmware-to-kernel handoff](../../ROADMAP.md)
- **Supersedes:** None

## Context

The Vibrix UEFI loader has discovered an ACPI RSDP and a GOP framebuffer,
but it has not yet transferred execution to the standalone kernel.
The current `BootInfo` layout in `kernel/src/main.rs` has `u64`
fields named `framebuffer_base`, `rsdp`, and `memory_map`. The entry
function receives `*const BootInfo`. These values cannot safely be
treated as interchangeable physical and virtual addresses once the loader
transfers control under the kernel's page tables.

Vibrix must later reacquire its removable boot USB device through native
drivers after firmware boot services are gone. Firmware handles are not
persistent device identities or native driver interfaces.

## Decision (proposed; not yet an implemented contract)

For the initial x86-64 handoff:

1. **Physical-address fields:** `framebuffer_base` and `rsdp` denote
   physical addresses. `memory_map` denotes the physical address of a
   loader-owned contiguous copy of the final UEFI memory-map descriptor
   buffer. These integer fields are *not* directly dereferenceable Rust
   references.
2. **Kernel entry argument:** `*const BootInfo` is a virtual pointer valid
   under the page tables active on entry to `vibrix_kernel_entry`. The
   loader must map its backing allocation through the transfer. The
   kernel must not infer a physical address from the pointer's numeric
   value.
3. **Mapped access:** the kernel must explicitly map the framebuffer and
   ACPI structures before accessing their physical addresses. Framebuffer
   mappings need a deliberately chosen cache policy. Pointer provenance,
   ranges and access rights require checks; an address being nonzero
   proves nothing about accessibility.
4. **Memory-map representation:** `memory_map_len` is a byte count, not
   a descriptor count. `memory_descriptor_size` is the descriptor stride
   in bytes returned by firmware; consumers must check the minimum
   supported prefix size, a nonzero stride and whole-descriptor length.
   Consumers must not blindly step by Rust's struct size.
5. **Buffer lifetime:** allocate and map `BootInfo` and the map buffer
   before obtaining the final map/key pair. Keep their pages reserved
   until the kernel has safely consumed or copied their contents.
   Preserve relevant ACPI/framebuffer mappings independently of boot
   services.
6. **Exit sequence:** provision a memory-map buffer with headroom before
   the final `GetMemoryMap`. Avoid operations that change the map
   between acquiring a valid map/key and calling `ExitBootServices`.
   If the key is rejected, obtain a fresh map/key using the UEFI retry
   requirements; never reuse the stale key. Do not call boot services
   after successful exit.
7. **Versioning gap:** firmware reports a memory-descriptor *version*,
   which the present `BootInfo` does not include. Before implementing
   handoff, a separate coordinated ABI PR must define how the kernel
   receives or verifies the descriptor version, including the required
   `BootInfo` version/layout change if a field is added. This ADR does
   **not** silently alter the struct layout.
8. **Boot USB identity:** neither a firmware handle nor a CPU vendor,
   motherboard path or arbitrary storage controller alone proves the
   boot USB identity after firmware exit. Define its own boot-device
   identity/reacquisition contract before attempting persistent root
   mounting. Internal NVMe/SATA may become optional data devices, never
   the Vibrix system installation target.

A proposal is not an accepted contract. Implementation belongs in
separate focused PRs after another agent reviews the address model.

## Alternatives considered

- **All fields are virtual pointers:** misleading for firmware-discovered
  physical addresses and unsafe without explicit mappings.
- **The entry `*const BootInfo` is a physical pointer:** Rust dereferences
  pointers in the current address space; a new page-table layout makes
  the interpretation unreliable.
- **Pass active UEFI protocol pointers as the long-term interface:**
  incompatible with the post-`ExitBootServices` native-driver boundary.
- **Immediately replace the UEFI memory map with a custom Vibrix
  descriptor ABI:** potentially worthwhile later, but requires its own
  layout/versioning and runtime-attribute compatibility design.

## Consequences and compatibility

The existing Rust `BootInfo` layout remains unchanged by this
documentation PR. Future loader and kernel implementations will need to
coordinate early mappings, allocation ownership and memory-map parsing.
An accepted ADR is not evidence of successful boot or physical hardware
support. If review changes the proposed address semantics, update this
ADR before implementing them rather than declaring a hypothetical ABI
backwards compatible.

This design does not persist CPU-, motherboard- or controller-specific
hardware identities on Vibrix's removable USB drive.

## Validation plan and evidence

**Previously observed on current main:** CI has built the loader and
kernel and QEMU has exercised ELF validation plus ACPI and GOP discovery.
None of those markers demonstrate kernel entry or successful
`ExitBootServices`.

Before checking the affected M2 handoff items:

1. Run format, host parser tests, both Clippy targets and loader/kernel
   builds on the synchronized branch.
2. In QEMU/OVMF, demonstrate a successful final map acquisition,
   `ExitBootServices` and an independent kernel marker emitted
   without calling firmware boot services.
3. Exercise short-map-buffer, invalid descriptor stride,
   overflow, and stale-key retry behavior with controlled failures.
4. Prove that the active page tables map `BootInfo`, its descriptor
   copy and the necessary ACPI/framebuffer ranges, with reserved pages
   protected from allocator reuse.
5. Test Target 001 and native USB reacquisition separately before
   claiming either physical target or persistent-root completion.

**This PR is documentation only; it runs no kernel-entry or hardware
validation and completes no new roadmap checkbox.**

## References

- [UEFI specification](https://uefi.org/specifications): memory allocation
  services, `GetMemoryMap`, `ExitBootServices`, configuration tables and GOP.
- [Vibrix Boot ABI](../BOOT_ABI.md)
- [Vibrix architecture](../ARCHITECTURE.md)
- [Vibrix USB system model](../USB_MODEL.md)

This proposal does not use third-party operating-system implementation
source.
