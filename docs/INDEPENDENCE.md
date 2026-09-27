# Independence Policy

Vibrix exists to test how far an AI-assisted project can go while engineering its own Rust-native operating-system architecture. Using appropriately licensed community Rust libraries is compatible with this goal.

## Allowed

- the Rust language and official Rust toolchain
- Rust-provided `core` and compiler/runtime support that ships with the official toolchain
- appropriately licensed community Rust crates (crates.io, Git or vendored), subject to dependency review and target compatibility
- CPU and hardware manuals
- UEFI and other interface specifications
- standards and research papers
- compilers, assemblers and linkers used on the development host
- QEMU and other test hardware/emulators
- firmware such as OVMF used only as a development/test platform
- debuggers and analysis tools
- Git and CI infrastructure
- documentation describing algorithms or hardware behavior

## Not allowed in Vibrix

- Linux or BSD kernel code
- copied third-party drivers
- GNU or BSD userspace code
- third-party libc code
- BusyBox
- third-party bootloaders
- copied filesystem implementations
- third-party operating systems used as Vibrix's runtime or kernel

## Rust boundary

Vibrix is Rust-native, but it does not attempt to reimplement the Rust language itself.

Official Rust toolchain components and appropriately licensed community Rust crates may be linked into Vibrix. Crates can provide ELF parsing, synchronization, bitfields, ACPI, PCI, UEFI helpers, allocation, networking, filesystems and other functionality as needed. The Vibrix-owned boot chain, kernel integration, subsystem contracts and persistent USB system model remain Vibrix's responsibility.

All dependency additions follow [DEPENDENCIES.md](DEPENDENCIES.md). A dependency is not exempt from `no_std`, UEFI/bare-metal ABI, memory-safety, licensing or testing requirements.

Host-side development programs such as QEMU, OVMF, Git, GDB and shell utilities are tools used to build or test Vibrix; they are not part of the Vibrix runtime.

When writing first-party code, use documented behavior rather than copying or lightly rewriting other operating systems' source. Calling a community crate through its public API under its license is permitted and is not the same as copying its implementation into first-party Vibrix files.

## AI provenance

AI-assisted code is still subject to this policy. Generated code must not knowingly reproduce third-party implementation code. Important architectural decisions should be documented so the project remains understandable rather than becoming an opaque pile of generated code.
