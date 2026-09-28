//! Development access to the volatile bootstrap filesystem, not a userspace
//! shell. Output is injected so command behavior is tested without hardware.
use super::{
    Error, Kind, Result, Vfs,
    devfs::DevFs,
    files::{Files, Open},
    memfs::MemFs,
};
use core::fmt::Write;

pub type BootstrapRoot = MemFs<16, 256>;
pub type BootstrapFiles<'a> = Files<'a, 2, 16, 2, 256>;

pub fn bootstrap<'a>(
    root: &'a mut BootstrapRoot,
    devices: &'a mut DevFs,
) -> Result<BootstrapFiles<'a>> {
    let mut vfs = Vfs::<2>::new(root)?;
    vfs.create("/dev", Kind::Directory)?;
    vfs.create("/tmp", Kind::Directory)?;
    let welcome = vfs.create("/welcome", Kind::File)?;
    vfs.write(
        welcome,
        0,
        b"Vibrix bootstrap filesystem: files live in RAM until reboot.\n",
    )?;
    vfs.mount("/dev", devices)?;
    Ok(Files::new(vfs))
}

/// Returns false only for a command outside this module's registry.
pub fn execute(files: &mut BootstrapFiles<'_>, line: &str, output: &mut impl Write) -> bool {
    let line = line.trim();
    let (command, args) = line
        .split_once(' ')
        .map_or((line, ""), |(command, args)| (command, args.trim_start()));
    let result = match command {
        "ls" => list(files, if args.is_empty() { "/" } else { args }, output),
        "cat" if !args.is_empty() => cat(files, args, output),
        "mkdir" if !args.is_empty() => files.mkdir(args),
        "rm" if !args.is_empty() => files.remove(args),
        "write" => replace(files, args, output),
        "pipe" if args.is_empty() => pipe_roundtrip(files, output),
        "cat" | "mkdir" | "rm" | "pipe" => Err(Error::InvalidPath),
        _ => return false,
    };
    if let Err(error) = result {
        let _ = writeln!(output, "fs: {error:?}");
    }
    true
}

fn list(files: &BootstrapFiles<'_>, path: &str, output: &mut impl Write) -> Result<()> {
    let mut index = 0;
    while let Some(entry) = files.entry(path, index)? {
        let suffix = if entry.kind == Kind::Directory {
            "/"
        } else {
            ""
        };
        let _ = writeln!(output, "{}{}", entry.name.as_str(), suffix);
        index += 1;
    }
    Ok(())
}

fn cat(files: &mut BootstrapFiles<'_>, path: &str, output: &mut impl Write) -> Result<()> {
    // Unbounded devices such as zero must not hang the polled console.
    if files.metadata(path)?.kind != Kind::File {
        return Err(Error::Unsupported);
    }
    let fd = files.open(path, Open::READ)?;
    let result = (|| {
        let mut buffer = [0; 64];
        loop {
            let count = files.read(fd, &mut buffer)?;
            if count == 0 {
                break;
            }
            for &byte in &buffer[..count] {
                // Do not let file contents inject terminal control sequences.
                let ch = if byte.is_ascii_graphic() || byte == b' ' || byte == b'\n' {
                    char::from(byte)
                } else {
                    '.'
                };
                let _ = write!(output, "{ch}");
            }
        }
        let _ = writeln!(output);
        Ok(())
    })();
    files.close(fd)?;
    result
}

fn replace(files: &mut BootstrapFiles<'_>, args: &str, output: &mut impl Write) -> Result<()> {
    let (path, text) = args.split_once(' ').ok_or(Error::InvalidPath)?;
    if text.len() > 256 {
        return Err(Error::NoSpace);
    }
    match files.metadata(path) {
        Err(Error::NotFound) => files.create(path)?,
        Ok(_) => {}
        Err(error) => return Err(error),
    }
    let fd = files.open(path, Open::REPLACE)?;
    let result = files.write(fd, text.as_bytes());
    files.close(fd)?;
    let count = result?;
    let _ = writeln!(output, "wrote {count} bytes (RAM)");
    Ok(())
}

fn pipe_roundtrip(files: &mut BootstrapFiles<'_>, output: &mut impl Write) -> Result<()> {
    let (reader, writer) = files.pipe()?;
    let result = files.write(writer, b"pipe roundtrip");
    files.close(writer)?;
    if let Err(error) = result {
        files.close(reader)?;
        return Err(error);
    }
    let mut data = [0; 32];
    let result = files.read(reader, &mut data);
    files.close(reader)?;
    let count = result?;
    let text = core::str::from_utf8(&data[..count]).map_err(|_| Error::BackendContract)?;
    let _ = writeln!(output, "{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::String;

    #[test]
    fn commands_use_retained_files_and_mounts() {
        let mut root = BootstrapRoot::new().unwrap();
        let mut devices = DevFs::new();
        let mut files = bootstrap(&mut root, &mut devices).unwrap();
        let mut output = String::new();
        for command in [
            "mkdir /tmp/test",
            "write /tmp/test/note hello",
            "cat /tmp/test/note",
            "ls /dev",
            "pipe",
        ] {
            assert!(execute(&mut files, command, &mut output));
        }
        assert_eq!(
            output,
            "wrote 5 bytes (RAM)\nhello\nnull\nzero\npipe roundtrip\n"
        );
        output.clear();
        execute(&mut files, "rm /tmp/test/note", &mut output);
        execute(&mut files, "cat /tmp/test/note", &mut output);
        assert_eq!(output, "fs: NotFound\n");
        assert!(!execute(&mut files, "catastrophe /tmp", &mut output));
    }

    #[test]
    fn command_failures_do_not_leak_descriptors_or_hang_on_zero() {
        let mut root = BootstrapRoot::new().unwrap();
        let mut devices = DevFs::new();
        let mut files = bootstrap(&mut root, &mut devices).unwrap();
        for _ in 0..32 {
            let mut output = String::new();
            execute(&mut files, "cat /dev/zero", &mut output);
            execute(&mut files, "write /dev/null test", &mut output);
            execute(&mut files, "pipe bad", &mut output);
            assert_eq!(
                output,
                "fs: Unsupported\nfs: NotSeekable\nfs: InvalidPath\n"
            );
        }
        let mut output = String::new();
        execute(&mut files, "pipe", &mut output);
        assert_eq!(output, "pipe roundtrip\n");
    }
}
