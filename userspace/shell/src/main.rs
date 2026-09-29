#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_shell::{
    LINE_BYTES, Shell,
    runtime::render_fetch,
    system::{Native, write_all},
};
use vibrix_syscall as syscall;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut io = Native;
    let mut shell = Shell::new();
    let mut line = [0u8; LINE_BYTES];
    let mut len = 0usize;
    let mut overflow = false;
    let _ = write_all(&mut io, 1, b"Vibrix shell\n");
    let _ = render_fetch(&mut io, 1);
    let _ = write_all(&mut io, 1, b"Type help for commands.\nvibrix$ ");
    loop {
        let mut byte = [0u8; 1];
        match syscall::read(0, &mut byte) {
            Ok(0) | Err(_) => {
                let _ = syscall::yield_now();
            }
            Ok(_) => match byte[0] {
                b'\n' | b'\r' => {
                    let _ = write_all(&mut io, 1, b"\n");
                    if overflow {
                        let _ =
                            write_all(&mut io, 2, b"sh: input too long; command not executed\n");
                        shell.status = 2;
                    } else {
                        shell.run(&mut io, &line[..len]);
                    }
                    if let Some(status) = shell.exit {
                        let _ = write_all(
                            &mut io,
                            1,
                            b"Boot shell stopped; restart the VM for a new session.\n",
                        );
                        let _ = syscall::exit(u64::from(status));
                        loop {
                            core::hint::spin_loop();
                        }
                    }
                    len = 0;
                    overflow = false;
                    let _ = write_all(&mut io, 1, b"vibrix$ ");
                }
                8 | 127 if !overflow => len = len.saturating_sub(1),
                value if !overflow && len < line.len() => {
                    line[len] = value;
                    len += 1;
                }
                _ => overflow = true,
            },
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = syscall::write(2, b"Vibrix shell panic\n");
    loop {
        core::hint::spin_loop();
    }
}
