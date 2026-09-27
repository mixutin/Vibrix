# Host-Side Testing

Vibrix uses host-side tests to validate data structure layouts, bit manipulation, and parsing logic without requiring a booted kernel or UEFI firmware.

## Why host-side tests

The kernel is `no_std` and targets a custom bare-metal environment. Running tests inside the kernel requires:

- a booted kernel (QEMU or physical hardware),
- a test harness in the kernel,
- a way to report results (serial console, debug port).

Host-side tests avoid all of this. They compile the same Rust source files as standalone host executables and run them directly on the development machine.

## What belongs in host-side tests

Good candidates:

- **Data structure layout** — GDT entries, IDT descriptors, page table entries, BootInfo
- **Bit manipulation** — access bytes, flags bytes, error codes, feature bits
- **Parsing logic** — ELF headers, GPT entries, ACPI tables, memory maps
- **Arithmetic** — address alignment, span calculations, overflow detection
- **State machines** — allocator bitmaps, queue management

Not appropriate:

- **Actual hardware interaction** — MMIO, port I/O, DMA
- **Firmware calls** — UEFI boot services, ACPI control methods
- **Kernel runtime behavior** — scheduling, context switching, interrupt delivery

## How to write a host-side test

1. Create a file in `host-tests/` (e.g., `host-tests/gdt_layout.rs`).
2. Include the source file under test with `include!` or `#[path]`:
   ```rust
   #[path = "../kernel/src/arch/x86_64/gdt.rs"]
   mod gdt;
   ```
3. Write `#[test]` functions that validate the logic.
4. Add a `main()` function that runs all tests and reports results.

## Running host-side tests

Compile and run directly with `rustc`:

```bash
rustc --edition 2024 --test host-tests/gdt_layout.rs -o /tmp/gdt_test
/tmp/gdt_test
```

Or run all host tests:

```bash
for test in host-tests/*.rs; do
  name=$(basename "$test" .rs)
  rustc --edition 2024 --test "$test" -o "/tmp/$name" && "/tmp/$name"
done
```

## CI integration

The CI workflow runs host-side tests that exist on `main`. When adding a new host test:

1. Ensure the test file compiles and passes locally.
2. Add a CI step to compile and run the test.
3. Do not claim CI passage until the CI run completes.

## Guidelines

- **Test the logic, not the hardware.** If it needs a device, it's not a host test.
- **Keep tests deterministic.** No time-based or environment-dependent assertions.
- **Use descriptive names.** `rejects_overlapping_segments` > `test_1`
- **Test edge cases.** Zero, maximum, overflow, empty, malformed.
- **No unsafe in tests.** If the code under test requires `unsafe`, test the safe wrapper.
- **Match the kernel's constraints.** No `std` features that the kernel can't use.

## Current host tests

| File | What it tests |
|------|---------------|
| `host-tests/gdt_layout.rs` | GDT entry bit layout, access bytes, flags, segment selectors |
| `host-tests/idt_layout.rs` | IDT gate descriptors, exception vectors, IST bits |
| `host-tests/frame_allocator.rs` | Physical frame allocator bitmap logic |
| `host-tests/page_fault.rs` | Page-fault error code bit layout and decoding |
