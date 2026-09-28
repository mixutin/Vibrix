# Managed VM CPU protection probes

Author: **GPT-6 Astra Pro**. Slice 10 of the ordered
[managed-VM stack](MANAGED_VM.md), based on the
[native adapter contract](MANAGED_VM_NATIVE.md). No independent review claimed.

## What runs

The opt-in kernel features use the same production VM and native backend as the
normal boot proof. After that proof has completely torn down its arena and
scratch mappings, a fresh disjoint pool is acquired for one terminal probe.
Deliberately invalid accesses use x86 assembly, not Rust references or volatile
intrinsics whose validity contracts would prohibit trapping memory accesses.

| Probe | Preparation | Required CPU observation |
| --- | --- | --- |
| `write` | Write/warm RW page, protect RO, attempt another write | CR2 `0xffffd00000000000`, #PF error `0x3` |
| `unmap` | Write/read live page, unmap and invalidate, read again | Same CR2, #PF error `0x0` |
| `guard` | Allocate reserved lower/upper guards around one RW payload page, verify payload, read upper guard | CR2 `0xffffd00000002000`, #PF error `0x0` |
| `nx` | Put a RET byte into owned RW/NX RAM, attempt an indirect call | CR2 and RIP `0xffffd00000000000`, #PF error `0x11` |

If an access unexpectedly returns, the kernel panics and the validator rejects
the run. These probes are not enabled by normal builds and do not install new
exception handlers or relax the existing IDT path. An all-features lint build
selects write first; each runtime matrix job enables exactly its one probe.

## Run locally on a configured development host

```sh
./tools/test-managed-vm.sh write
./tools/test-managed-vm.sh unmap
./tools/test-managed-vm.sh guard
./tools/test-managed-vm.sh nx
```

The runner only uses the existing disposable QEMU ESP directory and copied
OVMF variable store. It never opens a physical disk. Each invocation rebuilds
with its explicit feature, removes stale logs and executes real QEMU/TCG.

The independent Python validator requires ordered, exact kernel debug markers
and matching COM1 records. It checks the complete CR2/error values, non-user
fault address context, decoded P/W/U/reserved/instruction-fetch flags, and NX's
exact fault RIP. Duplicate, incomplete, reordered, mismatched-probe, panic,
#GP/#DF or resumed-console output fails. Numeric prefix matching cannot turn
`0x30` into a successful `0x3` result. Parser unit fixtures deliberately mutate
records to test rejection; they are **not guest execution evidence**.

## Completion boundary

The dedicated Actions matrix must pass on the exact PR head before these CPU
properties are claimed. Host/parser success alone is insufficient. The author
has no local Rust/QEMU runtime and records observed Actions evidence in the PR.

This proves a bounded BSP-only managed supervisor arena, not global kernel
W^X, user address spaces, demand paging, SMP shootdowns, permanent stack/IST
protection, a growing heap, hardware Target 001 or the whole M3 VMM milestone.
The roadmap remains unchecked for those broader requirements. The normal
native path retains 96 KiB of monotonic allocator supply; a terminal probe
retains a second 96 KiB pool until its intentionally halted VM is discarded.
