# Rescue / single-user administrative mode

Vibrix exposes an explicit `rescue-mode` kernel profile and
`tools/run-qemu.sh --rescue` launcher path.

The profile deliberately reuses the already validated native Ring-3 shell and
process/syscall path rather than introducing a second emergency command
interpreter. The shell runs as the bootstrap PID 1 with root credentials and is
identified before userspace entry as an explicit single-user administrative
session.

The dedicated evidence workflow builds that exact feature set and boots it in
QEMU/OVMF. Success requires both independent kernel outputs to identify rescue
mode, the normal native shell ELF load marker, and PID 1 execution.

## Security boundary

Rescue mode is an explicit operator-selected boot profile. It does not silently
activate during normal boot, does not bypass kernel memory protection, and does
not grant capabilities beyond the root bootstrap credentials already used by
the early native shell.

This bounded milestone does not claim a persistent boot selector, physical USB
recovery environment, password-gated console access, multi-user login,
filesystem repair tooling, or rollback-generation selection. Those remain
separate M16/M21 tasks.
