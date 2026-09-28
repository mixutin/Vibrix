# NIC abstraction and bounded loopback

Author: **GPT-6 Astra Pro**.

`kernel/src/nic.rs` defines the public `no_std` M10 `NetworkInterface` contract:
a validated unicast MAC, MTU, link state, synchronous transmit, and nonblocking
receive. Frames contain the Ethernet header and payload, but not the preamble
or FCS. Accepted lengths are 14 through 1514 bytes; actual Ethernet hardware
must supply required padding/FCS. VLAN, jumbo frames and offloads are not part
of this initial contract. Transmit acceptance is not proof of remote delivery.

Drivers must copy/consume borrowed buffers before returning; they cannot retain
references for asynchronous DMA. That will require separately owned DMA memory.
Receive returns `None` when empty and never truncates silently. A too-small
buffer is unchanged and the queued frame is retained for a larger-buffer retry.
Exclusive mutable access provides serialization; this is not an IRQ-safe queue.

The reference `Loopback` backend stores four frames in bounded RAM. It provides
FIFO/backpressure, saturating counters and explicit link transitions. Link-down
purges pending traffic so it cannot reappear in another link session. This
backend performs no PCI configuration, DMA, interrupts or actual network I/O.

## Evidence gate

Host tests exercise the production trait, invalid MAC/frame sizes, full-queue
rejection, repeated ring wraparound/FIFO, untouched short buffers, preserved
output tails, counters, and stale-frame disposal on link-down.

The production QEMU-debug kernel calls the same public trait after firmware
exit. It transmits a real 60-byte frame into loopback, rejects a short receive,
then verifies exact bytes/length and an empty queue before emitting
`VIBRIX: kernel NIC abstraction verified` over independent COM1 and debugcon.
The dedicated Actions workflow requires both exact marker lines.

This is the **NIC abstraction and software reference backend**, not the
RTL8168 driver, Ethernet/ARP stack, IP, sockets, network connectivity, or
Target 001 networking. No dependencies or unsafe code are added. No external
implementation source was used.
