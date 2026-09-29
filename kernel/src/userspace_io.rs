//! Feature-gated single-BSP userspace stdio owner.
//!
//! This binds ABI descriptors 0/1/2 to the existing /dev/tty VFS node. It is
//! deliberately bounded bootstrap plumbing: one process, one CPU, no IRQ-side
//! VFS access, and no descriptor inheritance yet.

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use vibrix_kernel::vfs::{
    Error, Result,
    console::{BootstrapFiles, BootstrapRoot, bootstrap},
    devfs::DevFs,
    files::{Access, Open},
};

use crate::arch::x86_64::{ps2, serial};

struct StaticCell<T>(UnsafeCell<T>);

// SAFETY: this entire module is enabled only for the single-BSP userspace I/O
// proof. Initialization occurs once before entering CPL3; all later access is
// from the same CPU with syscall FMASK keeping IF clear.
unsafe impl<T> Sync for StaticCell<T> {}

static ROOT: StaticCell<MaybeUninit<BootstrapRoot>> =
    StaticCell(UnsafeCell::new(MaybeUninit::uninit()));
static DEVICES: StaticCell<MaybeUninit<DevFs>> = StaticCell(UnsafeCell::new(MaybeUninit::uninit()));
static FILES: StaticCell<Option<BootstrapFiles<'static>>> = StaticCell(UnsafeCell::new(None));
static KEYS: StaticCell<ps2::SetOne> = StaticCell(UnsafeCell::new(ps2::SetOne::new()));
static READY: AtomicBool = AtomicBool::new(false);
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
    if READY.load(Ordering::SeqCst) {
        return Err(InitError::AlreadyInitialized);
    }

    let result = (|| {
        // SAFETY: one-time initialization of static backing before any live
        // references are published. These objects never move afterward.
        let root: &'static mut BootstrapRoot = unsafe {
            let slot = &mut *ROOT.0.get();
            slot.write(BootstrapRoot::new()?);
            &mut *slot.as_mut_ptr()
        };
        // SAFETY: identical one-time static initialization invariant as ROOT.
        let devices: &'static mut DevFs = unsafe {
            let slot = &mut *DEVICES.0.get();
            slot.write(DevFs::new());
            &mut *slot.as_mut_ptr()
        };
        let mut files = bootstrap(root, devices)?;

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

        // SAFETY: no userspace syscall can access FILES before READY remains
        // published true at function return on this single CPU.
        unsafe { *FILES.0.get() = Some(files) };
        Ok(())
    })();

    if result.is_ok() {
        READY.store(true, Ordering::SeqCst);
    }
    result
}

fn with_files<T>(operation: impl FnOnce(&mut BootstrapFiles<'static>) -> Result<T>) -> Result<T> {
    if !READY.load(Ordering::SeqCst) {
        return Err(Error::BadDescriptor);
    }
    // SAFETY: single BSP, syscall entry has IF masked, and no reentrant VFS
    // caller exists in this bounded proof. The mutable borrow never escapes.
    let files = unsafe { (*FILES.0.get()).as_mut().ok_or(Error::BadDescriptor)? };
    operation(files)
}

pub fn read(fd: usize, buffer: &mut [u8]) -> Result<usize> {
    loop {
        match with_files(|files| files.read(fd, buffer)) {
            Ok(count) => return Ok(count),
            Err(Error::WouldBlock) if fd == 0 => {
                if !INPUT_WAIT_REPORTED.swap(true, Ordering::SeqCst) {
                    crate::debugcon::write("VIBRIX: userspace TTY read waiting for keyboard\r\n");
                }
                // SAFETY: this proof owns the sole post-UEFI i8042 consumer.
                if let Some(scan) = unsafe { ps2::poll_scancode() } {
                    // SAFETY: same single-BSP ownership as FILES.
                    if let Some(ascii) = unsafe { &mut *KEYS.0.get() }.feed(scan) {
                        with_files(|files| files.device_input("/dev/tty", ascii))?;
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
        // Userspace output is bytes, not trusted UTF-8. COM1 already exposes
        // a byte-oriented bounded writer.
        serial::write_bytes(&drained[..n]).map_err(|_| Error::BackendContract)?;
    }
    Ok(count)
}
