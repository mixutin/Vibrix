//! Bounded x86-64 development reboot through the legacy i8042 controller.
//!
//! QEMU q35 already exposes the same i8042 path used by the early keyboard
//! console. This module deliberately does not claim Target 001 reset support;
//! a later platform layer may prefer an ACPI FADT reset register when present.

#[cfg(target_os = "none")]
use core::arch::asm;

const I8042_STATUS_COMMAND: u16 = 0x64;
const INPUT_BUFFER_FULL: u8 = 1 << 1;
const RESET_PULSE_COMMAND: u8 = 0xfe;
const READY_POLL_BUDGET: u32 = 1_000_000;

fn input_buffer_empty(status: u8) -> bool {
    status & INPUT_BUFFER_FULL == 0
}

#[cfg(target_os = "none")]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: caller guarantees CPL0 access to the legacy i8042 status port.
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(target_os = "none")]
unsafe fn outb(port: u16, value: u8) {
    // SAFETY: caller guarantees CPL0 access and a valid i8042 command byte.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Request a CPU reset through the i8042 output-port pulse command.
///
/// # Safety
/// The caller must run at CPL0 on a machine where port 0x64 is the legacy
/// i8042 status/command register and must accept that this function never
/// returns. The current proof target is QEMU q35 only. If the controller never
/// becomes writable, or ignores the reset command, the CPU intentionally
/// remains in this non-returning path rather than pretending reboot succeeded.
#[cfg(target_os = "none")]
pub unsafe fn reboot_i8042() -> ! {
    let mut budget = READY_POLL_BUDGET;
    while budget != 0 {
        // SAFETY: documented CPL0 i8042 ownership contract.
        let status = unsafe { inb(I8042_STATUS_COMMAND) };
        if input_buffer_empty(status) {
            // Stop timer delivery between committing to reset and the command.
            // SAFETY: CPL0; reboot is a terminal operation.
            unsafe {
                asm!("cli", options(nomem, nostack, preserves_flags));
                outb(I8042_STATUS_COMMAND, RESET_PULSE_COMMAND);
            }
            loop {
                // If reset is unsupported, remain halted instead of resuming
                // normal kernel execution after a failed reboot request.
                unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
            }
        }
        budget -= 1;
        core::hint::spin_loop();
    }

    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_buffer_bit_controls_command_writability() {
        assert!(input_buffer_empty(0));
        assert!(input_buffer_empty(1));
        assert!(!input_buffer_empty(INPUT_BUFFER_FULL));
        assert!(!input_buffer_empty(INPUT_BUFFER_FULL | 0x80));
    }
}
