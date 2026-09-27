# ADR-0001: Rust as the system implementation language

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** Project bootstrap

## Context

Vibrix is an independent OS built from scratch. The implementation language affects every layer: bootloader, kernel, drivers, userspace, tooling. The language must support low-level systems programming (no mandatory runtime, direct hardware access, fine-grained memory control) while providing safety guarantees that reduce the bug surface in kernel code.

## Decision

Rust is the implementation language for all Vibrix code: bootloader, kernel, drivers, system libraries, userspace, filesystem, networking, installer, and package tooling.

Tiny architecture-specific assembly is permitted only where hardware requires it (early CPU entry, context transitions). It must be isolated behind Rust interfaces and documented.

## Consequences

- **Safety:** Ownership, borrowing, and lifetimes prevent entire classes of memory bugs (use-after-free, double-free, data races) at compile time.
- **No runtime:** Rust's `no_std` mode allows bare-metal code without a garbage collector, runtime, or hidden allocations.
- **Modern tooling:** `cargo` provides builds, dependencies, and documentation. The type system catches errors early.
- **Learning curve:** Rust's borrow checker and `unsafe` discipline require care in kernel code. Every `unsafe` block needs a documented invariant.
- **Ecosystem tension:** The project must resist the temptation to pull in crates that implement subsystems Vibrix intends to build itself (ELF parsing, filesystems, networking, etc.).

## Alternatives considered

- **C:** The traditional systems language. Rejected because manual memory management in a from-scratch kernel is a well-known source of critical vulnerabilities. Vibrix aims to demonstrate that a memory-safe language can build an OS.
- **C++:** Offers more abstraction than C but retains manual memory management and undefined behavior risks. Rejected for the same safety reasons.
- **Zig:** A newer systems language with comptime and no hidden control flow. Promising, but the ecosystem and tooling are less mature than Rust's. Revisit for future subsystems if appropriate.
- **Ada/SPARK:** Strong safety guarantees and proven in high-assurance systems. Rejected due to smaller ecosystem, less community momentum, and steeper onboarding for new contributors.

## References

- Rust Language: https://www.rust-lang.org/
- Rustonomicon (unsafe Rust): https://doc.rust-lang.org/nomicon/
- `no_std` Rust: https://docs.rust-embedded.org/book/intro/no-std.html
