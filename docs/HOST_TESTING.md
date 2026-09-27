# Host-side tests

Vibrix tests code that does not need firmware or a booted kernel on the development host. This provides fast feedback for parsers and CPU-feature decoding, but it does **not** demonstrate that kernel code executes after `ExitBootServices`.

## Tests available on `main`

From the repository root, with the Rust toolchain selected by `rust-toolchain.toml`:

```bash
rustc --edition=2024 --test boot/src/elf.rs -o /tmp/vibrix-elf-tests
/tmp/vibrix-elf-tests

rustc --edition=2024 --test kernel/src/arch/x86_64/cpuid.rs -o /tmp/vibrix-cpuid-tests
/tmp/vibrix-cpuid-tests

rustc --edition=2024 --test tools/inspect-gpt.rs -o /tmp/vibrix-gpt-tests
/tmp/vibrix-gpt-tests
```

These commands match the standalone Rust test-harness approach in `.github/workflows/ci.yml`; the CI runner uses its temporary directory instead of `/tmp`. The ELF tests exercise loader metadata validation using fixtures. The CPUID tests exercise the kernel's decoder with synthetic register values (and a host CPU smoke test); they do not show CPUID discovery running inside Vibrix. The GPT tests exercise a **read-only host image inspector**, not USB provisioning or a Vibrix filesystem driver. See [GPT inspection](GPT_INSPECTION.md) for its limits.

For boot-path evidence, use `bash tools/test-qemu.sh` and consult [CI](CI.md) for the exact markers currently required. A host test passing is not a QEMU boot, and a QEMU loader marker is not evidence of kernel entry or operation on Target 001.

## Adding a host test

- Prefer `#[cfg(test)] mod tests` in the **first-party source module** when it can be compiled as a standalone host test, as in `boot/src/elf.rs`. Test the real parser/decoder instead of duplicating its constants and logic in a separate test-only implementation.
- If host compilation of a kernel module needs isolation from hardware-specific code, extract a small testable first-party unit and explicitly document what remains untested. A passing descriptor-layout test alone does not prove `lgdt`, interrupt delivery, or frame allocation works in the kernel.
- Include malformed inputs, boundary values and checked arithmetic where relevant. Do not run destructive tests against real disks; the GPT inspector accepts regular image files read-only.
- Run the new test locally and add its command to `.github/workflows/ci.yml` in the same PR **if the new test becomes part of the supported CI suite**. If another agent is active, coordinate shared CI edits through the current [board #46](https://github.com/mixutin/Vibrix/issues/46); deleted issue #14 is not an approval gate. In a single-agent workflow, inspect competing PRs and validate the exact merged head.
- Report exact commands and observed results separately from planned QEMU or hardware tests. Follow [the independence policy](INDEPENDENCE.md) and [dependency policy](DEPENDENCIES.md): reviewed community Rust crates are allowed, but copying another operating system's implementation is not.

Host tests complement, rather than replace, Clippy/build checks and QEMU observation. The roadmap only advances when the relevant behavior is demonstrated on its stated target.
