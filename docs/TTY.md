# Bootstrap TTY contract

The first Vibrix TTY is a bounded canonical terminal device at `/dev/tty`.
It is deliberately small enough to audit and exists to bridge the proven
kernel console to future userspace init and shell work.

## Input

A kernel-owned device driver injects bytes through the VFS device-input hook.
Ordinary consumers read the terminal through normal file descriptors.

Canonical input rules are:

- carriage return and newline commit a line as `\n`;
- Backspace and Delete erase one uncommitted byte;
- Ctrl-U erases the current uncommitted line;
- printable ASCII and tab are accepted;
- other control bytes are ignored;
- reads return `WouldBlock` until at least one line has been committed.

Input storage is fixed at 256 bytes. Capacity exhaustion fails without
overwriting unread data.

## Output

Writes through a normal `/dev/tty` descriptor enter a fixed 512-byte output
queue. A kernel-owned console/serial driver drains that queue through the
device-output hook. A write that does not fit fails with `WouldBlock` before
partial mutation.

The bootstrap TTY does not yet implement termios, signals, job control,
sessions/process groups, UTF-8 editing, terminal escape interpretation,
window-size ioctls, or blocking scheduler wakeups.

## Evidence boundary

Production VFS self-tests exercise canonical edit/read behavior and output
queue draining. The real post-ExitBootServices PS/2 path also injects actual
QEMU keyboard bytes into `/dev/tty`; the existing development console remains
in parallel until userspace init/shell owns the terminal.
