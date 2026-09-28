#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_syscall as syscall;

const STDOUT: u64 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let _ = syscall::write(STDOUT, b"Vibrix init starting\n");
    loop {
        let _ = syscall::yield_now();
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = syscall::write(STDOUT, b"Vibrix init panic\n");
    loop {
        core::hint::spin_loop();
    }
}
