//! QEMU-only kernel-entry diagnostic independent of firmware and COM1.
pub fn write(message: &str) {
    #[cfg(feature = "qemu-debugcon")]
    for byte in message.bytes() {
        // SAFETY: debug I/O port 0xe9 is configured only in QEMU builds,
        // and the standalone kernel executes at CPL0 after firmware exit.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0xe9u16,
                in("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }
    }

    #[cfg(not(feature = "qemu-debugcon"))]
    let _ = message;
}
