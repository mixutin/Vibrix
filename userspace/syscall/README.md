# vibrix-syscall

vibrix-syscall is the dependency-free, no_std Rust userspace binding for
Vibrix syscall ABI v1.

It shares the repository canonical ABI definitions from shared/syscall_abi.rs
and emits the native x86-64 syscall instruction with the v1 register
convention:

- RAX: syscall number / result
- RDI, RSI, RDX, R10, R8, R9: arguments 0 through 5
- RCX and R11: architectural clobbers

The crate provides result decoding plus wrappers for scalar/slice-based calls.
exec_raw stays unsafe because argv/environment vector layout belongs to a later
M5 milestone.

This library being buildable does not mean the kernel implements each reserved
call. Kernel dispatch, process lifecycle, executable loading, argv/environment,
and wait/exit semantics remain separate roadmap work.
