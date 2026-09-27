# ADR 0001: BootInfo address spaces across ExitBootServices

- **Status:** Accepted
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

Firmware handles are not persistent USB device identities after boot services
end. The separate boot-device reacquisition contract belongs in a future
USB/storage ADR.

## Decision

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
   mappings need a deliberate cache policy, but the choice of WC/UC or
   another supported mode is deferred to the initial page-table and
   framebuffer mapping decision and documented in that implementation.
   Pointer provenance, ranges and access rights require checks; a
   nonzero address proves nothing about accessibility.
4. **Memory-map representation:** `memory_map_len` is a byte count, not
   a descriptor count. `memory_descriptor_size` is the descriptor stride
   in bytes returned by firmware; consumers must check the minimum
   supported prefix size, a nonzero stride and whole-descriptor length.
   Consumers must not blindly step by Rust's struct size.
5. **Buffer lifetime:** allocate `BootInfo` and the memory-map buffer as
   loader-owned `EfiLoaderData` pages and establish necessary virtual
   mappings before obtaining the final map/key pair. They are **not**
   `EfiRuntimeServicesData`: that memory type is reserved for firmware
   runtime state, not Vibrix handoff data. After a successful
   `ExitBootServices`, firmware does not asynchronously reclaim those
   pages; **the Vibrix kernel's early physical frame allocator** must
   reserve them until it has safely consumed or copied their contents.
   Preserve necessary ACPI/framebuffer mappings independently of boot
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
The address contract is **accepted following RIFT's technical review**, but
is not proof that mapping or firmware handoff is implemented. The
boot-device identity/reacquisition design remains a separate USB/storage
ADR rather than a rule defined here.

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

The companion [kernel staging PR #20](https://github.com/mixutin/Vibrix/pull/20)
proposes ADR 0002 for physical backing of higher-half PT_LOAD segments;
that staging design remains separately reviewable. USB boot-device
identity and persistent configuration policy remain separate decisions.

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
5. Test Target 001 separately before claiming physical handoff on the
   named hardware. USB reacquisition and persistent root have their own
   later milestones and are not validated by this ADR.

**This ADR records an accepted architectural decision, not completed
implementation. This PR is documentation only; it runs no kernel-entry
or hardware validation and completes no new roadmap checkbox.**

## References

- [UEFI specification](https://uefi.org/specifications): memory allocation
  services, `GetMemoryMap`, `ExitBootServices`, configuration tables and GOP.
- [Vibrix Boot ABI](../BOOT_ABI.md)
- [Vibrix architecture](../ARCHITECTURE.md)
- [Vibrix USB system model](../USB_MODEL.md)

This proposal does not use third-party operating-system implementation
source.
