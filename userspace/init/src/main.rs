#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_syscall as syscall;

const STDOUT: u64 = 1;

#[cfg(feature = "advisory-lock-probe")]
fn advisory_lock_self_test() -> bool {
    let path = b"/tmp/lock-probe";
    if syscall::create(path).is_err() {
        return false;
    }
    let Ok(first) = syscall::open(path, syscall::abi::OPEN_READ_WRITE) else {
        return false;
    };
    let Ok(second) = syscall::open(path, syscall::abi::OPEN_READ_WRITE) else {
        let _ = syscall::close(first);
        return false;
    };

    let passed = syscall::advisory_lock(first, syscall::abi::FD_LOCK_EXCLUSIVE).is_ok()
        && syscall::advisory_lock(second, syscall::abi::FD_LOCK_SHARED)
            == Err(syscall::abi::Errno::Busy.code())
        && syscall::advisory_lock(first, syscall::abi::FD_LOCK_UNLOCK).is_ok()
        && syscall::advisory_lock(second, syscall::abi::FD_LOCK_SHARED).is_ok()
        && syscall::advisory_lock(second, syscall::abi::FD_LOCK_UNLOCK).is_ok();

    let closed = syscall::close(first).is_ok() && syscall::close(second).is_ok();
    let removed = syscall::remove(path).is_ok();
    passed && closed && removed
}

fn credential_transition_self_test() -> bool {
    let mut uids = syscall::abi::IdTriple::ROOT;
    let mut gids = syscall::abi::IdTriple::ROOT;
    if syscall::getresuid(&mut uids).is_err()
        || syscall::getresgid(&mut gids).is_err()
        || uids != syscall::abi::IdTriple::ROOT
        || gids != syscall::abi::IdTriple::ROOT
    {
        return false;
    }

    if syscall::setresgid(Some(0), Some(1000), Some(0)).is_err()
        || syscall::setresuid(Some(0), Some(1000), Some(0)).is_err()
        || syscall::getresuid(&mut uids).is_err()
        || syscall::getresgid(&mut gids).is_err()
        || uids.real != 0
        || uids.effective != 1000
        || uids.saved != 0
        || gids.real != 0
        || gids.effective != 1000
        || gids.saved != 0
    {
        return false;
    }

    // Effective UID is now non-root. Regaining UID 0 is permitted only
    // because 0 remains in the real/saved set; this exercises the non-root
    // transition rule rather than a blanket privilege bypass.
    if syscall::setresuid(Some(0), Some(0), Some(0)).is_err()
        || syscall::setresgid(Some(0), Some(0), Some(0)).is_err()
        || syscall::getresuid(&mut uids).is_err()
        || syscall::getresgid(&mut gids).is_err()
    {
        return false;
    }

    uids == syscall::abi::IdTriple::ROOT && gids == syscall::abi::IdTriple::ROOT
}

#[unsafe(no_mangle)]
/// PID 1 entry used by the real ELF/runtime proof.
pub extern "C" fn _start() -> ! {
    #[cfg(feature = "advisory-lock-probe")]
    let status = if syscall::getpid() == Ok(1) && advisory_lock_self_test() {
        let _ = syscall::write(STDOUT, b"Vibrix advisory lock probe passed\n");
        0
    } else {
        1
    };

    #[cfg(not(feature = "advisory-lock-probe"))]
    let status = if syscall::getpid() == Ok(1) && credential_transition_self_test() {
        0
    } else {
        1
    };
    let _ = syscall::write(STDOUT, b"Vibrix init starting\n");
    let _ = syscall::exit(status);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = syscall::exit(127);
    loop {
        core::hint::spin_loop();
    }
}
