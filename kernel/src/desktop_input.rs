//! Exclusive polling i8042 owner for the single-process desktop profile.
//! QEMU reference only: no IRQ delivery, USB integration, wheel or hotplug.
use crate::arch::x86_64::ps2::SetOne;
use crate::framebuffer::desktop::abi;
use core::{
    arch::asm,
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
#[path = "../../shared/desktop_mouse.rs"]
mod mouse;
#[path = "../../shared/desktop_queue.rs"]
mod queue;

struct Input {
    keyboard: SetOne,
    mouse: mouse::Mouse,
    events: queue::Queue<abi::InputEvent, 256>,
}
struct Slot(UnsafeCell<Input>);
// SAFETY: sole BSP; pre-STI setup followed by IF-masked InputPoll or
// graphics pumping calls. Rendering never overlaps another input borrow.
// Canonical TTY reads cannot poll hardware in this feature. No IRQ/AP users.
unsafe impl Sync for Slot {}
static INPUT: Slot = Slot(UnsafeCell::new(Input {
    keyboard: SetOne::new(),
    mouse: mouse::Mouse::new(),
    events: queue::Queue::new(),
}));
static READY: AtomicBool = AtomicBool::new(false);
static POINTER: AtomicBool = AtomicBool::new(false);
const POLLS: usize = 100_000;

fn input(port: u16) -> u8 {
    let byte: u8;
    // SAFETY: invoked only on CPL0 BSP with sole ownership of i8042 ports.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") byte, options(nomem, nostack, preserves_flags))
    };
    byte
}
fn output(port: u16, byte: u8) {
    // SAFETY: same ownership; caller supplies only i8042 command/data ports.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") byte, options(nomem, nostack, preserves_flags))
    };
}
fn send(port: u16, byte: u8) -> Result<(), ()> {
    for _ in 0..POLLS {
        if input(0x64) & 2 == 0 {
            output(port, byte);
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(())
}
fn aux_byte() -> Result<u8, ()> {
    for _ in 0..POLLS {
        let status = input(0x64);
        if status & 1 != 0 {
            let byte = input(0x60);
            if status & 0xc0 != 0 {
                return Err(());
            }
            if status & 0x20 != 0 {
                return Ok(byte);
            }
        }
        core::hint::spin_loop();
    }
    Err(())
}
fn mouse_command(command: u8) -> Result<(), ()> {
    send(0x64, 0xd4)?;
    send(0x60, command)?;
    if aux_byte()? == 0xfa { Ok(()) } else { Err(()) }
}

/// # Safety
/// Run once after firmware exit, before STI/userspace. No other owner may
/// consume i8042 in this profile. IRQ1/12 remain masked in the BSP IRQ setup.
pub unsafe fn init() {
    if READY.swap(true, Ordering::SeqCst) {
        return;
    }
    // Bound stale firmware input draining. No user session exists yet.
    for _ in 0..256 {
        if input(0x64) & 1 == 0 {
            break;
        }
        let _ = input(0x60);
    }
    let initialized = (|| {
        send(0x64, 0xa8)?;
        // Reset selects standard type zero (three-byte packets), even if
        // firmware previously negotiated a wheel mouse. Require all replies.
        mouse_command(0xff)?;
        if aux_byte()? != 0xaa || aux_byte()? != 0 {
            return Err(());
        }
        mouse_command(0xf6)?;
        mouse_command(0xf4)
    })()
    .is_ok();
    POINTER.store(initialized, Ordering::SeqCst);
    if initialized {
        crate::debugcon::write("VIBRIX: desktop PS2 pointer ready\r\n");
    } else {
        crate::debugcon::write("VIBRIX: desktop keyboard-only input\r\n");
    }
}

pub fn pointer_ready() -> bool {
    POINTER.load(Ordering::SeqCst)
}

/// Capture input during long software repaints. Stop before consuming a
/// byte when the queue is full; no queued key or click is overwritten.
/// Caller is the sole BSP with IF=0 under either retained kernel/user CR3.
pub fn pump() {
    if !READY.load(Ordering::SeqCst) {
        return;
    }
    // SAFETY: no IRQ/AP consumer, no callback can recursively borrow INPUT.
    let state = unsafe { &mut *INPUT.0.get() };
    for _ in 0..64 {
        if state.events.full() {
            break;
        }
        let Some(event) = hardware_event(state) else {
            break;
        };
        let _ = state.events.push(event);
    }
}

pub fn poll() -> Option<abi::InputEvent> {
    pump();
    // SAFETY: pump's borrow ended; same sole-BSP/IF=0 invariant.
    unsafe { &mut *INPUT.0.get() }.events.pop()
}

fn hardware_event(state: &mut Input) -> Option<abi::InputEvent> {
    for _ in 0..32 {
        let status = input(0x64);
        if status & 1 == 0 {
            return None;
        }
        let byte = input(0x60);
        if status & 0xc0 != 0 {
            state.mouse.reset();
            continue;
        }
        if status & 0x20 != 0 {
            if pointer_ready()
                && let Some(report) = state.mouse.feed(byte)
            {
                return Some(abi::InputEvent {
                    kind: abi::EVENT_POINTER,
                    code: report.buttons,
                    x: report.dx,
                    y: report.dy,
                });
            }
        } else if let Some(code) = state.keyboard.feed_desktop(byte) {
            return Some(abi::InputEvent {
                kind: abi::EVENT_KEY,
                code,
                x: 0,
                y: 0,
            });
        }
    }
    None
}
