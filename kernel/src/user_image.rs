//! Bounded userspace ELF64 image planning and loading.
//!
//! Parsing is shared with the bootloader through `shared/elf.rs`. This layer
//! adds userspace policy: lower-half canonical addresses, null-page exclusion,
//! W^X, bounded segment count/footprint, full validation before sink mutation,
//! BSS zeroing, and sink abort on a backend failure.

#[path = "../../shared/elf.rs"]
mod elf;

pub const MAX_SEGMENTS: usize = 16;
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const USER_MIN: u64 = 0x1000;
pub const USER_END: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Permissions {
    pub write: bool,
    pub execute: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Segment {
    pub file_offset: usize,
    pub virtual_address: u64,
    pub file_size: usize,
    pub memory_size: usize,
    pub permissions: Permissions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Elf,
    TooManySegments,
    IntegerOverflow,
    UserAddress,
    WriteExecute,
    ImageTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Loaded {
    pub entry: u64,
    pub segments: usize,
    pub memory_bytes: usize,
}

pub struct Plan {
    entry: u64,
    segments: [Option<Segment>; MAX_SEGMENTS],
    count: usize,
    memory_bytes: usize,
}

impl Plan {
    pub const fn entry(&self) -> u64 {
        self.entry
    }

    pub const fn count(&self) -> usize {
        self.count
    }

    pub const fn memory_bytes(&self) -> usize {
        self.memory_bytes
    }

    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.segments[..self.count].iter().flatten().copied()
    }
}

pub trait Sink {
    type Error;

    fn begin(&mut self) -> Result<(), Self::Error>;
    fn map(
        &mut self,
        virtual_address: u64,
        memory_size: usize,
        permissions: Permissions,
    ) -> Result<(), Self::Error>;
    fn write(&mut self, virtual_address: u64, bytes: &[u8]) -> Result<(), Self::Error>;
    fn zero(&mut self, virtual_address: u64, bytes: usize) -> Result<(), Self::Error>;
    fn commit(&mut self, entry: u64) -> Result<(), Self::Error>;
    fn abort(&mut self);
}

#[derive(Debug, Eq, PartialEq)]
pub enum LoadError<E> {
    Image(Error),
    Sink(E),
}

pub fn plan(data: &[u8]) -> Result<Plan, Error> {
    let info = elf::validate(data).map_err(|_| Error::Elf)?;
    if !(USER_MIN..USER_END).contains(&info.entry) {
        return Err(Error::UserAddress);
    }

    let mut segments = [None; MAX_SEGMENTS];
    let mut count = 0usize;
    let mut memory_bytes = 0usize;

    for raw in info.load_segments(data) {
        let raw = raw.map_err(|_| Error::Elf)?;
        if raw.memory_size == 0 {
            continue;
        }
        if count == MAX_SEGMENTS {
            return Err(Error::TooManySegments);
        }

        let file_offset = usize::try_from(raw.file_offset).map_err(|_| Error::IntegerOverflow)?;
        let file_size = usize::try_from(raw.file_size).map_err(|_| Error::IntegerOverflow)?;
        let memory_size = usize::try_from(raw.memory_size).map_err(|_| Error::IntegerOverflow)?;
        let end = raw
            .virtual_address
            .checked_add(raw.memory_size)
            .ok_or(Error::IntegerOverflow)?;
        if raw.virtual_address < USER_MIN || end > USER_END || end <= raw.virtual_address {
            return Err(Error::UserAddress);
        }

        let permissions = Permissions {
            write: raw.writable(),
            execute: raw.executable(),
        };
        if permissions.write && permissions.execute {
            return Err(Error::WriteExecute);
        }

        memory_bytes = memory_bytes
            .checked_add(memory_size)
            .ok_or(Error::IntegerOverflow)?;
        if memory_bytes > MAX_IMAGE_BYTES {
            return Err(Error::ImageTooLarge);
        }

        segments[count] = Some(Segment {
            file_offset,
            virtual_address: raw.virtual_address,
            file_size,
            memory_size,
            permissions,
        });
        count += 1;
    }

    Ok(Plan {
        entry: info.entry,
        segments,
        count,
        memory_bytes,
    })
}

pub fn load<S: Sink>(data: &[u8], sink: &mut S) -> Result<Loaded, LoadError<S::Error>> {
    let plan = plan(data).map_err(LoadError::Image)?;
    sink.begin().map_err(LoadError::Sink)?;

    let result = (|| {
        for segment in plan.segments() {
            sink.map(
                segment.virtual_address,
                segment.memory_size,
                segment.permissions,
            )?;

            let file_end = segment
                .file_offset
                .checked_add(segment.file_size)
                .expect("validated ELF file range");
            let bytes = data
                .get(segment.file_offset..file_end)
                .expect("validated ELF file range");
            if !bytes.is_empty() {
                sink.write(segment.virtual_address, bytes)?;
            }

            let bss = segment.memory_size - segment.file_size;
            if bss != 0 {
                let start = segment
                    .virtual_address
                    .checked_add(segment.file_size as u64)
                    .expect("validated userspace range");
                sink.zero(start, bss)?;
            }
        }
        sink.commit(plan.entry())
    })();

    if let Err(error) = result {
        sink.abort();
        return Err(LoadError::Sink(error));
    }

    Ok(Loaded {
        entry: plan.entry(),
        segments: plan.count(),
        memory_bytes: plan.memory_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    const ELF_HEADER: usize = 64;
    const PROGRAM_HEADER: usize = 56;
    const PAYLOAD: usize = ELF_HEADER + PROGRAM_HEADER;

    fn image(flags: u32, vaddr: u64, file: &[u8], memory_size: u64) -> Vec<u8> {
        let mut data = vec![0u8; PAYLOAD + file.len()];
        data[0..4].copy_from_slice(b"\x7fELF");
        data[4] = 2;
        data[5] = 1;
        data[6] = 1;
        data[16..18].copy_from_slice(&2u16.to_le_bytes());
        data[18..20].copy_from_slice(&62u16.to_le_bytes());
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        data[24..32].copy_from_slice(&vaddr.to_le_bytes());
        data[32..40].copy_from_slice(&(ELF_HEADER as u64).to_le_bytes());
        data[52..54].copy_from_slice(&(ELF_HEADER as u16).to_le_bytes());
        data[54..56].copy_from_slice(&(PROGRAM_HEADER as u16).to_le_bytes());
        data[56..58].copy_from_slice(&1u16.to_le_bytes());

        let ph = ELF_HEADER;
        data[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());
        data[ph + 4..ph + 8].copy_from_slice(&flags.to_le_bytes());
        data[ph + 8..ph + 16].copy_from_slice(&(PAYLOAD as u64).to_le_bytes());
        data[ph + 16..ph + 24].copy_from_slice(&vaddr.to_le_bytes());
        data[ph + 32..ph + 40].copy_from_slice(&(file.len() as u64).to_le_bytes());
        data[ph + 40..ph + 48].copy_from_slice(&memory_size.to_le_bytes());
        data[ph + 48..ph + 56].copy_from_slice(&1u64.to_le_bytes());
        data[PAYLOAD..].copy_from_slice(file);
        data
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Op {
        Begin,
        Map(u64, usize, Permissions),
        Write(u64, Vec<u8>),
        Zero(u64, usize),
        Commit(u64),
        Abort,
    }

    #[derive(Default)]
    struct Mock {
        ops: Vec<Op>,
        fail_on: Option<usize>,
        calls: usize,
    }

    impl Mock {
        fn maybe_fail(&mut self) -> Result<(), &'static str> {
            self.calls += 1;
            if self.fail_on == Some(self.calls) {
                Err("injected")
            } else {
                Ok(())
            }
        }
    }

    impl Sink for Mock {
        type Error = &'static str;

        fn begin(&mut self) -> Result<(), Self::Error> {
            self.maybe_fail()?;
            self.ops.push(Op::Begin);
            Ok(())
        }

        fn map(
            &mut self,
            virtual_address: u64,
            memory_size: usize,
            permissions: Permissions,
        ) -> Result<(), Self::Error> {
            self.maybe_fail()?;
            self.ops
                .push(Op::Map(virtual_address, memory_size, permissions));
            Ok(())
        }

        fn write(&mut self, virtual_address: u64, bytes: &[u8]) -> Result<(), Self::Error> {
            self.maybe_fail()?;
            self.ops.push(Op::Write(virtual_address, bytes.to_vec()));
            Ok(())
        }

        fn zero(&mut self, virtual_address: u64, bytes: usize) -> Result<(), Self::Error> {
            self.maybe_fail()?;
            self.ops.push(Op::Zero(virtual_address, bytes));
            Ok(())
        }

        fn commit(&mut self, entry: u64) -> Result<(), Self::Error> {
            self.maybe_fail()?;
            self.ops.push(Op::Commit(entry));
            Ok(())
        }

        fn abort(&mut self) {
            self.ops.push(Op::Abort);
        }
    }

    #[test]
    fn plans_lower_half_rx_image() {
        let data = image(1, 0x40_0000, &[0x90, 0xc3], 4096);
        let plan = plan(&data).unwrap();
        assert_eq!(plan.entry(), 0x40_0000);
        assert_eq!(plan.count(), 1);
        assert_eq!(plan.memory_bytes(), 4096);
        assert_eq!(
            plan.segments().next().unwrap(),
            Segment {
                file_offset: PAYLOAD,
                virtual_address: 0x40_0000,
                file_size: 2,
                memory_size: 4096,
                permissions: Permissions {
                    write: false,
                    execute: true
                }
            }
        );
    }

    #[test]
    fn loader_maps_copies_zeroes_and_commits() {
        let data = image(1, 0x40_0000, &[1, 2, 3, 4], 8);
        let mut sink = Mock::default();
        let loaded = load(&data, &mut sink).unwrap();
        assert_eq!(
            loaded,
            Loaded {
                entry: 0x40_0000,
                segments: 1,
                memory_bytes: 8
            }
        );
        assert_eq!(
            sink.ops,
            vec![
                Op::Begin,
                Op::Map(
                    0x40_0000,
                    8,
                    Permissions {
                        write: false,
                        execute: true
                    }
                ),
                Op::Write(0x40_0000, vec![1, 2, 3, 4]),
                Op::Zero(0x40_0004, 4),
                Op::Commit(0x40_0000)
            ]
        );
    }

    #[test]
    fn rejects_null_upper_half_and_writable_executable_images_before_sink_mutation() {
        for data in [
            image(1, 0, &[0x90], 1),
            image(1, USER_END, &[0x90], 1),
            image(3, 0x40_0000, &[0x90], 1),
        ] {
            let mut sink = Mock::default();
            assert!(matches!(load(&data, &mut sink), Err(LoadError::Image(_))));
            assert!(sink.ops.is_empty());
        }
    }

    #[test]
    fn backend_failure_aborts_partial_load() {
        let data = image(1, 0x40_0000, &[1, 2, 3], 8);
        let mut sink = Mock {
            fail_on: Some(3),
            ..Mock::default()
        };
        assert_eq!(load(&data, &mut sink), Err(LoadError::Sink("injected")));
        assert_eq!(
            sink.ops,
            vec![
                Op::Begin,
                Op::Map(
                    0x40_0000,
                    8,
                    Permissions {
                        write: false,
                        execute: true
                    }
                ),
                Op::Abort
            ]
        );
    }

    #[test]
    fn rejects_oversized_image_footprint() {
        let data = image(1, 0x40_0000, &[0x90], (MAX_IMAGE_BYTES as u64) + 1);
        assert_eq!(plan(&data).unwrap_err(), Error::ImageTooLarge);
    }
}
