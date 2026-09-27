# Vibrix Architecture

## Scope

Vibrix is a 64-bit, UEFI-booted Unix-like operating system for x86-64 whose persistent system lives on removable USB storage.

The first development platform is QEMU. The first physical reference is Target 001.

Vibrix has **no internal-disk edition**. The boot USB becomes the persistent root/system device after the kernel takes control.

## Boot model

    UEFI firmware
        |
    Vibrix UEFI loader on USB
        |
    Vibrix kernel
        |
    USB/xHCI discovery
        |
    reacquire boot USB
        |
    mount persistent Vibrix root
        |
    memory + interrupts + scheduler
        |
    userspace
        |
    init -> shell -> applications

The loader and kernel are Vibrix code. UEFI firmware is a platform interface, not part of the operating system.

## Kernel direction

The initial design is a pragmatic monolithic kernel with explicit subsystem boundaries.

Expected subsystems:

- architecture layer
- physical and virtual memory
- scheduler/processes
- syscall layer
- VFS
- USB/xHCI
- removable block devices
- device/driver model
- IPC
- networking
- TTY
- security/credentials

## Persistent USB storage model

The USB device is not merely boot media. It is the long-lived system disk.

A mature Vibrix USB is expected to contain:

    GPT
    ├── EFI System Partition
    │   └── EFI/BOOT/BOOTX64.EFI
    ├── Vibrix system/root filesystem
    │   ├── system
    │   ├── packages
    │   ├── configuration
    │   └── home
    └── optional recovery area

The exact partition/filesystem format is not yet frozen.

After ExitBootServices, Vibrix must rediscover its xHCI controller, enumerate USB devices, identify the boot device, and reacquire persistent block access without depending on firmware storage services.

## Volatile data

Paths such as /tmp, runtime state and selected caches should be RAM-backed or aggressively buffered to reduce flash wear.

## Hardware portability

Because the system drive is meant to move between machines, hardware discovery happens on every boot. Persistent configuration should avoid binding the operating system permanently to one motherboard, NIC, GPU or USB topology.

## Internal disks

Internal NVMe/SATA devices may eventually be exposed as optional data storage. Vibrix does not install itself to them and must never silently repartition or claim them as its root device.

## Third-party boundary

Official Rust toolchain components and reviewed, appropriately licensed community Rust crates are permitted, including in shipped components where they support the required target. Using another operating system as Vibrix's runtime or kernel remains out of scope. See [DEPENDENCIES.md](DEPENDENCIES.md) and [INDEPENDENCE.md](INDEPENDENCE.md).

Development/test tools such as QEMU and OVMF are external infrastructure and are not part of the Vibrix runtime.

## x86-64 CPUID foundations

The kernel enumerates one CPU with the official Rust `core::arch::x86_64::__cpuid_count` intrinsic on entry. The snapshot includes the 12-byte vendor string and APIC, x2APIC, NX, SMEP and SMAP feature bits. Optional leaves are queried only if their basic or extended maximum permits them. Discovery does **not** enable those features and does not persist CPU-specific choices to the removable USB.

The decoder has synthetic host-side tests and is now also executed during the standalone kernel's real post-ExitBootServices QEMU entry path, before the kernel GDT/TSS success marker. This establishes one-CPU QEMU discovery, not physical Target 001 or SMP validation.

Primary references: [Rust core x86-64 CPUID intrinsic](https://doc.rust-lang.org/core/arch/x86_64/fn.__cpuid_count.html), Intel 64 and IA-32 SDM CPUID instruction and extended feature enumeration, AMD64 Architecture Programmer's Manual volume 3.
