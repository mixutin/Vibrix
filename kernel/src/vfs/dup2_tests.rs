use super::{Files, Open};
use crate::vfs::{Error, Vfs, memfs::MemFs};

#[test]
fn full_table_replacement_shares_offsets_and_releases_old_description() {
    let mut ram = MemFs::<4, 32>::new().unwrap();
    let mut files = Files::<1, 2, 0, 8>::new(Vfs::new(&mut ram).unwrap());
    files.create("/source").unwrap();
    files.create("/target").unwrap();
    let source = files.open("/source", Open::READ_WRITE).unwrap();
    let target = files.open("/target", Open::READ_WRITE).unwrap();
    files.write(source, b"abc").unwrap();
    files.seek(source, 0).unwrap();
    assert_eq!(files.dup(source), Err(Error::NoSpace));
    assert_eq!(files.dup2(source, target), Ok(target));
    files.remove("/target").unwrap();
    let mut byte = [0];
    assert_eq!(files.read(source, &mut byte), Ok(1));
    assert_eq!(&byte, b"a");
    assert_eq!(files.read(target, &mut byte), Ok(1));
    assert_eq!(&byte, b"b");
    files.close(source).unwrap();
    assert_eq!(files.read(target, &mut byte), Ok(1));
    assert_eq!(&byte, b"c");
    assert_eq!(files.remove("/source"), Err(Error::Busy));
    files.close(target).unwrap();
    files.remove("/source").unwrap();
}

#[test]
fn invalid_source_or_target_preserves_both_descriptions() {
    let mut ram = MemFs::<4, 32>::new().unwrap();
    let mut files = Files::<1, 2, 0, 8>::new(Vfs::new(&mut ram).unwrap());
    files.create("/a").unwrap();
    files.create("/b").unwrap();
    let a = files.open("/a", Open::READ_WRITE).unwrap();
    let b = files.open("/b", Open::READ_WRITE).unwrap();
    files.write(a, b"A").unwrap();
    files.write(b, b"B").unwrap();
    files.seek(a, 0).unwrap();
    files.seek(b, 0).unwrap();
    for (source, target) in [(2, a), (usize::MAX, b), (a, 2), (b, usize::MAX), (2, 2)] {
        assert_eq!(files.dup2(source, target), Err(Error::BadDescriptor));
    }
    let mut byte = [0];
    assert_eq!(files.read(a, &mut byte), Ok(1));
    assert_eq!(&byte, b"A");
    assert_eq!(files.read(b, &mut byte), Ok(1));
    assert_eq!(&byte, b"B");
    files.close(b).unwrap();
    assert_eq!(files.dup2(b, a), Err(Error::BadDescriptor));
    files.seek(a, 0).unwrap();
    assert_eq!(files.read(a, &mut byte), Ok(1));
    assert_eq!(&byte, b"A");
}

#[test]
fn self_and_already_aliased_targets_do_not_leak_references() {
    let mut ram = MemFs::<2, 32>::new().unwrap();
    let mut files = Files::<1, 3, 0, 8>::new(Vfs::new(&mut ram).unwrap());
    files.create("/a").unwrap();
    let a = files.open("/a", Open::READ).unwrap();
    let b = files.dup(a).unwrap();
    for _ in 0..8 {
        assert_eq!(files.dup2(a, a), Ok(a));
        assert_eq!(files.dup2(a, b), Ok(b));
        assert_eq!(files.dup2(b, a), Ok(a));
    }
    files.close(a).unwrap();
    assert_eq!(files.remove("/a"), Err(Error::Busy));
    files.close(b).unwrap();
    files.remove("/a").unwrap();
}

#[test]
fn replacing_pipe_writer_delivers_eof_and_preserves_source_lifetime() {
    let mut ram = MemFs::<1, 8>::new().unwrap();
    let mut files = Files::<1, 4, 2, 8>::new(Vfs::new(&mut ram).unwrap());
    let (reader, writer) = files.pipe().unwrap();
    let (old_reader, target) = files.pipe().unwrap();
    assert_eq!(files.dup2(writer, target), Ok(target));
    let mut buffer = [0; 8];
    assert_eq!(files.read(old_reader, &mut buffer), Ok(0));
    files.close(old_reader).unwrap();
    assert_eq!(files.write(target, b"abc"), Ok(3));
    files.close(writer).unwrap();
    assert_eq!(files.read(reader, &mut buffer), Ok(3));
    assert_eq!(&buffer[..3], b"abc");
    assert_eq!(files.read(reader, &mut buffer), Err(Error::WouldBlock));
    files.close(target).unwrap();
    assert_eq!(files.read(reader, &mut buffer), Ok(0));
    files.close(reader).unwrap();
    assert!(files.pipe().is_ok());
    assert!(files.pipe().is_ok());
}

#[test]
fn replacing_last_pipe_reader_propagates_broken_pipe_and_reclaims_slot() {
    let mut ram = MemFs::<2, 8>::new().unwrap();
    let mut files = Files::<1, 3, 1, 8>::new(Vfs::new(&mut ram).unwrap());
    files.create("/a").unwrap();
    let file = files.open("/a", Open::READ).unwrap();
    let (reader, writer) = files.pipe().unwrap();
    assert_eq!(files.dup2(file, reader), Ok(reader));
    assert_eq!(files.write(writer, b"x"), Err(Error::BrokenPipe));
    files.close(writer).unwrap();
    files.close(reader).unwrap();
    assert!(files.pipe().is_ok());
}

#[test]
fn exact_free_target_does_not_consume_lower_holes() {
    let mut ram = MemFs::<2, 8>::new().unwrap();
    let mut files = Files::<1, 4, 0, 8>::new(Vfs::new(&mut ram).unwrap());
    files.create("/a").unwrap();
    let source = files.open("/a", Open::READ).unwrap();
    assert_eq!(source, 0);
    assert_eq!(files.dup2(source, 3), Ok(3));
    assert_eq!(files.dup(source), Ok(1));
    assert_eq!(files.write(3, b"x"), Err(Error::AccessDenied));
}

#[test]
fn zero_capacity_and_single_descriptor_boundaries() {
    let mut ram = MemFs::<2, 8>::new().unwrap();
    {
        let mut files = Files::<1, 0, 0, 8>::new(Vfs::new(&mut ram).unwrap());
        assert_eq!(files.dup2(0, 0), Err(Error::BadDescriptor));
    }
    let mut files = Files::<1, 1, 0, 8>::new(Vfs::new(&mut ram).unwrap());
    files.create("/a").unwrap();
    let source = files.open("/a", Open::READ).unwrap();
    assert_eq!(files.dup2(source, source), Ok(source));
    assert_eq!(files.dup2(source, 1), Err(Error::BadDescriptor));
    assert_eq!(files.dup(source), Err(Error::NoSpace));
    files.close(source).unwrap();
    files.remove("/a").unwrap();
}
