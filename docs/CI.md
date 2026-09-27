# Continuous Integration

Vibrix CI builds the Rust UEFI loader and bare-metal kernel, checks formatting and lints, and boots the resulting EFI tree in headless QEMU.

CI checks that every tracked `tools/*.sh` script retains Git executable mode `100755` and passes `bash -n` before installing QEMU. Invoking scripts with `bash` alone does not verify their executable bits, so this protects the M0 script-permissions milestone against regressions.

The CI job also compiles `boot/src/elf.rs` as a standalone host test harness and runs its parser regression tests. This uses only the official Rust toolchain and exercises malformed ELF metadata without requiring UEFI firmware. Host parser tests do not demonstrate kernel handoff.

The parser also rejects a kernel entry point that does not belong to a file-backed, executable `PT_LOAD` range. Host regression fixtures cover non-executable code, BSS-only entry points and out-of-range entry points. This prevents an invalid future kernel jump but does not yet implement the firmware-to-kernel handoff.

The QEMU smoke test captures Vibrix's QEMU-only debug port and requires the bootloader to prove that it:

1. entered the Vibrix loader,
2. opened `/vibrix/kernel.elf`,
3. validated ELF64 little-endian x86-64 metadata,
4. parsed at least one valid `PT_LOAD` segment,
5. accepted the kernel image,
6. discovered an ACPI RSDP with valid firmware-provided checksum(s),
7. located a linear UEFI GOP framebuffer with sane mode and size metadata,
8. allocated physical backing pages for the kernel, zeroed the image span, copied every validated PT_LOAD file range and verified BSS bytes remain zero,
9. constructed and software-verified **inactive** higher-half kernel page tables; it has not activated them with CR3.
10. successfully captured the final UEFI memory map and retained its byte length, descriptor stride/version and map key. This does not call ExitBootServices.

Run the same smoke test locally:

```bash
sudo apt install -y qemu-system-x86 ovmf
bash tools/test-qemu.sh
```

The debug port is compiled only for QEMU builds. Bare-metal Vibrix builds do not write to the QEMU debug I/O port.

## Evidence levels

- **Host tests and builds:** a passing parser/decoder test or successful loader/kernel build checks that specific code path or artifact; neither proves that the kernel ran in QEMU.
- **CI QEMU/OVMF:** GitHub Actions runs `tools/test-qemu.sh` in headless QEMU and requires the debug-port markers above. At the current verified checkpoint, this proves the UEFI loader found and validated `kernel.elf`, discovered ACPI/GOP, staged the physical segments, and emitted `VIBRIX: kernel page tables verified` after a software walk of an **inactive** hierarchy, then `VIBRIX: final memory map captured` after the last GetMemoryMap call. The test does not activate CR3 or require an `ExitBootServices` or kernel-entry marker.
- **Human-reproduced QEMU/OVMF:** [Mixutin's workstation report](https://github.com/mixutin/Vibrix/issues/14#issuecomment-5856279590) records a manual run of `./tools/test-qemu.sh` on `main` at `4a1eac2`, including `VIBRIX: kernel segments staged` and the smoke-test success line. Mixutin also ran `./tools/run-qemu.sh` and visually observed the loader sequence through the staging marker via QEMU VNC. This independently reproduces that **loader-stage QEMU milestone**, not a later handoff.
- **Bare-metal Target 001:** requires a separate observed boot on the named physical machine. Neither GitHub Actions nor a workstation QEMU run is evidence of Target 001 operation; no such result is claimed here.

Neither the owner reproduction nor the CI smoke test establishes higher-half kernel execution, a successful `ExitBootServices`, native USB reacquisition, persistent root, or a CLI. The owner's earlier run at `4a1eac2` also does **not** reproduce the later page-table marker. For any later milestone, cite the exact branch/commit, test or machine, observed output, and the specific behavior demonstrated rather than upgrading a loader marker into kernel or bare-metal proof.
