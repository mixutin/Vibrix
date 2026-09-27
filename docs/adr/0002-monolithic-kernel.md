# ADR-0002: Monolithic kernel architecture

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** Project bootstrap

## Context

The kernel architecture determines how subsystems (memory management, scheduling, drivers, filesystems, networking) interact. The main options are monolithic, microkernel, and hybrid.

## Decision

Vibrix uses a monolithic kernel. All core subsystems run in kernel space with full hardware access. Device drivers are kernel modules (statically linked or dynamically loaded) running in kernel mode.

## Consequences

- **Performance:** No inter-process communication (IPC) overhead for kernel-internal calls. Direct function calls between subsystems.
- **Simplicity:** No need to design IPC protocols, capability systems, or userspace driver frameworks for core functionality.
- **Risk:** A bug in any kernel-mode driver can crash the entire system. Mitigated by Rust's safety guarantees and careful `unsafe` discipline.
- **Bootstrapping:** Simpler to build incrementally — no need to set up a userspace driver infrastructure before basic functionality works.

## Alternatives considered

- **Microkernel:** Minimal kernel with drivers and services in userspace. Rejected because IPC overhead and complexity are premature for a bootstrap-stage OS. The performance cost is significant for a system that does not yet have userspace infrastructure.
- **Hybrid (e.g., Windows NT, XNU):** Monolithic core with some services in userspace. Rejected as a premature optimization — the boundary between kernel and userspace services is a design burden that Vibrix does not need at this stage.
- **Exokernel:** Minimal hardware abstraction with library OSes. Rejected because it pushes complexity to userspace and requires a mature ecosystem to be practical.

## References

- Tanenbaum, A. S., & Woodhull, A. S. *Operating Systems: Design and Implementation.* (microkernel vs. monolithic debate)
- Linux kernel architecture: https://www.kernel.org/doc/html/latest/
