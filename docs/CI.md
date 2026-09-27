# Continuous Integration

Vibrix CI builds the Rust UEFI loader and bare-metal kernel, checks formatting and lints, and boots the resulting EFI tree in headless QEMU.

CI checks that every tracked `tools/*.sh` script retains Git executable mode `100755` and passes `bash -n` before installing QEMU. Invoking scripts with `bash` alone does not verify their executable bits, so this protects the M0 script-permissions milestone against regressions.

The CI job also compiles `boot/src/elf.rs` as a standalone host test harness and runs its parser regression tests. This uses only the official Rust toolchain and exercises malformed ELF metadata without requiring UEFI firmware. Host parser tests do not demonstrate kernel handoff.

The parser also rejects a kernel entry point that does not belong to a file-backed, executable `PT_LOAD` range. Host regression fixtures cover non-executable code, BSS-only entry points and out-of-range entry points. This rejects invalid jump targets on the now-implemented firmware-to-kernel handoff path; the separate QEMU gate proves the kernel actually ran.

The QEMU smoke test captures both the loader/kernel QEMU debug port **and a separate native kernel COM1 serial log**. It requires the loader and then standalone kernel to prove that they:

1. entered the Vibrix loader,
2. opened `/vibrix/kernel.elf`,
3. validated ELF64 little-endian x86-64 metadata,
4. parsed at least one valid `PT_LOAD` segment,
5. accepted the kernel image,
6. discovered an ACPI RSDP with valid firmware-provided checksum(s),
7. located a linear UEFI GOP framebuffer with sane mode and size metadata,
8. allocated physical backing pages for the kernel, zeroed the image span, copied every validated PT_LOAD file range and verified BSS bytes remain zero,
9. constructed and software-verified higher-half kernel page tables before ExitBootServices; they are subsequently **activated via CR3**.
10. explicitly mapped and verified the loader image, 16-page kernel stack, BootInfo allocation, full memory-map buffer, RSDP and uncached GOP framebuffer under initially **inactive** page tables, and refreshed the preallocated map buffer after the last paging allocation. Captured the final UEFI memory-map tuple and successfully called ExitBootServices with its fresh key.
11. allocated a loader-owned BootInfo page before capture, populated validated v2 from the final tuple, and passed it to the **standalone kernel** after changing CR3 and moving to the dedicated stack. The kernel validated BootInfo, discovered CPUID, initialized GDT/TSS and native COM1 and wrote bounded pixels to the uncached framebuffer.

Run the same smoke test locally:

```bash
sudo apt install -y qemu-system-x86 ovmf
bash tools/test-qemu.sh
```

The debug port is compiled only for QEMU builds. Bare-metal Vibrix builds do not write to the QEMU debug I/O port.

## Evidence levels

- **Host tests and builds:** check parser, page-table, memory-map, CPUID,
  transition and framebuffer functions, but do not prove physical hardware.
- **QEMU/OVMF firmware-to-kernel handoff:** [CI run 36337520346](https://github.com/mixutin/Vibrix/actions/runs/36337520346)
  observed `VIBRIX: ExitBootServices succeeded` from the loader, then the
  **kernel's** `VIBRIX: kernel entry after ExitBootServices`,
  `VIBRIX: kernel BootInfo v2 validated`, `VIBRIX: kernel GDT/TSS loaded`,
  `VIBRIX: kernel serial initialized`, and `VIBRIX: kernel framebuffer
  wrote pixels`. A distinct QEMU serial capture contains
  `Vibrix kernel started.`; loader debug output cannot satisfy that check.
- **Post-firmware early frame allocator:** [CI run 36339966455](https://github.com/mixutin/Vibrix/actions/runs/36339966455)
  required kernel-only `VIBRIX: kernel frame allocator initialized` and
  `VIBRIX: kernel conventional frames allocated` markers during both normal
  and panic-probe boots. They prove issuance of two distinct conventional
  physical frame numbers; not mapping, zeroing, reuse, SMP or virtual memory.
- **Native IDT and page-fault diagnostics:** [CI run 36340579141](https://github.com/mixutin/Vibrix/actions/runs/36340579141)
  exercised normal/panic QEMU plus separate real `int3` and canonical
  unmapped-memory #PF probes after firmware exit. A returning #BP logs
  RIP; the non-returning #PF logs CR2 = `0x10000000000`, RIP, error code
  and decoded P/W/U/RSVD/I flags on COM1. The IDT does not enable IF,
  APIC IRQ routing, IST/RSP0 privilege stacks or SMP execution.
- **Kernel panic:** [CI run 36337648665](https://github.com/mixutin/Vibrix/actions/runs/36337648665)
  additionally booted a separately built `panic-probe` kernel, which
  reached post-firmware execution and printed the real panic handler's
  `VIBRIX: kernel panic` in debugcon and `kernel panic:` on COM1.
  The default kernel does not deliberately panic.

- **Historical workstation QEMU/OVMF report:** the owner's now-unavailable comment on deleted issue #14 records a manual run of `./tools/test-qemu.sh` on `main` at `4a1eac2`, including `VIBRIX: kernel segments staged` and the smoke-test success line. Mixutin also ran `./tools/run-qemu.sh` and visually observed the loader sequence through the staging marker via QEMU VNC. This independently reproduces that **loader-stage QEMU milestone**, not a later handoff.
- **Bare-metal Target 001:** requires a separate observed boot on the named physical machine. Neither GitHub Actions nor a workstation QEMU run is evidence of Target 001 operation; no such result is claimed here.

Neither the owner reproduction of the earlier loader-only code nor these
QEMU CI results establish a native USB/xHCI driver, persistent USB root,
hardware IRQ routing, processes, userspace shell, or Target 001 bare-metal boot.
The older workstation report is historical and its deleted Issue #14
source is no longer retrievable. The active coordination board is [#46](https://github.com/mixutin/Vibrix/issues/46). Do not generalize QEMU success to physical
hardware or unrelated milestones.
