# Userspace stack-protector toolchain probe

The M23 roadmap asks for stack canaries for supported userspace toolchains.
Vibrix currently builds native Ring-3 programs with the repository-pinned
nightly Rust compiler for `x86_64-unknown-none`.

Before adding a runtime ABI or claiming the roadmap item, the dedicated workflow
compiles a small `no_std` userspace-target object with Rust's
`-Zstack-protector=all` and inspects the resulting machine object. It requires
an unresolved `__stack_chk_fail` reference and preserves the undefined-symbol
and disassembly evidence. This establishes what the exact pinned compiler emits
for the actual Vibrix userspace target.

This probe is intentionally **not** roadmap completion. A later change must
supply a fail-closed runtime, seed any guard material appropriately, build the
real base-system userspace binaries with the mitigation enabled, and prove both
normal execution and corruption detection before the checkbox can be marked.
