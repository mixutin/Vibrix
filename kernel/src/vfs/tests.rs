use super::*;
use devfs::DevFs;
use files::{Access, Files, Open};
use memfs::MemFs;

#[test]
fn guest_behavior_proof_runs_on_production_code() {
    let mut markers = std::vec::Vec::new();
    self_test(|marker| markers.push(std::string::String::from(marker))).unwrap();
    assert_eq!(markers.len(), 7);
}

#[test]
fn directories_names_and_metadata_have_real_lifetimes() {
    let mut fs = MemFs::<4, 8>::new().unwrap();
    let root = fs.root();
    let dir = fs.create(root, "dir", Kind::Directory).unwrap();
    let file = fs.create(dir, "λ", Kind::File).unwrap();
    assert_eq!(fs.lookup(dir, "λ"), Ok(file));
    assert_eq!(fs.create(dir, "λ", Kind::File), Err(Error::Exists));
    assert_eq!(fs.lookup(file, "bad"), Err(Error::NotDirectory));
    assert_eq!(fs.remove(root, "dir"), Err(Error::NotEmpty));
    let entry = fs.entry(dir, 0).unwrap().unwrap();
    assert_eq!((entry.name.as_str(), entry.kind), ("λ", Kind::File));
    assert_eq!(fs.entry(dir, 1), Ok(None));
    fs.remove(dir, "λ").unwrap();
    fs.remove(root, "dir").unwrap();
    assert_eq!(fs.metadata(dir), Err(Error::StaleNode));
    assert_eq!(fs.metadata(file), Err(Error::StaleNode));
}

#[test]
fn slot_reuse_does_not_revive_handles_or_data() {
    let mut fs = MemFs::<2, 8>::new().unwrap();
    let root = fs.root();
    let old = fs.create(root, "old", Kind::File).unwrap();
    fs.write(old, 0, b"secret").unwrap();
    fs.remove(root, "old").unwrap();
    let new = fs.create(root, "new", Kind::File).unwrap();
    assert_ne!(old, new);
    assert_eq!(fs.write(old, 0, b"bad"), Err(Error::StaleNode));
    fs.write(new, 5, b"x").unwrap();
    let mut bytes = [0xaa; 8];
    assert_eq!(fs.read(new, 0, &mut bytes), Ok(6));
    assert_eq!(&bytes[..6], b"\0\0\0\0\0x");
    assert_eq!(&bytes[6..], &[0xaa, 0xaa]);
}

#[test]
fn file_exhaustion_and_overflow_are_atomic() {
    let mut fs = MemFs::<2, 4>::new().unwrap();
    let root = fs.root();
    let file = fs.create(root, "one", Kind::File).unwrap();
    assert_eq!(fs.create(root, "two", Kind::File), Err(Error::NoSpace));
    fs.write(file, 0, b"abcd").unwrap();
    assert_eq!(fs.write(file, 3, b"zz"), Err(Error::NoSpace));
    assert_eq!(fs.write(file, usize::MAX, b"z"), Err(Error::NoSpace));
    let mut bytes = [0; 4];
    assert_eq!(fs.read(file, 0, &mut bytes), Ok(4));
    assert_eq!(&bytes, b"abcd");
    assert_eq!(fs.read(file, usize::MAX, &mut bytes), Ok(0));
    assert_eq!(fs.write(file, usize::MAX, b""), Ok(0));
    assert_eq!(fs.metadata(file).unwrap().len, 4);
    fs.truncate(file).unwrap();
    fs.write(file, 3, b"X").unwrap();
    fs.read(file, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"\0\0\0X");
}

#[test]
fn zero_capacities_reject_without_panicking() {
    assert!(matches!(MemFs::<0, 0>::new(), Err(Error::NoSpace)));
    let mut fs = MemFs::<2, 0>::new().unwrap();
    let file = fs.create(fs.root(), "empty", Kind::File).unwrap();
    assert_eq!(fs.write(file, 0, b"x"), Err(Error::NoSpace));
    assert!(matches!(Vfs::<0>::new(&mut fs), Err(Error::NoSpace)));
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 0, 0, 0>::new(vfs);
    assert_eq!(files.open("/empty", Open::READ), Err(Error::NoSpace));
    assert_eq!(files.pipe(), Err(Error::NoSpace));
}

#[test]
fn invalid_paths_names_and_trailing_slashes_fail_closed() {
    let mut fs = MemFs::<8, 4>::new().unwrap();
    let mut vfs = Vfs::<1>::new(&mut fs).unwrap();
    vfs.create("/a", Kind::Directory).unwrap();
    let file = vfs.create("/a/file", Kind::File).unwrap();
    assert_eq!(vfs.resolve("//a/./file"), Ok(file));
    for bad in ["", "relative", "/a/fi\0le"] {
        assert_eq!(vfs.resolve(bad), Err(Error::InvalidPath));
    }
    assert_eq!(vfs.resolve("/missing/../a"), Err(Error::NotFound));
    assert_eq!(vfs.resolve("/a/file/"), Err(Error::NotDirectory));
    assert_eq!(vfs.resolve("/a/file/.."), Err(Error::NotDirectory));
    assert_eq!(vfs.resolve("/a/file/."), Err(Error::NotDirectory));
    assert_eq!(vfs.create("/new/", Kind::File), Err(Error::NotDirectory));
    assert_eq!(vfs.create("/a/..", Kind::File), Err(Error::InvalidPath));
    assert_eq!(vfs.resolve("/../../a/file"), Ok(file));
    assert_eq!(
        vfs.resolve(&std::format!("/{}", "x".repeat(NAME_MAX + 1))),
        Err(Error::NameTooLong)
    );
    assert_eq!(
        vfs.resolve(&std::format!("/{}", "x".repeat(PATH_MAX))),
        Err(Error::NameTooLong)
    );
    assert_eq!(vfs.remove("/"), Err(Error::Busy));
}

#[test]
fn mounted_backend_overrides_only_the_exact_directory() {
    let mut fs = MemFs::<8, 8>::new().unwrap();
    let mut dev = DevFs::new();
    let mut second = DevFs::new();
    let mut vfs = Vfs::<3>::new(&mut fs).unwrap();
    vfs.create("/dev", Kind::Directory).unwrap();
    let hidden = vfs.create("/dev/hidden", Kind::File).unwrap();
    let nearby = vfs.create("/device", Kind::File).unwrap();
    vfs.mount("/dev", &mut dev).unwrap();
    assert_eq!(vfs.resolve("/dev/hidden"), Err(Error::NotFound));
    assert_eq!(vfs.resolve("/device"), Ok(nearby));
    assert_eq!(vfs.resolve("/dev/../device"), Ok(nearby));
    assert_eq!(vfs.metadata(hidden).unwrap().kind, Kind::File);
    assert_eq!(vfs.mount("/dev", &mut second), Err(Error::Busy));
    assert_eq!(vfs.remove("/dev"), Err(Error::Busy));
    assert_eq!(vfs.create("/dev/new", Kind::File), Err(Error::ReadOnly));
    assert_eq!(
        vfs.resolve("/dev/null")
            .and_then(|n| vfs.metadata(n))
            .unwrap()
            .kind,
        Kind::Device
    );
}

#[test]
fn nested_mount_dotdot_walks_back_to_namespace_parent() {
    let mut root = MemFs::<4, 4>::new().unwrap();
    let mut middle = MemFs::<4, 4>::new().unwrap();
    let mut dev = DevFs::new();
    let mut vfs = Vfs::<3>::new(&mut root).unwrap();
    vfs.create("/mnt", Kind::Directory).unwrap();
    vfs.mount("/mnt", &mut middle).unwrap();
    vfs.create("/mnt/dev", Kind::Directory).unwrap();
    vfs.mount("/mnt/dev", &mut dev).unwrap();
    assert_eq!(vfs.resolve("/mnt/dev/.."), vfs.resolve("/mnt"));
    assert_eq!(vfs.resolve("/mnt/dev/../.."), vfs.resolve("/"));
    assert_eq!(vfs.remove("/mnt/dev"), Err(Error::Busy));
}

#[test]
fn depth_limit_is_checked_on_actual_walk() {
    let mut fs = MemFs::<20, 1>::new().unwrap();
    let mut vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut path = std::string::String::new();
    for _ in 0..DEPTH_MAX {
        path.push_str("/a");
        vfs.create(&path, Kind::Directory).unwrap();
    }
    assert!(vfs.resolve(&path).is_ok());
    path.push_str("/a");
    assert_eq!(vfs.resolve(&path), Err(Error::NameTooLong));
    assert_eq!(vfs.create(&path, Kind::Directory), Err(Error::NameTooLong));
}

#[test]
fn failed_open_cannot_truncate_when_descriptors_are_full() {
    let mut fs = MemFs::<3, 8>::new().unwrap();
    let node = fs.create(fs.root(), "file", Kind::File).unwrap();
    fs.write(node, 0, b"keep").unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 1, 1, 4>::new(vfs);
    let fd = files.open("/file", Open::READ).unwrap();
    assert_eq!(files.open("/file", Open::REPLACE), Err(Error::NoSpace));
    assert_eq!(files.metadata("/file").unwrap().len, 4);
    assert_eq!(files.dup(fd), Err(Error::NoSpace));
    assert_eq!(files.pipe(), Err(Error::NoSpace));
    let mut data = [0; 4];
    files.read(fd, &mut data).unwrap();
    assert_eq!(&data, b"keep");
    files.close(fd).unwrap();
    assert_eq!(files.open("/file", Open::REPLACE), Ok(fd));
    assert_eq!(files.metadata("/file").unwrap().len, 0);
}

#[test]
fn access_offsets_append_and_close_are_enforced() {
    let mut fs = MemFs::<3, 16>::new().unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 8, 1, 4>::new(vfs);
    files.create("/file").unwrap();
    let append = Open {
        access: Access::Write,
        truncate: false,
        append: true,
    };
    let first = files.open("/file", append).unwrap();
    let second = files.open("/file", append).unwrap();
    files.write(first, b"ab").unwrap();
    files.write(second, b"cd").unwrap();
    files.seek(first, 0).unwrap();
    files.write(first, b"ef").unwrap();
    assert_eq!(files.read(first, &mut [0]), Err(Error::AccessDenied));
    let reader = files.open("/file", Open::READ).unwrap();
    let mut bytes = [0; 6];
    assert_eq!(files.read(reader, &mut bytes), Ok(6));
    assert_eq!(&bytes, b"abcdef");
    assert_eq!(files.write(reader, b"x"), Err(Error::AccessDenied));
    assert_eq!(files.remove("/file"), Err(Error::Busy));
    assert_eq!(
        files.open(
            "/file",
            Open {
                access: Access::Read,
                truncate: true,
                append: false
            }
        ),
        Err(Error::AccessDenied)
    );
    for fd in [first, second, reader] {
        files.close(fd).unwrap();
    }
    assert_eq!(files.close(reader), Err(Error::BadDescriptor));
    files.remove("/file").unwrap();
    assert_eq!(files.metadata("/file"), Err(Error::NotFound));
    assert_eq!(
        files.read(usize::MAX, &mut bytes),
        Err(Error::BadDescriptor)
    );
}

#[test]
fn duplicated_offsets_and_independent_opens_are_distinct() {
    let mut fs = MemFs::<3, 8>::new().unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 4, 0, 0>::new(vfs);
    files.create("/file").unwrap();
    let first = files.open("/file", Open::READ_WRITE).unwrap();
    files.write(first, b"abcd").unwrap();
    files.seek(first, 0).unwrap();
    let duplicate = files.dup(first).unwrap();
    let separate = files.open("/file", Open::READ).unwrap();
    let mut byte = [0];
    files.read(first, &mut byte).unwrap();
    assert_eq!(byte, [b'a']);
    files.read(duplicate, &mut byte).unwrap();
    assert_eq!(byte, [b'b']);
    files.close(first).unwrap();
    files.read(duplicate, &mut byte).unwrap();
    assert_eq!(byte, [b'c']);
    files.read(separate, &mut byte).unwrap();
    assert_eq!(byte, [b'a']);
}

#[test]
fn descriptor_rights_only_shrink_and_duplicates_inherit_restrictions() {
    let mut fs = MemFs::<3, 16>::new().unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 8, 1, 4>::new(vfs);
    files.create("/file").unwrap();
    let fd = files.open("/file", Open::READ_WRITE).unwrap();
    assert_eq!(
        files.rights(fd),
        Ok(files::RIGHT_READ | files::RIGHT_WRITE | files::RIGHT_SEEK)
    );

    files
        .restrict_rights(fd, files::RIGHT_READ | files::RIGHT_SEEK)
        .unwrap();
    assert_eq!(files.write(fd, b"x"), Err(Error::AccessDenied));
    assert_eq!(files.seek(fd, 0), Ok(()));

    let duplicate = files.dup(fd).unwrap();
    assert_eq!(
        files.rights(duplicate),
        Ok(files::RIGHT_READ | files::RIGHT_SEEK)
    );
    assert_eq!(
        files.restrict_rights(duplicate, files::RIGHTS_ALL),
        Err(Error::AccessDenied)
    );
    files.restrict_rights(duplicate, files::RIGHT_READ).unwrap();
    assert_eq!(files.seek(duplicate, 0), Err(Error::AccessDenied));

    let mut byte = [0u8; 1];
    assert_eq!(files.read(duplicate, &mut byte), Ok(0));
    files.close(duplicate).unwrap();
    assert_eq!(files.rights(duplicate), Err(Error::BadDescriptor));
}

#[test]
fn pipes_keep_ends_alive_until_last_duplicate_closes() {
    let mut fs = MemFs::<1, 0>::new().unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 8, 1, 4>::new(vfs);
    let (read, write) = files.pipe().unwrap();
    let read_dup = files.dup(read).unwrap();
    let write_dup = files.dup(write).unwrap();
    files.close(read).unwrap();
    files.close(write).unwrap();
    files.write(write_dup, b"data").unwrap();
    assert_eq!(files.pipe(), Err(Error::NoSpace));
    files.close(write_dup).unwrap();
    let mut bytes = [0; 4];
    assert_eq!(files.read(read_dup, &mut bytes), Ok(4));
    assert_eq!(&bytes, b"data");
    assert_eq!(files.read(read_dup, &mut bytes), Ok(0));
    files.close(read_dup).unwrap();
    let (read, write) = files.pipe().unwrap();
    assert_eq!(files.read(read, &mut bytes), Err(Error::WouldBlock));
    assert_eq!(files.seek(read, 0), Err(Error::NotSeekable));
    assert_eq!(files.write(read, b"x"), Err(Error::AccessDenied));
    assert_eq!(files.read(write, &mut bytes), Err(Error::AccessDenied));
}

#[test]
fn failed_pipe_creation_leaves_descriptor_slot_available() {
    let mut fs = MemFs::<2, 4>::new().unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 2, 1, 4>::new(vfs);
    files.create("/file").unwrap();
    let fd = files.open("/file", Open::READ).unwrap();
    assert_eq!(files.pipe(), Err(Error::NoSpace));
    assert_eq!(files.dup(fd), Ok(1));
}

#[test]
fn pipe_wraparound_full_and_oversized_writes_preserve_order() {
    let mut fs = MemFs::<1, 0>::new().unwrap();
    let vfs = Vfs::<1>::new(&mut fs).unwrap();
    let mut files = Files::<1, 4, 1, 4>::new(vfs);
    let (read, write) = files.pipe().unwrap();
    let mut bytes = [0; 4];
    for _ in 0..16 {
        assert_eq!(files.write(write, b"abc"), Ok(3));
        assert_eq!(files.write(write, b"ZZ"), Err(Error::WouldBlock));
        assert_eq!(files.write(write, b"ZZZZZ"), Err(Error::NoSpace));
        assert_eq!(files.read(read, &mut bytes[..2]), Ok(2));
        assert_eq!(&bytes[..2], b"ab");
        assert_eq!(files.write(write, b"de"), Ok(2));
        assert_eq!(files.read(read, &mut bytes), Ok(3));
        assert_eq!(&bytes[..3], b"cde");
    }
    files.close(read).unwrap();
    assert_eq!(files.write(write, b"x"), Err(Error::BrokenPipe));
    assert_eq!(files.write(write, b""), Ok(0));
}

#[test]
fn devfs_has_only_fixed_devices_and_no_seek_or_mutation() {
    let mut dev = DevFs::new();
    let null = dev.lookup(dev.root(), "null").unwrap();
    let zero = dev.lookup(dev.root(), "zero").unwrap();
    let mut buffer = [42; 32];
    assert_eq!(dev.read(null, 0, &mut buffer), Ok(0));
    assert_eq!(buffer, [42; 32]);
    assert_eq!(dev.read(zero, usize::MAX, &mut buffer), Ok(32));
    assert_eq!(buffer, [0; 32]);
    assert_eq!(dev.write(null, 0, b"ignored"), Ok(7));
    assert_eq!(dev.lookup(dev.root(), "disk"), Err(Error::NotFound));
    assert_eq!(dev.remove(dev.root(), "null"), Err(Error::ReadOnly));
    let tty = dev.lookup(dev.root(), "tty").unwrap();
    assert_eq!(dev.entry(dev.root(), 3), Ok(None));
    assert_eq!(dev.read(tty, 0, &mut buffer), Err(Error::WouldBlock));
    for byte in [b'a', b'b', 0x08, b'c', b'\n'] {
        dev.device_input(tty, byte).unwrap();
    }
    assert_eq!(dev.read(tty, 0, &mut buffer), Ok(3));
    assert_eq!(&buffer[..3], b"ac\n");
    assert_eq!(dev.write(tty, 0, b"out"), Ok(3));
    buffer.fill(0);
    assert_eq!(dev.device_output(tty, &mut buffer), Ok(3));
    assert_eq!(&buffer[..3], b"out");
    assert_eq!(dev.read(NodeId(999), 0, &mut buffer), Err(Error::StaleNode));
}
