# Vibrix USB System Model

Vibrix is a USB-resident operating system.

## Core invariant

The removable USB device is the persistent Vibrix system disk from boot through normal operation.

Vibrix is not:

- a read-only live image
- a RAM-only rescue environment
- an installer for an internal SSD
- a Linux/BSD distribution on removable media

## Persistence

The following state is expected to survive reboots and movement between compatible machines:

- operating-system files
- installed applications/packages
- users and authentication data
- configuration
- user home data
- package database
- update state

Temporary/runtime data should be RAM-backed where practical.

## Boot-to-runtime transition

UEFI can read the loader and kernel initially. That access disappears when Vibrix calls ExitBootServices.

Therefore Vibrix must later:

1. initialize its own xHCI controller,
2. enumerate USB devices,
3. identify the removable device that contains Vibrix,
4. initialize USB mass-storage transport,
5. reacquire block access,
6. mount the persistent Vibrix root filesystem.

This is a defining architecture requirement, not an optional portability feature.

## Internal storage

Internal NVMe/SATA devices are out of scope as installation targets.

Future drivers may expose them as optional user data devices, but Vibrix must never assume an internal disk is writable, available, or safe to modify.

## Flash lifetime

The USB storage policy should reduce unnecessary writes through techniques such as:

- RAM-backed /tmp
- bounded/log-buffered logging
- cache policies
- batched metadata updates where safe
- explicit sync semantics
- avoiding write-heavy background services

External USB SSDs are expected to provide better durability/performance than cheap flash drives, but ordinary USB flash media should remain a supported goal.
