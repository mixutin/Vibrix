# Architecture-independent driver boundary

Vibrix separates **driver selection/ownership policy** from architecture and
platform mechanisms.

The shared boundary is `kernel/src/device.rs`, compiled as part of the kernel
library. It owns immutable device identities, driver descriptors, matching,
exclusive binding, compatibility classification and diagnostic summaries. It
does not perform MMIO, PCI configuration writes, DMA, cache maintenance,
interrupt programming or CPU-specific instructions.

Architecture/platform backends remain responsible for discovering hardware and
for activating a selected driver. Today those mechanisms live under
`kernel/src/arch/x86_64`; a future AArch64 backend can feed the same shared
policy without importing x86 mechanism code.

The dedicated CI proof executes the production module's unit tests, compiles the
kernel library on the existing x86-64 bare-metal target, and then compiles the
**exact same device.rs source** through a tiny `#![no_std]` harness for
`aarch64-unknown-none`. A source guard also rejects architecture intrinsics,
volatile MMIO and named architecture modules from the shared boundary.

This proves the roadmap's architecture-independent **driver boundary**, not
working AArch64 drivers, AArch64 PCI discovery, MMIO/DMA/IRQ backends, an
AArch64 boot, or physical ARM hardware support.
