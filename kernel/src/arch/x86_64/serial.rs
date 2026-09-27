//! x86-64 COM1 (16550) serial debug console.
//!
//! Clean-room implementation. Provides the kernel's first output mechanism.
//! The UART programming sequence is computed by a pure function
//! ([`init_program`]) so it can be unit-tested off-target; [`SerialPort::init`]
//! applies it to hardware.

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
pub const INIT_LEN: usize = 7;

/// Compute the 16550 initialization sequence for a UART at `base`.
///
/// Programs 115200 baud (divisor 1), 8N1, FIFO enabled with a 14-byte
/// threshold, and DTR/RTS/OUT2 asserted.
pub fn init_program(base: u16) -> [PortWrite; INIT_LEN] {
    [
        PortWrite {
            port: base + 1,
            value: 0x00,
        }, // IER: disable interrupts
        PortWrite {
            port: base + 3,
            value: 0x80,
        }, // LCR: enable DLAB
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
        }, // LCR: 8 bits, no parity, 1 stop
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
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
    }
}

/// Read a byte from an I/O port.
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack));
    }
    value
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
            unsafe { outb(w.port, w.value) };
        }
    }

    /// Write a single byte, waiting for the transmitter to be ready.
    pub fn write_byte(&self, byte: u8) {
        // Wait for the transmit-holding register to be empty (LSR bit 5).
        while (unsafe { inb(self.port + 5) }) & 0x20 == 0 {
            core::hint::spin_loop();
        }
        unsafe { outb(self.port, byte) };
    }

    /// Write a byte slice.
    pub fn write_bytes(&self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }

    /// Write a string.
    pub fn write_str(&self, s: &str) {
        self.write_bytes(s.as_bytes());
    }
}

/// `fmt::Write` adapter over an immutable [`SerialPort`] reference.
struct SerialWriter<'a>(&'a SerialPort);

impl fmt::Write for SerialWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write_str(s);
        Ok(())
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
        // IER: disable interrupts
        assert_eq!(
            prog[0],
            PortWrite {
                port: COM1_BASE + 1,
                value: 0x00
            }
        );
        // LCR: enable DLAB
        assert_eq!(
            prog[1],
            PortWrite {
                port: COM1_BASE + 3,
                value: 0x80
            }
        );
        // DLL: divisor low (115200)
        assert_eq!(
            prog[2],
            PortWrite {
                port: COM1_BASE + 0,
                value: 0x01
            }
        );
        // DLM: divisor high
        assert_eq!(
            prog[3],
            PortWrite {
                port: COM1_BASE + 1,
                value: 0x00
            }
        );
        // LCR: 8N1
        assert_eq!(
            prog[4],
            PortWrite {
                port: COM1_BASE + 3,
                value: 0x03
            }
        );
        // FCR: enable + clear FIFO
        assert_eq!(
            prog[5],
            PortWrite {
                port: COM1_BASE + 2,
                value: 0xC7
            }
        );
        // MCR: DTR + RTS + OUT2
        assert_eq!(
            prog[6],
            PortWrite {
                port: COM1_BASE + 4,
                value: 0x0B
            }
        );
    }

    #[test]
    fn test_init_program_respects_base() {
        let prog = init_program(0x2f8);
        assert_eq!(prog[0].port, 0x2f9);
        assert_eq!(prog[2].port, 0x2f8);
    }
}
