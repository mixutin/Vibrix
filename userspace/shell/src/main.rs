#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibrix_shell::{Builtin, Command, LINE_BYTES};
use vibrix_syscall as syscall;

const STDIN: u64 = 0;
const STDOUT: u64 = 1;
const STDERR: u64 = 2;

fn write(bytes: &[u8]) {
    let _ = syscall::write(STDOUT, bytes);
}

fn dispatch(line: &[u8]) -> bool {
    let Ok(command) = Command::parse(line) else {
        write(b"sh: too many arguments\n");
        return true;
    };
    match command.builtin {
        None if command.argc == 0 => {}
        None => write(b"sh: unknown command\n"),
        Some(Builtin::Help) => {
            write(b"cat echo ls pwd cd mkdir cp mv rm ps kill exit help\n");
        }
        Some(Builtin::Echo) => {
            for index in 1..command.argc {
                if index != 1 {
                    write(b" ");
                }
                write(command.args[index]);
            }
            write(b"\n");
        }
        Some(Builtin::Exit) => return false,
        Some(_) => write(b"sh: command backend not ready\n"),
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut line = [0u8; LINE_BYTES];
    let mut len = 0usize;
    write(b"Vibrix shell\nvibrix$ ");

    loop {
        let mut byte = [0u8; 1];
        match syscall::read(STDIN, &mut byte) {
            Ok(0) => {
                let _ = syscall::yield_now();
            }
            Ok(_) => match byte[0] {
                b'\n' | b'\r' => {
                    write(b"\n");
                    if !dispatch(&line[..len]) {
                        let _ = syscall::exit(0);
                    }
                    len = 0;
                    write(b"vibrix$ ");
                }
                8 | 127 => {
                    if len != 0 {
                        len -= 1;
                    }
                }
                value if len < line.len() => {
                    line[len] = value;
                    len += 1;
                }
                _ => {}
            },
            Err(_) => {
                let _ = syscall::yield_now();
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = syscall::write(STDERR, b"Vibrix shell panic\n");
    loop {
        core::hint::spin_loop();
    }
}
