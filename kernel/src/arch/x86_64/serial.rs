//! x86-64 COM1 (16550) serial debug console.
//!
//! Clean-room implementation. Provides the kernel's first output mechanism.
//! The UART programming sequence is computed by a pure function
//! ([`init_program`]) so it can be unit-tested off-target; [`SerialPort::init`]
//! applies it to hardware.
//!
//! # Port-I/O safety
//!
//! Every raw port access ([`outb`], [`inb`]) is `unsafe` and carries the
//! invariant that `port` is a valid, accessible I/O port for the current
//! privilege level. The kernel runs in ring 0 with full I/O port access, so
//! this holds for COM1 (`0x3f8`). These are the only `unsafe` port-I/O
//! primitives in the console; all higher-level code goes through them.

use core::arch::asm;
use core::cell::UnsafeCell;
use core::fmt;

/// COM1 port base address.
pub const COM1_BASE: u16 = 0x3f8;

/// A single port-I/O write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PortWrite {
    pub port: u16,
    pub value: u8,
}

/// Number of writes in the 16550 initialization sequence.
pub const INIT_LEN: usize = 8;

/// Maximum number of times to poll the transmitter-ready bit (LSR bit 5)
/// before giving up. Prevents an unresponsive or unclocked UART from hanging
/// kernel entry — `println!` is unconditional, so debug output must never
/// block boot progress.
const TX_READY_BUDGET: u32 = 1_000_000;

/// Compute the 16550 initialization sequence for a UART at `base`.
///
/// Programs 115200 baud (divisor 1), 8N1, FIFO enabled with a 14-byte
/// threshold, and DTR/RTS/OUT2 asserted.
///
/// The sequence clears DLAB *before* the first IER write. Register 1 is IER
/// when DLAB=0 and DLM when DLAB=1, so an inherited DLAB=1 would otherwise
/// send the "disable interrupts" write to DLM and leave the inherited
/// interrupt enables intact. See TI PC16550D Table 2 / section 8.6.2.
pub fn init_program(base: u16) -> [PortWrite; INIT_LEN] {
    [
        PortWrite {
            port: base + 3,
            value: 0x03,
        }, // LCR: clear DLAB, 8N1 (so register 1 is IER)
        PortWrite {
            port: base + 1,
            value: 0x00,
        }, // IER: disable interrupts
        PortWrite {
            port: base + 3,
            value: 0x80,
        }, // LCR: enable DLAB (so register 1 is DLM)
        PortWrite {
            port: base,
            value: 0x01,
        }, // DLL: divisor low (115200)
        PortWrite {
            port: base + 1,
            value: 0x00,
        }, // DLM: divisor high
        PortWrite {
            port: base + 3,
            value: 0x03,
        }, // LCR: restore 8N1, clear DLAB
        PortWrite {
            port: base + 2,
            value: 0xC7,
        }, // FCR: enable + clear FIFO
        PortWrite {
            port: base + 4,
            value: 0x0B,
        }, // MCR: DTR + RTS + OUT2
    ]
}

/// Write a byte to an I/O port.
///
/// # Safety
/// `port` must be a valid, accessible I/O port for the current privilege
/// level. The kernel uses this only for COM1 registers in ring 0.
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
    }
}

/// Read a byte from an I/O port.
///
/// # Safety
/// `port` must be a valid, accessible I/O port for the current privilege
/// level. The kernel uses this only for COM1 registers in ring 0.
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack));
    }
    value
}

/// Wait until `is_ready` returns `true`, bounded by [`TX_READY_BUDGET`] polls.
///
/// Returns `true` if ready, `false` if the budget expired (transmitter never
/// ready). This prevents an unresponsive UART from hanging kernel entry.
fn wait_for_transmit_ready(mut is_ready: impl FnMut() -> bool) -> bool {
    let mut budget = TX_READY_BUDGET;
    while !is_ready() {
        budget = budget.saturating_sub(1);
        if budget == 0 {
            return false;
        }
        core::hint::spin_loop();
    }
    true
}

/// A 16550 UART.
///
/// All methods take `&self`: programming the UART is a sequence of volatile
/// port writes and needs no mutable state.
pub struct SerialPort {
    port: u16,
}

impl SerialPort {
    /// Create a UART handle for the given port base.
    pub const fn new(port: u16) -> Self {
        SerialPort { port }
    }

    /// Initialize the UART.
    pub fn init(&self) {
        for w in init_program(self.port) {
            // SAFETY: w.port is a COM1 register, valid in ring 0.
            unsafe { outb(w.port, w.value) };
        }
    }

    /// Write a single byte, waiting (bounded) for the transmitter to be ready.
    ///
    /// Returns `Err` if the transmitter never became ready, so an
    /// unresponsive UART drops the byte instead of hanging the boot.
    pub fn write_byte(&self, byte: u8) -> fmt::Result {
        // Wait for the transmit-holding register to be empty (LSR bit 5).
        let ready = wait_for_transmit_ready(|| (unsafe { inb(self.port + 5) }) & 0x20 != 0);
        if !ready {
            return Err(fmt::Error);
        }
        // SAFETY: self.port is the COM1 data register, valid in ring 0.
        unsafe { outb(self.port, byte) };
        Ok(())
    }

    /// Write a byte slice.
    pub fn write_bytes(&self, bytes: &[u8]) -> fmt::Result {
        for &b in bytes {
            self.write_byte(b)?;
        }
        Ok(())
    }

    /// Write a string.
    pub fn write_str(&self, s: &str) -> fmt::Result {
        self.write_bytes(s.as_bytes())
    }
}

/// `fmt::Write` adapter over an immutable [`SerialPort`] reference.
struct SerialWriter<'a>(&'a SerialPort);

impl fmt::Write for SerialWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write_str(s)
    }
}

/// `Sync` wrapper so the UART can live in a `static`.
///
/// # Safety
/// The kernel is single-threaded at this stage (no interrupts, no APs), so
/// concurrent access to the UART cannot occur. This must be replaced with a
/// proper lock before interrupts are enabled.
pub(crate) struct SerialConsole(UnsafeCell<SerialPort>);

unsafe impl Sync for SerialConsole {}

impl SerialConsole {
    const fn new(port: u16) -> Self {
        SerialConsole(UnsafeCell::new(SerialPort::new(port)))
    }

    fn port(&self) -> &SerialPort {
        // SAFETY: single-threaded; see struct-level safety note.
        unsafe { &*self.0.get() }
    }

    /// Initialize the UART.
    pub fn init(&self) {
        self.port().init();
    }

    /// Write pre-formatted arguments to the console.
    pub fn write_fmt(&self, args: fmt::Arguments) -> fmt::Result {
        use core::fmt::Write as _;
        let mut writer = SerialWriter(self.port());
        writer.write_fmt(args)
    }
}

/// The kernel's debug console (COM1).
pub static COM1: SerialConsole = SerialConsole::new(COM1_BASE);

/// Initialize the serial console.
pub fn init() {
    COM1.init();
}

/// Print a formatted string to the serial debug console.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = $crate::arch::x86_64::serial::COM1.write_fmt(format_args!($($arg)*));
    }};
}

/// Print a formatted string followed by a newline to the serial debug console.
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {{
        $crate::print!($($arg)*);
        $crate::print!("\n");
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_program_layout() {
        let prog = init_program(COM1_BASE);
        assert_eq!(prog.len(), INIT_LEN);
        // LCR: clear DLAB, 8N1
        assert_eq!(
            prog[0],
            PortWrite {
                port: COM1_BASE + 3,
                value: 0x03
            }
        );
        // IER: disable interrupts
        assert_eq!(
            prog[1],
            PortWrite {
                port: COM1_BASE + 1,
                value: 0x00
            }
        );
        // LCR: enable DLAB
        assert_eq!(
            prog[2],
            PortWrite {
                port: COM1_BASE + 3,
                value: 0x80
            }
        );
        // DLL: divisor low (115200)
        assert_eq!(
            prog[3],
            PortWrite {
                port: COM1_BASE,
                value: 0x01
            }
        );
        // DLM: divisor high
        assert_eq!(
            prog[4],
            PortWrite {
                port: COM1_BASE + 1,
                value: 0x00
            }
        );
        // LCR: restore 8N1
        assert_eq!(
            prog[5],
            PortWrite {
                port: COM1_BASE + 3,
                value: 0x03
            }
        );
        // FCR: enable + clear FIFO
        assert_eq!(
            prog[6],
            PortWrite {
                port: COM1_BASE + 2,
                value: 0xC7
            }
        );
        // MCR: DTR + RTS + OUT2
        assert_eq!(
            prog[7],
            PortWrite {
                port: COM1_BASE + 4,
                value: 0x0B
            }
        );
    }

    #[test]
    fn test_init_program_respects_base() {
        let prog = init_program(0x2f8);
        assert_eq!(prog[0].port, 0x2fb);
        assert_eq!(prog[3].port, 0x2f8);
    }

    /// Register-model test: the init sequence must clear IER regardless of the
    /// inherited DLAB state. Register 1 is IER when DLAB=0 and DLM when DLAB=1.
    #[test]
    fn test_init_program_clears_dlab_before_ier() {
        for initial_dlab in [false, true] {
            let mut dll = 0u8;
            let mut ier = 0xFFu8; // inherited: all interrupts enabled
            let mut dlm = 0u8;
            let mut lcr = if initial_dlab { 0x80 } else { 0x00 };

            for w in init_program(COM1_BASE) {
                let offset = (w.port - COM1_BASE) as usize;
                match offset {
                    0 => dll = w.value,
                    1 => {
                        if lcr & 0x80 != 0 {
                            dlm = w.value; // DLAB=1: register 1 is DLM
                        } else {
                            ier = w.value; // DLAB=0: register 1 is IER
                        }
                    }
                    3 => lcr = w.value,
                    _ => {}
                }
            }

            assert_eq!(lcr, 0x03, "LCR should be 8N1 (DLAB=0)");
            assert_eq!(ier, 0x00, "IER must be cleared regardless of initial DLAB");
            assert_eq!(dll, 0x01, "DLL divisor low");
            assert_eq!(dlm, 0x00, "DLM divisor high");
        }
    }

    #[test]
    fn test_write_byte_proceeds_when_ready() {
        let ready = wait_for_transmit_ready(|| true);
        assert!(ready);
    }

    #[test]
    fn test_write_byte_proceeds_after_delay() {
        let mut polls = 0u32;
        let ready = wait_for_transmit_ready(|| {
            polls += 1;
            polls > 3 // ready after a few polls
        });
        assert!(ready);
        assert_eq!(polls, 4);
    }

    #[test]
    fn test_write_byte_drops_when_transmitter_never_ready() {
        let mut polls = 0u32;
        let ready = wait_for_transmit_ready(|| {
            polls += 1;
            false // never ready
        });
        assert!(!ready);
        assert_eq!(polls, TX_READY_BUDGET);
    }
}
