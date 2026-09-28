# Single-vector MSI setup

Author: GPT-6 Astra Pro.

This dependent M4 increment builds on the checked conventional capability
inventory. `shared/pci_interrupts/msi.rs` implements actual configuration
transport calls, not a document-only proposed register sequence. Native
transport wiring and QEMU delivery are separate evidence requirements.

The caller supplies exclusive, serialized access to one stable function,
quiesces its interrupt sources and disables legacy INTx. A permanent handler
and live device state must already exist. The target must be physical xAPIC
with interrupt remapping disabled. The message constructor deliberately
reserves vectors below 0x50 and at/above 0xf0 for kernel policy and rejects
the broadcast APIC ID. It is not a vector allocator or an ownership token.

Setup rediscovers the entire capability chain and rejects a stale inventory,
already enabled MSI/MSI-X, or MSI extensions outside the supported control
layout. It selects exactly one message, disables MSI before editing, masks
all supported messages where available, programs address/data and checks
readback, unmasks only message zero and enables MSI last. WORD writes must
be real WORD transactions; no DWORD read-modify-write is permitted for them.
PCI command/status, BARs, DMA and pending registers are untouched.

Failure performs only a best-effort disable/mask. A failed transport can
leave hardware state indeterminate, and an interrupt may already be in
flight. Therefore every error requires retaining vector, handler and mapped
device resources until the owner establishes quiescence. A successful MSI
disable also does not, by itself, prove the interrupt pipeline is drained.

Host tests exercise all four conventional MSI layouts, 32-message masking,
register widths/order, stale/active capability rejection, each write-failure
position and ignored writes. They import the same source later used by the
kernel. Exact-head CI must pass; mocked register readback is not proof of
real interrupt delivery. The broad MSI/MSI-X roadmap checkbox stays open.

References: PCI Local Bus MSI capability interface and
[Intel SDM volume 3A, Message Signalled Interrupts](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).
No third-party implementation or runtime dependency was introduced.
