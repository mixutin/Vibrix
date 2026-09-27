# ADR-0005: No third-party runtime crates

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** Project bootstrap

## Context

Vibrix is an experiment in building an OS from first principles. Third-party crates that implement core OS functionality (ELF parsing, filesystems, networking, allocation, synchronization) would undermine the independence experiment and create a false sense of progress.

## Decision

Vibrix does not add shipped third-party runtime crates as a shortcut. All core OS functionality is implemented in Vibrix's own code. Host-only development tooling is separate and should remain minimal.

## Consequences

- **Independence:** Vibrix's codebase is genuinely self-implemented. Progress is real, not assembled from library calls.
- **Learning:** Every subsystem is understood because it was built, not imported.
- **Effort:** More work is required for each subsystem. This is intentional — the experiment is about what can be built, not what can be downloaded.
- **Dependency policy:** Before adding any dependency, determine whether it becomes part of Vibrix and whether it undermines the independence experiment. If yes, implement it in Vibrix instead.

## Alternatives considered

- **Use `elf` crate for ELF parsing:** Rejected — ELF parsing is a core boot function and a good learning exercise. Implemented in `boot/src/elf.rs`.
- **Use `bitflags` or similar for register definitions:** Rejected — register bit manipulation is straightforward in Rust and should be explicit.
- **Use a filesystem crate:** Rejected — the filesystem is a core Vibrix subsystem (M8).
- **Use a networking stack crate:** Rejected — the networking stack is a core Vibrix subsystem (M11).

## Exceptions

- **Host-only tooling:** Development tools that run on the host (not shipped as part of Vibrix) may use third-party crates if they significantly improve productivity. These must be clearly separated from Vibrix's runtime code.
- **Compiler-builtins:** The kernel uses `compiler_builtins` via `-Z build-std-features=compiler-builtins-mem` for memory operations. This is a compiler-provided primitive, not a third-party crate.

## References

- `docs/INDEPENDENCE.md`
- `AGENTS.md` — Dependencies section
