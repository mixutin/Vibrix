#![no_std]
#![no_main]
use core::panic::PanicInfo;
use vibrix_desktop::{App, Desktop, Terminal, files::Files, render, system::TerminalIo};
use vibrix_shell::{
    LINE_BYTES, Shell,
    runtime::render_fetch,
    system::{Native, write_all},
};
use vibrix_syscall::{self as syscall, abi, display};

fn fatal(message: &[u8]) -> ! {
    let _ = syscall::write(2, message);
    loop {
        core::hint::spin_loop();
    }
}

/// Exercise negative pointer and rectangle paths in the actual CPL3 guest.
fn check_boundary() -> bool {
    // SAFETY: deliberately invalid pointers are never dereferenced in user
    // code. Kernel must reject them through checked copy and return an errno.
    let bad_info =
        unsafe { syscall::raw_syscall6(abi::Syscall::DisplayInfo.number(), [1, 0, 0, 0, 0, 0]) };
    let bad_blit =
        unsafe { syscall::raw_syscall6(abi::Syscall::DisplayBlit.number(), [0, 0, 1, 1, 1, 1]) };
    let bad_event =
        unsafe { syscall::raw_syscall6(abi::Syscall::InputPoll.number(), [1, 0, 0, 0, 0, 0]) };
    abi::decode_result(bad_info) == Err(abi::Errno::BadAddress.code())
        && abi::decode_result(bad_blit) == Err(abi::Errno::BadAddress.code())
        && abi::decode_result(bad_event) == Err(abi::Errno::BadAddress.code())
        && syscall::display_fill(
            display::Rect {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1,
            },
            0,
        )
        .is_err()
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    if vibrix_shell::fetch::privilege_level() != 3 {
        fatal(b"desktop: not running at CPL3\n");
    }
    let mut info = display::DisplayInfo::default();
    if syscall::display_info(&mut info).is_err()
        || info.version != display::VERSION
        || info.format != display::XRGB8888
    {
        fatal(b"desktop: supported native graphics unavailable\n");
    }
    let Some(mut desktop) = Desktop::new(info.width, info.height) else {
        fatal(b"desktop: requires 640x480 to 4096x2160\n");
    };
    if !check_boundary() {
        fatal(b"desktop: graphics/input boundary self-check failed\n");
    }
    let _ = syscall::write(
        1,
        b"VIBRIX: desktop CPL3 graphics/input boundary verified\n",
    );
    let mut terminal = Terminal::new(
        ((desktop.window.width - 32) / 12) as usize,
        ((desktop.window.height - 68) / 16) as usize,
    );
    let mut shell = Shell::new();
    let mut backend = Native;
    let mut canvas = render::Native;
    let mut files = Files::new();
    let mut line = [0u8; LINE_BYTES];
    let mut len = 0usize;
    let mut overflow = false;
    {
        let mut io = TerminalIo {
            terminal: &mut terminal,
            backend: &mut backend,
        };
        let _ = write_all(&mut io, 1, b"Vibrix Terminal / native desktop\n");
        let _ = render_fetch(&mut io, 1);
        let _ = write_all(
            &mut io,
            1,
            b"Type help or man. F2 opens real RAM files.\nvibrix$ ",
        );
    }
    if render::draw(&mut canvas, &mut desktop, &mut terminal, &files).is_err() {
        fatal(b"desktop: initial render failed\n");
    }
    let _ = syscall::write(1, b"VIBRIX: desktop ready\n");
    loop {
        let mut changed = false;
        // Bounded event draining coalesces mouse motion without losing clicks.
        for _ in 0..32 {
            let mut event = display::InputEvent::default();
            match syscall::input_poll(&mut event) {
                Ok(false) => break,
                Err(_) => fatal(b"desktop: event polling failed\n"),
                Ok(true) => {}
            }
            changed = true;
            let previous = desktop.app;
            if event.kind == display::EVENT_POINTER {
                if let Some((x, y)) = desktop.pointer(event.x, event.y, event.code)
                    && desktop.app == App::Files
                {
                    if x < 248 && (64..92).contains(&y) {
                        files.parent(&mut backend);
                    } else if x < 248 && y >= 96 {
                        let visible = ((desktop.window.height - 132) / 20) as usize;
                        let first = files.selected / visible * visible;
                        let index = first + (y as usize - 96) / 20;
                        if index < first + visible {
                            files.open(index, &mut backend);
                        }
                    }
                    desktop.full_redraw = true;
                }
            } else if event.kind == display::EVENT_KEY
                && !desktop.key(event.code)
                && desktop.visible
            {
                if desktop.app == App::Files {
                    match event.code {
                        display::KEY_UP => files.selected = files.selected.saturating_sub(1),
                        display::KEY_DOWN if files.count != 0 => {
                            files.selected = (files.selected + 1).min(files.count - 1)
                        }
                        10 | 13 => files.open(files.selected, &mut backend),
                        8 => files.parent(&mut backend),
                        _ => {}
                    }
                    desktop.full_redraw = true;
                } else if desktop.app == App::Terminal {
                    let mut io = TerminalIo {
                        terminal: &mut terminal,
                        backend: &mut backend,
                    };
                    match event.code {
                        10 | 13 => {
                            let _ = write_all(&mut io, 1, b"\n");
                            if overflow {
                                let _ = write_all(
                                    &mut io,
                                    2,
                                    b"sh: input too long; command not executed\n",
                                );
                            } else {
                                shell.run(&mut io, &line[..len]);
                            }
                            if shell.exit.is_some() {
                                shell = Shell::new();
                                let _ = write_all(&mut io, 1, b"Terminal session restarted.\n");
                            }
                            len = 0;
                            overflow = false;
                            let _ = write_all(&mut io, 1, b"vibrix$ ");
                        }
                        8 | 127 if len != 0 && !overflow => {
                            len -= 1;
                            let _ = write_all(&mut io, 1, b"\x08");
                        }
                        21 => {
                            for _ in 0..len {
                                let _ = write_all(&mut io, 1, b"\x08");
                            }
                            len = 0;
                            overflow = false;
                        }
                        9 | 32..=126 if !overflow => {
                            if len < line.len() {
                                let byte = if event.code == 9 {
                                    b' '
                                } else {
                                    event.code as u8
                                };
                                line[len] = byte;
                                len += 1;
                                let _ = write_all(&mut io, 1, &[byte]);
                            } else {
                                overflow = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if desktop.app == App::Files
                && (previous != App::Files
                    || event.kind == display::EVENT_KEY && event.code == display::KEY_F2)
            {
                files.refresh(&mut backend);
            }
        }
        if changed {
            if render::draw(&mut canvas, &mut desktop, &mut terminal, &files).is_err() {
                fatal(b"desktop: render failed\n");
            }
        } else {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    fatal(b"Vibrix desktop panic\n")
}
