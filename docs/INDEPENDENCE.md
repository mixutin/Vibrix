# Independence Policy

Vibrix exists to test how far an AI-assisted project can go while implementing an operating system from scratch.

## Allowed

- CPU and hardware manuals
- UEFI and other interface specifications
- standards and research papers
- compilers, assemblers and linkers used on the development host
- QEMU and other test hardware/emulators
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
- vendored runtime libraries

Clean-room implementation means understanding documented behavior and writing Vibrix's implementation ourselves rather than translating or lightly rewriting existing source code.

## AI provenance

AI-assisted code is still subject to this policy. Generated code must not knowingly reproduce third-party implementation code. Important architectural decisions should be documented so the project remains understandable rather than becoming an opaque pile of generated code.
