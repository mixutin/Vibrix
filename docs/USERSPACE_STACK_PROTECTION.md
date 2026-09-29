# Userspace stack protection

This M11 slice validates the page-table protections around the existing guarded userspace stack. It does not add a new stack allocator; it proves the production guarded allocation policy with a real CPL3 CPU fault.

## Mapping contract

The current address-space path creates userspace stacks with GuardedLayout. The payload pages are user-accessible read/write and non-executable. The page immediately below and the page immediately above the payload are left unmapped. The ELF/shell/desktop paths use the same guarded primitive, with bounded payload sizes selected by profile.

For the dedicated proof, the one-page stack layout is:

- lower guard: 0x007f_e000..0x007f_f000 — absent;
- payload: 0x007f_f000..0x0080_0000 — user RW/NX;
- upper guard: 0x0080_0000..0x0080_1000 — absent;
- initial RSP: 0x0080_0000.

The feature-gated CPL3 probe executes a byte write to RSP - 4097, exactly 0x007f_efff, the last byte of the absent lower guard page. The write must never reach mapped stack storage.

## Required hardware evidence

The QEMU proof requires:

- the independent userspace CR3 to be prepared and activated;
- an explicit marker immediately before CPL3 entry;
- CR2 = 0x007f_efff;
- page-fault decoding of present=false, write=true, user=true, reserved=false, exec=false.

These bits distinguish a genuine user-mode write into a non-present guard page from a supervisor fault, read fault, reserved-bit failure, or instruction fetch.

Intel's architecture manual defines page-fault error-code bit 0 as present/protection, bit 1 as read/write, bit 2 as user/supervisor, and bit 4 as instruction/data. Intel also specifies execute-disable page protection for IA-32e paging.

Primary references checked 2026-09-29:

- Intel 64 and IA-32 Architectures Software Developer's Manual, system programming guide:
  https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html
- Intel SDM Volume 3A, page-level protection and execute-disable:
  https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-3a-part-1-manual.pdf

## Boundary

This milestone means the current userspace stack is bounded by absent guard pages and its payload is RW/NX, with a real CPL3 lower-guard write fault.

It does not claim compiler stack canaries, shadow stacks or Intel CET, userspace ASLR, guard pages around every kernel/interrupt stack, automatic stack growth, signal alternate stacks, SMP TLB-shootdown hardening, or physical Target 001 validation.
