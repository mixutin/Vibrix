#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_syscall as syscall;

const STDOUT: u64 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let status = match syscall::getpid() {
        Ok(1) => 0,
        _ => 1,
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
