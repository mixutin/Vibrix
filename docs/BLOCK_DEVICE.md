# Bounded block-device abstraction

Author: **GPT-6 Astra Pro**.

## Contract

`kernel/src/block.rs` is the public, `no_std`, allocation-free M7 block API.
`BlockDevice<T>` owns a synchronous transport and immutable validated geometry.
It checks nonempty, whole-sector requests and checked LBA arithmetic **before**
calling the transport. Supported logical sector sizes are powers of two from
512 through 4096 bytes. Zero-capacity and overflowing media are rejected.

The caller owns each borrowed input/output buffer throughout a call. A transport
must complete all access before returning and must never retain a DMA reference.
Future asynchronous DMA needs a different explicit ownership contract. Errors
can represent partial I/O; neither the wrapper nor its callers may infer atomicity
or consume an errored read as valid data. No automatic retries are performed.

Read-only devices reject writes. Durability is independent of write capability:
`Volatile` rejects `flush`, while `FlushSupported` delegates to a transport with
a real ordered, power-loss durability barrier and propagates its failures.
The RAM backend **always** reports volatile storage, never persistent success.

A device handle alone does not establish boot-device identity or permission to
write a physical disk. Removable-root selection still requires ADR 0005 and the
native USB implementation. There is no device opening/provisioning path here.

## Implemented backend and validation

The exclusive borrowed RAM backend copies real bytes through the same checked
interface. Host tests exercise 512/4096-byte sectors, round trips, invalid geometry,
read-only behavior, empty/misaligned/overflowing/out-of-range requests, transport
non-invocation for rejected requests, and propagated transport/flush errors.

The QEMU kernel probe uses the production public library: it writes sector 1,
reads and compares its payload, verifies sector 0 is unchanged, rejects an
out-of-bounds write, and rejects a false durable flush. Only then does the kernel
emit `VIBRIX: kernel block abstraction verified` over COM1 and debugcon.

## Scope

This completes the **abstraction with a RAM reference backend** when its tests
pass. It does not implement USB/SCSI, hardware block I/O, persistence, filesystem
mounting, removable-root reacquisition, or Target 001 support. No dependencies,
unsafe code, firmware calls, physical disk writes or third-party implementation
source are added. Rust core checked arithmetic and borrowing are the only runtime
facilities used.
