# ADR-0006: Custom kernel target specification

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** Project bootstrap

## Context

The kernel runs in a bare-metal environment with no operating system services. It needs a custom target specification that differs from standard Rust targets like `x86_64-unknown-none`.

## Decision

Vibrix uses a custom JSON target specification at `kernel/x86_64-vibrix.json`. The kernel is built with `-Z build-std=core,compiler_builtins` and linked with a custom linker script (`kernel/linker.ld`).

## Consequences

- **Full control:** The target spec defines the exact CPU features, ABI, and linking behavior. No hidden assumptions from a generic target.
- **Higher-half kernel:** The linker script places the kernel at `0xFFFFFFFF80000000`, allowing user space to occupy the lower half of the address space.
- **Nightly requirement:** `-Z build-std` and custom JSON targets require a nightly Rust toolchain. This is pinned in `rust-toolchain.toml`.
- **No precompiled core:** The kernel must build `core` from source via `build-std`. This increases build time but ensures the kernel's `core` matches its target exactly.

## Alternatives considered

- **`x86_64-unknown-none`:** A generic bare-metal target. Rejected because it does not support the higher-half kernel layout and lacks the custom linker script integration.
- **`x86_64-unknown-uefi`:** The UEFI target used by the bootloader. Rejected because the kernel runs after `ExitBootServices` and must not depend on UEFI.
- **Standard OS target (e.g., `x86_64-unknown-linux-gnu`):** Rejected because the kernel is not a Linux application and must not link against libc or a Linux syscall ABI.

## References

- Custom target specifications: https://doc.rust-lang.org/rustc/targets/custom.html
- `-Z build-std`: https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#build-std
- Kernel linker script: `kernel/linker.ld`
- Kernel target spec: `kernel/x86_64-vibrix.json`
