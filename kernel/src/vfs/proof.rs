//! The same assertions run as host tests and in the real post-firmware kernel.
use super::{
    Error, Filesystem, Kind, Result, Vfs,
    devfs::DevFs,
    files::{Files, Open},
    memfs::MemFs,
};

pub fn self_test(mut report: impl FnMut(&str)) -> Result<()> {
    let mut ram = MemFs::<12, 64>::new()?;
    let mut dev = DevFs::new();
    let root = ram.root();
    let stale = ram.create(root, "recycled", Kind::File)?;
    ram.remove(root, "recycled")?;
    let fresh = ram.create(root, "fresh", Kind::File)?;
    assert_ne!(stale, fresh);
    assert_eq!(ram.metadata(stale), Err(Error::StaleNode));
    ram.remove(root, "fresh")?;
    let mut vfs = Vfs::<2>::new(&mut ram)?;
    vfs.create("/dev", Kind::Directory)?;
    vfs.create("/tmp", Kind::Directory)?;
    vfs.mount("/dev", &mut dev)?;
    let file = vfs.create("/tmp/note", Kind::File)?;
    assert_eq!(vfs.write(file, 3, b"vibrix")?, 6);
    let mut data = [0xa5; 16];
    assert_eq!(vfs.read(file, 0, &mut data)?, 9);
    assert_eq!(&data[..9], b"\0\0\0vibrix");
    assert_eq!(vfs.write(file, 63, b"XX"), Err(Error::NoSpace));
    assert_eq!(vfs.metadata(file)?.len, 9);
    assert_eq!(vfs.resolve("/dev/../tmp/./note")?, file);
    assert_eq!(vfs.resolve("/missing/../tmp"), Err(Error::NotFound));
    assert_eq!(vfs.resolve("/tmp/note/.."), Err(Error::NotDirectory));
    assert_eq!(vfs.remove("/dev"), Err(Error::Busy));
    report("VIBRIX: kernel VFS and memory filesystem verified");

    let mut files = Files::<2, 8, 1, 8>::new(vfs);
    let fd = files.open("/tmp/note", Open::READ_WRITE)?;
    files.seek(fd, 3)?;
    let duplicate = files.dup(fd)?;
    assert_eq!(files.read(fd, &mut data[..2])?, 2);
    assert_eq!(&data[..2], b"vi");
    assert_eq!(files.read(duplicate, &mut data[..4])?, 4);
    assert_eq!(&data[..4], b"brix");
    let separate = files.open("/tmp/note", Open::READ)?;
    assert_eq!(files.read(separate, &mut data[..3])?, 3);
    assert_eq!(&data[..3], &[0, 0, 0]);
    assert_eq!(files.write(separate, b"x"), Err(Error::AccessDenied));
    assert_eq!(files.remove("/tmp/note"), Err(Error::Busy));
    files.close(fd)?;
    assert_eq!(files.read(fd, &mut data), Err(Error::BadDescriptor));
    files.close(duplicate)?;
    files.close(separate)?;
    report("VIBRIX: kernel file descriptors verified");

    let null = files.open("/dev/null", Open::READ_WRITE)?;
    assert_eq!(files.write(null, b"discard")?, 7);
    assert_eq!(files.read(null, &mut data)?, 0);
    files.close(null)?;
    let zero = files.open("/dev/zero", Open::READ)?;
    data.fill(0xa5);
    assert_eq!(files.read(zero, &mut data)?, data.len());
    assert_eq!(data, [0; 16]);
    assert_eq!(files.seek(zero, 0), Err(Error::NotSeekable));
    files.close(zero)?;
    report("VIBRIX: kernel devfs null and zero verified");

    let tty = files.open("/dev/tty", Open::READ_WRITE)?;
    assert_eq!(files.read(tty, &mut data), Err(Error::WouldBlock));
    for byte in [b'h', b'e', b'l', b'x', 0x08, b'l', b'o', b'\r'] {
        files.device_input("/dev/tty", byte)?;
    }
    assert_eq!(files.read(tty, &mut data)?, 6);
    assert_eq!(&data[..6], b"hello\n");
    assert_eq!(files.write(tty, b"ready\n")?, 6);
    data.fill(0);
    assert_eq!(files.device_output("/dev/tty", &mut data)?, 6);
    assert_eq!(&data[..6], b"ready\n");
    assert_eq!(files.device_output("/dev/tty", &mut data)?, 0);
    files.close(tty)?;
    report("VIBRIX: kernel TTY line discipline verified");

    let (reader, writer) = files.pipe()?;
    assert_eq!(files.read(reader, &mut data), Err(Error::WouldBlock));
    assert_eq!(files.write(writer, b"abcdef")?, 6);
    assert_eq!(files.write(writer, b"XYZ"), Err(Error::WouldBlock));
    assert_eq!(files.read(reader, &mut data[..4])?, 4);
    assert_eq!(&data[..4], b"abcd");
    assert_eq!(files.write(writer, b"123456")?, 6);
    let writer_dup = files.dup(writer)?;
    files.close(writer)?;
    assert_eq!(files.read(reader, &mut data)?, 8);
    assert_eq!(&data[..8], b"ef123456");
    assert_eq!(files.read(reader, &mut data), Err(Error::WouldBlock));
    files.close(writer_dup)?;
    assert_eq!(files.read(reader, &mut data)?, 0);
    files.close(reader)?;
    let (reader, writer) = files.pipe()?;
    files.close(reader)?;
    assert_eq!(files.write(writer, b"x"), Err(Error::BrokenPipe));
    files.close(writer)?;
    report("VIBRIX: kernel pipe lifecycle verified");
    files.remove("/tmp/note")?;
    Ok(())
}
