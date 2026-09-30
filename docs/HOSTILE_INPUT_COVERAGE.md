# Privileged parser hostile-input coverage

This document is the current M23 inventory for parser/state-machine inputs that
cross a privilege boundary or consume untrusted bytes. Vibrix does not yet ship
privileged userspace daemons, so the present inventory is kernel/boot/package
parsers plus the native shell command parser.

The dedicated `Privileged parser hostile-input evidence` workflow executes the
production-linked test suites for each current class:

| Boundary | Production parser / state machine | Hostile-input evidence |
| --- | --- | --- |
| Boot image | ELF64 loader | malformed class, machine, ranges, segments and W^X rejection |
| Handoff | BootInfo | version, size, flags, pointer/range and identity validation |
| Firmware tables | ACPI | checksums, lengths, signatures and bounded table walking |
| Device discovery | PCI | malformed BAR/capability and bounded enumeration tests |
| USB storage | USB BOT + SCSI | invalid signatures, residue/status, lengths and command framing |
| Filesystem wire | VibrixFS | corrupt metadata, names, extents, modes and record framing |
| Network | IPv4, UDP, DHCP, DNS, TCP | malformed lengths/checksums/options/state transitions and bounded resource behavior |
| Packages | package metadata/database | reserved fields, dependency failures and capacity rules |
| Shell | shell parser | malformed quoting, redirection, arguments, paths and bounded input |

The umbrella workflow intentionally reuses the same production modules exercised
by canonical CI rather than creating mock parsers. `cargo test -p vibrix-kernel
--lib` also keeps newly added kernel parser tests in the hostile-input gate by
default.

This evidence is scoped to parsers/privileged state machines present in the
tree at the time of the run. Adding a new privileged daemon or parser requires
updating this inventory and adding negative/adversarial tests before the M23
coverage claim remains valid. This is deterministic adversarial regression
testing, not coverage-guided fuzzing or a claim that memory-safe Rust eliminates
logic vulnerabilities.
