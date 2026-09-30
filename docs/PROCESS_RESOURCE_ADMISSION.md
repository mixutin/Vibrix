# Per-process file descriptor admission

The M23 resource-budget foundation now participates in the real Ring-3 file
descriptor syscall path.

The bootstrap shell's inherited standard descriptors 0, 1 and 2 are charged to
PID 1 before entering userspace. Each later `open` reserves one file slot from
the process budget **before** the VFS can truncate or otherwise mutate a target.
If the VFS open fails, the reservation is rolled back. A successful `close`
releases the process charge only after the descriptor owner has accepted the
close.

This ordering keeps resource-limit failure atomic with respect to the existing
VFS: exhausting the process file budget cannot truncate an existing file, and a
failed VFS lookup cannot leak budget.

The existing process table already enforces child-count limits on
`spawn_child`; this change connects the open-file dimension to the actual
userspace syscall transport.

This remains a partial M23 resource-limits integration. CPU ticks are not yet
charged by the scheduler, user mapping pages are not yet tied to process
lifetime accounting, and the current general socket ABI does not yet have an
owner suitable for socket-budget admission. The broad roadmap item therefore
stays unchecked.
