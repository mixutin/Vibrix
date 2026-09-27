# Independence Policy

Vibrix exists to test how far an AI-assisted project can go while implementing an operating system from scratch.

## Allowed

- the Rust language and official Rust toolchain
- Rust-provided `core` and compiler/runtime support that ships with the official toolchain
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

- crates.io/community Rust packages
- vendored third-party Rust crates
- Linux or BSD kernel code
- copied third-party drivers
- GNU or BSD userspace code
- third-party libc code
- BusyBox
- third-party bootloaders
- copied filesystem implementations
- third-party runtime libraries

## Rust boundary

Vibrix is Rust-native, but it does not attempt to reimplement the Rust language itself.

Official components distributed as part of the Rust toolchain may be linked where the language requires them. Community crates are not allowed in shipped Vibrix code, even for common functionality such as ELF parsing, synchronization, bitfields, ACPI, PCI, UEFI helpers, allocation, networking or filesystems.

Those facilities are implemented by Vibrix.

Host-side development programs such as QEMU, OVMF, Git, GDB and shell utilities are tools used to build or test Vibrix; they are not part of the Vibrix runtime.

Clean-room implementation means understanding documented behavior and writing Vibrix's implementation ourselves rather than translating or lightly rewriting existing source code.

## AI provenance

AI-assisted code is still subject to this policy. Generated code must not knowingly reproduce third-party implementation code. Important architectural decisions should be documented so the project remains understandable rather than becoming an opaque pile of generated code.
