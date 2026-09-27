# Vibrix Architecture

## Scope

Vibrix begins as a 64-bit, UEFI-booted Unix-like operating system for x86-64.

The first development platform is QEMU. Bare-metal support is introduced deliberately as drivers mature.

## Boot model

```
UEFI firmware
    |
Vibrix UEFI loader
    |
Vibrix kernel
    |
memory + interrupts + scheduler
    |
userspace
    |
init -> shell -> applications
```

The loader and kernel are Vibrix code. UEFI firmware is a platform interface, not part of the operating system.

## Kernel direction

The initial design is a pragmatic monolithic kernel with explicit subsystem boundaries. This keeps early bring-up tractable while leaving room to evolve interfaces later.

Expected subsystems:

- architecture layer
- physical and virtual memory
- scheduler/processes
- syscall layer
- VFS
- block devices
- device/driver model
- IPC
- networking
- TTY
- security/credentials

## Filesystem

Vibrix intends to implement its own persistent filesystem. Early development may use an in-memory bootstrap filesystem implemented by Vibrix itself until persistent block I/O exists.

## Portable edition

The portable edition mounts the removable Vibrix system device as the persistent root filesystem. It is not a disposable live environment.

Volatile paths such as temporary files and selected caches should later be RAM-backed to reduce flash wear.

## Installed edition

The installed edition uses the same kernel and userspace but places the persistent system on an internal disk.

## Third-party boundary

Third-party tools may participate in the build/test environment. Third-party runtime components may not become part of the bootable Vibrix system.
