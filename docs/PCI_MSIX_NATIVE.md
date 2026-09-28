# Native QEMU MSI-X delivery evidence

Author: **GPT-5.6 Sol**.

This is a bounded test-only completion proof for the M4 MSI/MSI-X lane. It
uses the already merged production capability parser and MSI-X programming
contract; it is not a general ivshmem driver.

## Device and host protocol

The probe uses QEMU `ivshmem-doorbell` with exactly one vector. QEMU exposes
BAR0 for registers, BAR1 for the MSI-X table/PBA and BAR2 for shared memory.
The test server implements the documented one-way ivshmem client-server
protocol: version 0, client ID 0, shared-memory FD, then one receive eventfd.
The server writes 64-bit value 1 to that eventfd only after guest debug markers
prove the preceding state transition completed.

QEMU's MSI-X BAR is treated as a dedicated 4 KiB extent. The guest requires
the capability table and PBA to fit that page, rejects any different identity,
revision, BAR type or vector count, and never sizes a live BAR by writing ones.

## Guest safety boundary

The sole BSP runs under TCG. MSI-X setup happens with IF=0 and exclusive legacy
PCI configuration access. Memory decoding, INTx disable and bus mastering are requested only for the
isolated proof because QEMU routes MSI-X message writes through the PCI
bus-master address space. No data-DMA engine or shared-memory transfer is
programmed, and BME is cleared immediately after verified MSI-X disable. BAR1
is mapped UC through the existing
BootInfo v3 window only long enough to program and read back vector zero, then
the CPU mapping is removed before STI.

Vector 0x51 has a permanent IDT gate. The interrupt handler performs no
allocation, logging or device MMIO: it increments one atomic delivery counter
and sends LAPIC EOI.

## Evidence sequence

The host sends three real eventfd notifications:

1. after the guest reports MSI-X armed;
2. after the guest has observed the first interrupt;
3. only after the guest has disabled MSI-X with configuration readback.

The guest must observe exactly two deliveries. After disable, it waits across
five real PIT ticks and requires the count to remain two, then continues to the
normal console. CI checks paired guest/server logs, including all three host
eventfd writes.

This demonstrates isolated QEMU MSI-X delivery and disable behavior. It does
not provide generic vector allocation, SMP interrupt remapping/shootdown,
hotplug teardown, IOMMU isolation, a production ivshmem driver or Target 001
hardware evidence.
