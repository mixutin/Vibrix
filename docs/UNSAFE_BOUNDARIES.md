# Vibrix Unsafe Code Boundaries

This document outlines the explicit unsafe code boundaries in the Vibrix kernel, documenting the invariants and safety contracts for each unsafe operation.

## Principles

1. **Every unsafe operation has a documented invariant** — the conditions under which it is safe to call.
2. **Unsafe primitives are small and isolated** — wrapped with safe interfaces when possible.
3. **Unsafe is never used to silence the borrow checker** — only for hardware access, FFI, or performance-critical operations where safe alternatives don't exist.
4. **All unsafe code is reviewed** — with explicit justification for each `unsafe` block.

## Boundary Categories

### 1. Port I/O (UART, PIC, PIT)

**Location:** `kernel/src/arch/x86_64/serial.rs`, `kernel/src/arch/x86_64/timer.rs`

**Invariants:**
- Port addresses are compile-time constants validated against the hardware specification.
- Port I/O is only performed during initialization or in interrupt context.
- No concurrent access to the same port without synchronization.

**Safety Contract:**
```rust
/// # Safety
/// `port` must be a valid, writable I/O port for the current hardware.
unsafe fn outb(port: u16, value: u8);

/// # Safety
/// `port` must be a valid, readable I/O port for the current hardware.
unsafe fn inb(port: u16) -> u8;
```

### 2. Memory-Mapped I/O (APIC, HPET)

**Location:** `kernel/src/arch/x86_64/apic.rs`, `kernel/src/arch/x86_64/timer.rs`

**Invariants:**
- MMIO base addresses are obtained from ACPI tables (MADT, HPET) and validated before use.
- MMIO regions are mapped into the kernel address space with appropriate cacheability settings.
- No concurrent access to the same MMIO register without synchronization.

**Safety Contract:**
```rust
/// # Safety
/// `base` must be a valid, mapped MMIO region of at least `size` bytes.
unsafe fn read_mmio<T>(base: *const T) -> T;
unsafe fn write_mmio<T>(base: *mut T, value: T);
```

### 3. Inline Assembly (Context Switching, Special Instructions)

**Location:** `kernel/src/arch/x86_64/*.rs`

**Invariants:**
- Assembly blocks are only used where hardware interfaces make them unavoidable.
- Register constraints are explicitly specified.
- Memory clobbers are declared when assembly modifies memory.
- Assembly is isolated behind Rust interfaces.

**Safety Contract:**
```rust
/// # Safety
/// This function performs a context switch. The caller must ensure:
/// - Both stack pointers are valid and point to mapped memory.
/// - The target context is in a valid state.
/// - Interrupts are disabled during the switch.
unsafe fn switch_context(from: *mut Context, to: *const Context);
```

### 4. Raw Pointer Dereferencing (BootInfo, Memory Map)

**Location:** `kernel/src/main.rs`, `kernel/src/arch/x86_64/mod.rs`

**Invariants:**
- Pointers are validated before dereferencing (null check, alignment check, bounds check).
- Boot-time data structures are treated as untrusted input.
- Lifetime of pointed-to data is guaranteed to exceed the dereference.

**Safety Contract:**
```rust
/// # Safety
/// `ptr` must be a valid, aligned pointer to a `T` that is valid for reads.
unsafe fn read_boot_info(ptr: *const BootInfo) -> &'static BootInfo;
```

### 5. Page Table Manipulation

**Location:** `kernel/src/arch/x86_64/vmem.rs`

**Invariants:**
- Page table entries are validated before use (present bit, reserved bits).
- Physical addresses are obtained from the frame allocator and validated.
- Page table modifications are followed by TLB flushes.

**Safety Contract:**
```rust
/// # Safety
/// `phys_addr` must be a valid, page-aligned physical address obtained from
/// the frame allocator. The caller must ensure the address is not already
/// mapped or is being intentionally remapped.
unsafe fn map_page(pml4: *mut PageTableEntry, vaddr: VirtAddr, phys_addr: PhysAddr, flags: PageFlags);
```

### 6. Interrupt Descriptor Table

**Location:** `kernel/src/arch/x86_64/idt.rs`

**Invariants:**
- IDT entries are validated before installation (present bit, DPL, gate type).
- Interrupt handlers are valid function pointers.
- IDT is loaded only once during initialization.

**Safety Contract:**
```rust
/// # Safety
/// `handler` must be a valid function pointer to an interrupt handler with
/// the correct signature. The handler must be `extern "C"` and follow the
/// interrupt calling convention.
unsafe fn set_idt_entry(idt: *mut IdtEntry, index: u8, handler: HandlerFunc, flags: IdtFlags);
```

### 7. Global Descriptor Table

**Location:** `kernel/src/arch/x86_64/gdt.rs`

**Invariants:**
- GDT entries are validated before installation (present bit, DPL, type).
- Segment selectors are valid indices into the GDT.
- GDT is loaded only once during initialization.

**Safety Contract:**
```rust
/// # Safety
/// `selector` must be a valid segment selector pointing to a GDT entry
/// with the correct type and privilege level.
unsafe fn load_gdt(gdt: *const GdtEntry, code_selector: u16, data_selector: u16);
```

### 8. Physical Frame Allocator

**Location:** `kernel/src/arch/x86_64/frame_alloc.rs`

**Invariants:**
- Frame addresses are obtained from the UEFI memory map and validated.
- Frame allocator state is protected by a lock (once SMP is enabled).
- Frames are marked as allocated before being returned.

**Safety Contract:**
```rust
/// # Safety
/// `frame` must be a valid, page-aligned physical address obtained from
/// the frame allocator. The caller must ensure the frame is not already
/// allocated or is being intentionally double-allocated.
unsafe fn PhysAddr::as_ptr(self) -> *mut u8;
```

### 9. Kernel Heap Allocator

**Location:** `kernel/src/heap.rs`

**Invariants:**
- Heap allocator state is protected by a lock.
- Allocations are aligned to the requested alignment.
- Freed memory is not accessed after being freed.

**Safety Contract:**
```rust
/// # Safety
/// `layout` must be a valid `Layout` (non-zero size, power-of-two alignment).
/// The returned pointer must be deallocated with the same layout.
unsafe fn alloc(layout: Layout) -> *mut u8;
unsafe fn dealloc(ptr: *mut u8, layout: Layout);
```

### 10. Context Switching

**Location:** `kernel/src/arch/x86_64/context.rs`

**Invariants:**
- Context structures are valid and point to valid stack memory.
- Interrupts are disabled during context switches.
- Context switches only occur in process context, not interrupt context.

**Safety Contract:**
```rust
/// # Safety
/// `from` and `to` must be valid pointers to `Context` structures.
/// The caller must ensure:
/// - Both contexts are in a valid state.
/// - Interrupts are disabled.
/// - The context switch is not reentrant.
unsafe fn context_switch(from: *mut Context, to: *const Context) -> !;
```

## Review Checklist

When adding new unsafe code, ensure:

- [ ] The unsafe operation is necessary (no safe alternative exists).
- [ ] The invariants are documented in a `/// # Safety` comment.
- [ ] The unsafe block is as small as possible.
- [ ] The unsafe operation is wrapped in a safe interface when possible.
- [ ] The unsafe code is reviewed by another agent.
- [ ] The unsafe code is tested (when possible).

## References

- Rustonomicon: https://doc.rust-lang.org/nomicon/
- Rust Unsafe Code Guidelines: https://rust-lang.github.io/unsafe-code-guidelines/
- Intel SDM Vol. 3A (for hardware-specific unsafe operations)
- AMD APM Vol. 2 (for hardware-specific unsafe operations)
