# Package metadata runtime proof

The M14 package foundation defines the fixed-size `VPKGv001` manifest and a
bounded installed-package database in the first-party `vibrix-package` crate.
The implementation remains allocation-free and `no_std`.

The target proof builds `vibrix-package-probe` as a real
`x86_64-unknown-none` ELF. The kernel loads that ELF through the existing
private userspace CR3 and guarded stack, enters CPL3, and exposes only the
existing process syscall path used by other userspace proofs.

Inside Ring 3 the probe verifies:

- canonical manifest encode/decode;
- rejection of non-zero reserved bytes;
- dependency installation failing transactionally when a dependency is absent;
- successful dependency-ordered installation;
- reverse-dependency protection during removal; and
- successful removal after dependents are removed.

The probe also requires PID 1 and exits with status 0 only when every package
check succeeds. CI requires independent kernel debugcon markers for ELF loading,
the userspace syscall crossing and successful package validation, plus the
serial exit record.

This completes only the bounded **package format/database/dependencies**
behavior once exact-head QEMU evidence is green and merged. It does not provide
a package manager, archives/payload installation, repository transport,
signatures, persistent package state, transactions across files, dependency
solving beyond bounded minimum versions, or M15 self-hosting.
