# Crash and fault context

Vibrix's x86-64 fatal exception handlers retain the architectural frame pushed
by the CPU and print the fields needed to identify where and under which
privilege/stack state a fault occurred.

For double fault and general-protection faults the serial diagnostic includes
RIP, CS, RFLAGS, RSP, SS and the architectural error code. Page faults add CR2
and the decoded present/write/user/reserved/instruction-fetch bits.

The dedicated evidence workflow deliberately triggers the existing unmapped
page read after ExitBootServices. It requires the real serial record to contain
the complete CPU frame and the known CR2 address, while debugcon independently
records that fault context was captured.

This is a bounded crash-context facility. It does not yet unwind or symbolize
kernel stacks, dump general-purpose registers, persist a crash record across
reboot, collect SMP/per-CPU state, or provide a debugger.
