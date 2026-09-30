//! Single-BSP userspace stdio owner for the shell and diagnostic profiles.
//!
//! ABI descriptors 0/1/2 bind to /dev/tty. The interactive userspace profile
//! mirrors the TTY drain and accepted key edits to the supervisor framebuffer.
//! One process, one CPU, no IRQ-side VFS access or descriptor inheritance.

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

use vibrix_kernel::vfs::{
    Entry, Error, Result,
    console::{BootstrapFiles, bootstrap},
    devfs::DevFs,
    files::{Access, Open, RIGHT_READ, RIGHT_SEEK, RIGHT_WRITE, RIGHTS_ALL},
    memfs::MemFs,
};

use crate::arch::x86_64::{ps2, serial, syscall::abi};

// Only the static userspace root grows. The legacy console keeps its
// original small stack-owned filesystem. Capacity remains deterministic.
type BootstrapRoot = MemFs<64, 1024>;

struct StaticCell<T>(UnsafeCell<T>);

// SAFETY: only the BSP is started. Initialization occurs once before CPL3;
// later access is from the same CPU with syscall FMASK keeping IF clear.
// Neither interrupts nor the framebuffer frontend borrow the VFS state.
unsafe impl<T> Sync for StaticCell<T> {}

// A static initializer is evaluated at compile time. Constructing the RAM
// filesystem through MaybeUninit::write at runtime still creates large stack
// temporaries in debug builds before copying them into the static slot.
static ROOT: StaticCell<BootstrapRoot> = StaticCell(UnsafeCell::new(match BootstrapRoot::new() {
    Ok(root) => root,
    Err(_) => panic!("invalid bootstrap filesystem capacity"),
}));
static DEVICES: StaticCell<DevFs> = StaticCell(UnsafeCell::new(DevFs::new()));
static FILES: StaticCell<Option<BootstrapFiles<'static>>> = StaticCell(UnsafeCell::new(None));
static KEYS: StaticCell<ps2::SetOne> = StaticCell(UnsafeCell::new(ps2::SetOne::new()));
static READY: AtomicBool = AtomicBool::new(false);
static INIT_STARTED: AtomicBool = AtomicBool::new(false);
static INPUT_WAIT_REPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    AlreadyInitialized,
    Vfs(Error),
    DescriptorLayout,
}

impl From<Error> for InitError {
    fn from(error: Error) -> Self {
        Self::Vfs(error)
    }
}

pub fn init() -> core::result::Result<(), InitError> {
    crate::debugcon::write("VIBRIX: userspace stdio initialization entered\r\n");
    if INIT_STARTED.swap(true, Ordering::SeqCst) {
        return Err(InitError::AlreadyInitialized);
    }

    let result = (|| {
        // SAFETY: INIT_STARTED allows these exclusive static borrows only
        // once, including after an initialization failure. Both objects are
        // already valid, never move, and have no other reference owner.
        let root: &'static mut BootstrapRoot = unsafe { &mut *ROOT.0.get() };
        // SAFETY: identical one-time ownership invariant as ROOT.
        let devices: &'static mut DevFs = unsafe { &mut *DEVICES.0.get() };
        let mut files = bootstrap(root, devices)?;
        crate::debugcon::write("VIBRIX: userspace stdio bootstrap VFS ready\r\n");

        let stdin = files.open("/dev/tty", Open::READ)?;
        let write_only = Open {
            access: Access::Write,
            truncate: false,
            append: false,
        };
        let stdout = files.open("/dev/tty", write_only)?;
        let stderr = files.open("/dev/tty", write_only)?;
        if (stdin, stdout, stderr) != (0, 1, 2) {
            return Err(InitError::DescriptorLayout);
        }
        crate::debugcon::write("VIBRIX: userspace stdio descriptors ready\r\n");

        // SAFETY: no userspace syscall can access FILES before READY remains
        // published true at function return on this single CPU.
        unsafe { *FILES.0.get() = Some(files) };
        Ok(())
    })();

    if result.is_ok() {
        READY.store(true, Ordering::SeqCst);
        crate::debugcon::write("VIBRIX: userspace stdio initialization complete\r\n");
    }
    result
}

fn with_files<T>(operation: impl FnOnce(&mut BootstrapFiles<'static>) -> Result<T>) -> Result<T> {
    if !READY.load(Ordering::SeqCst) {
        return Err(Error::BadDescriptor);
    }
    // SAFETY: single BSP, syscall entry has IF masked, and no reentrant VFS
    // caller exists. The mutable borrow never escapes this operation.
    let files = unsafe { (*FILES.0.get()).as_mut().ok_or(Error::BadDescriptor)? };
    operation(files)
}

pub fn open(path: &str, flags: u64) -> Result<usize> {
    let access_bits = flags & 0xff;
    let truncate = flags & abi::OPEN_TRUNCATE != 0;
    if flags & !(0xff | abi::OPEN_TRUNCATE) != 0 {
        return Err(Error::Unsupported);
    }
    let access = match access_bits {
        abi::OPEN_READ => Access::Read,
        abi::OPEN_WRITE => Access::Write,
        abi::OPEN_READ_WRITE => Access::ReadWrite,
        _ => return Err(Error::Unsupported),
    };
    with_files(|files| {
        files.open(
            path,
            Open {
                access,
                truncate,
                append: false,
            },
        )
    })
}

pub fn close(fd: usize) -> Result<()> {
    with_files(|files| files.close(fd))
}

pub fn descriptor_rights(fd: usize) -> Result<u64> {
    with_files(|files| files.rights(fd)).map(u64::from)
}

pub fn restrict_descriptor_rights(fd: usize, rights: u64) -> Result<()> {
    if rights & !abi::FD_RIGHT_ALL != 0 {
        return Err(Error::AccessDenied);
    }
    let mut native = 0u8;
    if rights & abi::FD_RIGHT_READ != 0 {
        native |= RIGHT_READ;
    }
    if rights & abi::FD_RIGHT_WRITE != 0 {
        native |= RIGHT_WRITE;
    }
    if rights & abi::FD_RIGHT_SEEK != 0 {
        native |= RIGHT_SEEK;
    }
    debug_assert_eq!(RIGHTS_ALL, RIGHT_READ | RIGHT_WRITE | RIGHT_SEEK);
    with_files(|files| files.restrict_rights(fd, native))
}

pub fn create(path: &str) -> Result<()> {
    with_files(|files| files.create(path))
}

pub fn mkdir(path: &str) -> Result<()> {
    with_files(|files| files.mkdir(path))
}

pub fn remove(path: &str) -> Result<()> {
    with_files(|files| files.remove(path))
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    with_files(|files| files.rename(from, to))
}

pub fn entry(path: &str, index: usize) -> Result<Option<Entry>> {
    with_files(|files| files.entry(path, index))
}

pub fn read(fd: usize, buffer: &mut [u8]) -> Result<usize> {
    // The desktop is the only i8042 consumer in this profile. Its user-side
    // line editor uses InputPoll; a raw canonical TTY read cannot steal input.
    if cfg!(feature = "userspace-desktop") && fd == 0 {
        return Err(Error::Unsupported);
    }
    loop {
        match with_files(|files| files.read(fd, buffer)) {
            Ok(count) => return Ok(count),
            Err(Error::WouldBlock) if fd == 0 => {
                if !INPUT_WAIT_REPORTED.swap(true, Ordering::SeqCst) {
                    crate::debugcon::write("VIBRIX: userspace TTY read waiting for keyboard\r\n");
                }
                // SAFETY: this owner is the sole post-UEFI i8042 consumer.
                if let Some(scan) = unsafe { ps2::poll_scancode() } {
                    // SAFETY: same single-BSP ownership as FILES.
                    if let Some(ascii) = unsafe { &mut *KEYS.0.get() }.feed(scan) {
                        #[cfg(all(
                            feature = "userspace-shell",
                            not(feature = "userspace-desktop")
                        ))]
                        if !frontend::input_fits(ascii) {
                            continue;
                        }
                        with_files(|files| files.device_input("/dev/tty", ascii))?;
                        #[cfg(all(
                            feature = "userspace-shell",
                            not(feature = "userspace-desktop")
                        ))]
                        frontend::echo(ascii);
                    }
                } else {
                    core::hint::spin_loop();
                }
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn write(fd: usize, buffer: &[u8]) -> Result<usize> {
    let count = with_files(|files| files.write(fd, buffer))?;
    let mut drained = [0u8; 128];
    loop {
        let n = with_files(|files| files.device_output("/dev/tty", &mut drained))?;
        if n == 0 {
            break;
        }
        // Only drained TTY bytes reach the display, not writes to RAM files.
        // The syscall layer has already copied this data into kernel memory.
        #[cfg(all(feature = "userspace-shell", not(feature = "userspace-desktop")))]
        crate::framebuffer::terminal::write(&drained[..n]);
        serial::write_bytes(&drained[..n]).map_err(|_| Error::BackendContract)?;
    }
    Ok(count)
}

#[cfg(all(feature = "userspace-shell", not(feature = "userspace-desktop")))]
mod frontend {
    use crate::{arch::x86_64::serial, framebuffer::terminal};
    use core::sync::atomic::{AtomicUsize, Ordering};

    static LINE_BYTES: AtomicUsize = AtomicUsize::new(0);

    pub fn input_fits(byte: u8) -> bool {
        // Reserve the last byte of the existing 256-byte canonical TTY buffer
        // for Enter. Otherwise a full input line cannot ever be committed.
        !matches!(byte, b'\t' | 0x20..=0x7e) || LINE_BYTES.load(Ordering::SeqCst) < 255
    }

    fn erase() {
        terminal::write(b"\x08");
        let _ = serial::write_bytes(b"\x08 \x08");
    }

    pub fn echo(byte: u8) {
        // Called only after successful device_input, so rejected input never
        // appears on screen. Count input bytes, not prompt characters.
        match byte {
            b'\r' | b'\n' => {
                LINE_BYTES.store(0, Ordering::SeqCst);
                // The current shell writes the dispatch newline itself.
            }
            8 | 127 => {
                if LINE_BYTES.load(Ordering::SeqCst) > 0 {
                    LINE_BYTES.fetch_sub(1, Ordering::SeqCst);
                    erase();
                }
            }
            0x15 => {
                let count = LINE_BYTES.swap(0, Ordering::SeqCst);
                for _ in 0..count {
                    erase();
                }
            }
            b'\t' | 0x20..=0x7e => {
                LINE_BYTES.fetch_add(1, Ordering::SeqCst);
                // Echo a typed tab as one cell so canonical backspace and
                // Ctrl-U always erase exactly the accepted input, not prompt.
                let bytes = [if byte == b'\t' { b' ' } else { byte }];
                terminal::write(&bytes);
                let _ = serial::write_bytes(&bytes);
            }
            _ => {}
        }
    }
}
