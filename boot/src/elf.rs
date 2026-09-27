//! Clean-room ELF64 parser for the Vibrix bootloader.
//!
//! Implements just enough of the ELF64 specification to validate and inspect
//! PT_LOAD segments of a Vibrix kernel image. No external crates.
//!
//! Primary reference: System V ABI, ELF64 object file format.

/// ELF64 magic number: 0x7F, 'E', 'L', 'F'.
const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];

/// 64-bit ELF class.
const ELFCLASS64: u8 = 2;

/// Little-endian data encoding.
const ELFDATA2LSB: u8 = 1;

/// Current ELF version.
const EV_CURRENT: u32 = 1;

/// Executable file type (static ET_EXEC; ET_DYN also accepted for PIE kernels).
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;

/// AMD x86-64 machine type.
const EM_X86_64: u16 = 62;

/// Program header type: loadable segment.
pub const PT_LOAD: u32 = 1;

/// Maximum number of PT_LOAD segments the loader will track.
pub const MAX_LOAD_SEGMENTS: usize = 16;

/// ELF64 file header (64 bytes total).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Elf64Header {
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

/// ELF64 program header (56 bytes total).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// Errors returned when validating an ELF64 image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// Input is shorter than the ELF header.
    TooShortForHeader,
    /// Input is shorter than the declared program header table.
    TooShortForPhdrs,
    /// Magic number mismatch.
    BadMagic,
    /// Not a 64-bit image.
    BadClass,
    /// Not little-endian.
    BadEndian,
    /// Unsupported ELF version.
    BadVersion,
    /// Unsupported file type (not ET_EXEC or ET_DYN).
    BadType,
    /// Unsupported machine architecture.
    BadMachine,
    /// Program header entry size is wrong for ELF64.
    BadPhdrEntrySize,
    /// Program header count exceeds the input length.
    PhdrOverflow,
    /// More PT_LOAD segments than MAX_LOAD_SEGMENTS.
    TooManyLoadSegments,
}

impl core::fmt::Display for ElfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ElfError::TooShortForHeader => write!(f, "input shorter than ELF header"),
            ElfError::TooShortForPhdrs => write!(f, "input shorter than program header table"),
            ElfError::BadMagic => write!(f, "bad ELF magic"),
            ElfError::BadClass => write!(f, "not a 64-bit ELF"),
            ElfError::BadEndian => write!(f, "not little-endian ELF"),
            ElfError::BadVersion => write!(f, "unsupported ELF version"),
            ElfError::BadType => write!(f, "unsupported ELF file type"),
            ElfError::BadMachine => write!(f, "unsupported machine architecture"),
            ElfError::BadPhdrEntrySize => write!(f, "program header entry size mismatch"),
            ElfError::PhdrOverflow => write!(f, "program header table extends past input"),
            ElfError::TooManyLoadSegments => write!(f, "too many PT_LOAD segments"),
        }
    }
}

/// Read a `u16` from a byte slice (little-endian).
fn u16_from_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

/// Read a `u32` from a byte slice (little-endian).
fn u32_from_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/// Read a `u64` from a byte slice (little-endian).
fn u64_from_le(buf: &[u8], off: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&buf[off..off + 8]);
    u64::from_le_bytes(bytes)
}

/// Parsed and validated ELF64 image.
///
/// Owns a fixed-capacity array of PT_LOAD segments so the loader can use
/// this in a `no_std` context without heap allocation.
#[derive(Debug, PartialEq)]
pub struct Elf64Image {
    pub header: Elf64Header,
    /// PT_LOAD program headers, in file order.
    load_segments: [Elf64Phdr; MAX_LOAD_SEGMENTS],
    /// Number of valid entries in `load_segments`.
    load_segment_count: usize,
}

impl Elf64Image {
    /// Validate and parse an ELF64 image from raw bytes.
    ///
    /// Checks magic, class, endianness, version, type, and machine. Extracts
    /// the PT_LOAD program headers. Does not validate segment overlap or
    /// virtual address layout -- that is the loader's concern.
    pub fn parse(file_data: &[u8]) -> Result<Self, ElfError> {
        if file_data.len() < core::mem::size_of::<Elf64Header>() {
            return Err(ElfError::TooShortForHeader);
        }

        // --- e_ident checks (first 16 bytes) ---
        if file_data[0..4] != ELFMAG {
            return Err(ElfError::BadMagic);
        }
        if file_data[4] != ELFCLASS64 {
            return Err(ElfError::BadClass);
        }
        if file_data[5] != ELFDATA2LSB {
            return Err(ElfError::BadEndian);
        }

        let e_version = u32_from_le(file_data, 20);
        if e_version != EV_CURRENT {
            return Err(ElfError::BadVersion);
        }

        let e_type = u16_from_le(file_data, 16);
        if e_type != ET_EXEC && e_type != ET_DYN {
            return Err(ElfError::BadType);
        }

        let e_machine = u16_from_le(file_data, 18);
        if e_machine != EM_X86_64 {
            return Err(ElfError::BadMachine);
        }

        let e_phentsize = u16_from_le(file_data, 54);
        let e_phnum = u16_from_le(file_data, 56);

        // ELF64 program headers are exactly 56 bytes.
        if e_phentsize != core::mem::size_of::<Elf64Phdr>() as u16 {
            return Err(ElfError::BadPhdrEntrySize);
        }

        // Ensure the full header table fits inside the image.
        let phoff = u64_from_le(file_data, 32) as usize;
        let phnum = e_phnum as usize;
        let ph_table_len = phnum
            .checked_mul(e_phentsize as usize)
            .ok_or(ElfError::PhdrOverflow)?;
        let ph_end = phoff
            .checked_add(ph_table_len)
            .ok_or(ElfError::PhdrOverflow)?;
        if ph_end > file_data.len() {
            return Err(ElfError::TooShortForPhdrs);
        }

        let header = Elf64Header {
            e_type,
            e_machine,
            e_version,
            e_entry: u64_from_le(file_data, 24),
            e_phoff: phoff as u64,
            e_shoff: u64_from_le(file_data, 40),
            e_flags: u32_from_le(file_data, 48),
            e_ehsize: u16_from_le(file_data, 52),
            e_phentsize,
            e_phnum,
            e_shentsize: u16_from_le(file_data, 58),
            e_shnum: u16_from_le(file_data, 60),
            e_shstrndx: u16_from_le(file_data, 62),
        };

        // Collect PT_LOAD segments.
        let mut load_segments = [Elf64Phdr {
            p_type: 0,
            p_flags: 0,
            p_offset: 0,
            p_vaddr: 0,
            p_paddr: 0,
            p_filesz: 0,
            p_memsz: 0,
            p_align: 0,
        }; MAX_LOAD_SEGMENTS];
        let mut load_segment_count: usize = 0;

        for i in 0..phnum {
            let base = phoff + i * e_phentsize as usize;
            let p_type = u32_from_le(file_data, base);

            if p_type == PT_LOAD {
                if load_segment_count >= MAX_LOAD_SEGMENTS {
                    return Err(ElfError::TooManyLoadSegments);
                }
                load_segments[load_segment_count] = Elf64Phdr {
                    p_type,
                    p_flags: u32_from_le(file_data, base + 4),
                    p_offset: u64_from_le(file_data, base + 8),
                    p_vaddr: u64_from_le(file_data, base + 16),
                    p_paddr: u64_from_le(file_data, base + 24),
                    p_filesz: u64_from_le(file_data, base + 32),
                    p_memsz: u64_from_le(file_data, base + 40),
                    p_align: u64_from_le(file_data, base + 48),
                };
                load_segment_count += 1;
            }
        }

        Ok(Elf64Image {
            header,
            load_segments,
            load_segment_count,
        })
    }

    /// Return the PT_LOAD segments as a slice.
    pub fn load_segments(&self) -> &[Elf64Phdr] {
        &self.load_segments[..self.load_segment_count]
    }

    /// Return the entry point virtual address.
    pub fn entry_point(&self) -> u64 {
        self.header.e_entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid ELF64 header for testing.
    fn make_elf_header(phoff: u64, phnum: u16, phentsize: u16) -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0..4].copy_from_slice(&ELFMAG);
        buf[4] = ELFCLASS64;
        buf[5] = ELFDATA2LSB;
        buf[6] = 1; // EI_VERSION
        // e_ident[7..16] left as zero (EI_OSABI etc.)
        buf[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        buf[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        buf[20..24].copy_from_slice(&EV_CURRENT.to_le_bytes());
        buf[24..32].copy_from_slice(&0xFFFFFFFF8000_0000u64.to_le_bytes()); // entry
        buf[32..40].copy_from_slice(&phoff.to_le_bytes());
        buf[40..48].copy_from_slice(&0u64.to_le_bytes()); // shoff
        buf[48..52].copy_from_slice(&0u32.to_le_bytes()); // flags
        buf[52..54].copy_from_slice(&64u16.to_le_bytes()); // ehsize
        buf[54..56].copy_from_slice(&phentsize.to_le_bytes()); // phentsize
        buf[56..58].copy_from_slice(&phnum.to_le_bytes()); // phnum
        buf[58..60].copy_from_slice(&0u16.to_le_bytes()); // shentsize
        buf[60..62].copy_from_slice(&0u16.to_le_bytes()); // shnum
        buf[62..64].copy_from_slice(&0u16.to_le_bytes()); // shstrndx
        buf
    }

    /// Build a single PT_LOAD program header.
    fn make_pt_load(offset: u64, vaddr: u64, filesz: u64, memsz: u64) -> [u8; 56] {
        let mut buf = [0u8; 56];
        buf[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        buf[4..8].copy_from_slice(&7u32.to_le_bytes()); // flags: RWX
        buf[8..16].copy_from_slice(&offset.to_le_bytes());
        buf[16..24].copy_from_slice(&vaddr.to_le_bytes());
        buf[24..32].copy_from_slice(&vaddr.to_le_bytes()); // paddr = vaddr
        buf[32..40].copy_from_slice(&filesz.to_le_bytes());
        buf[40..48].copy_from_slice(&memsz.to_le_bytes());
        buf[48..56].copy_from_slice(&4096u64.to_le_bytes()); // align
        buf
    }

    /// Concatenate header + program headers into one buffer.
    fn assemble_elf(header: &[u8; 64], phdrs: &[[u8; 56]]) -> Vec<u8> {
        let mut img = Vec::with_capacity(64 + 56 * phdrs.len());
        img.extend_from_slice(header);
        for phdr in phdrs {
            img.extend_from_slice(phdr);
        }
        img
    }

    #[test]
    fn test_valid_minimal_elf() {
        let header = make_elf_header(64, 1, 56);
        let phdr = make_pt_load(0x1000, 0xFFFFFFFF8000_0000, 0x800, 0x1000);
        let img = assemble_elf(&header, &[phdr]);

        let elf = Elf64Image::parse(&img).expect("valid ELF should parse");
        assert_eq!(elf.entry_point(), 0xFFFFFFFF8000_0000);
        let segs = elf.load_segments();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].p_type, PT_LOAD);
        assert_eq!(segs[0].p_vaddr, 0xFFFFFFFF8000_0000);
        assert_eq!(segs[0].p_filesz, 0x800);
        assert_eq!(segs[0].p_memsz, 0x1000);
    }

    #[test]
    fn test_bad_magic() {
        let mut header = make_elf_header(64, 0, 56);
        header[0] = 0x00;
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadMagic));
    }

    #[test]
    fn test_bad_class() {
        let mut header = make_elf_header(64, 0, 56);
        header[4] = 1; // ELFCLASS32
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadClass));
    }

    #[test]
    fn test_bad_endian() {
        let mut header = make_elf_header(64, 0, 56);
        header[5] = 2; // ELFDATA2MSB
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadEndian));
    }

    #[test]
    fn test_bad_version() {
        let mut header = make_elf_header(64, 0, 56);
        header[20..24].copy_from_slice(&0u32.to_le_bytes());
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadVersion));
    }

    #[test]
    fn test_bad_type() {
        let mut header = make_elf_header(64, 0, 56);
        header[16..18].copy_from_slice(&1u16.to_le_bytes()); // ET_REL
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadType));
    }

    #[test]
    fn test_bad_machine() {
        let mut header = make_elf_header(64, 0, 56);
        header[18..20].copy_from_slice(&0u16.to_le_bytes()); // EM_NONE
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadMachine));
    }

    #[test]
    fn test_bad_phentsize() {
        let header = make_elf_header(64, 0, 32); // wrong phentsize
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::BadPhdrEntrySize));
    }

    #[test]
    fn test_phdr_overflow() {
        let header = make_elf_header(64, 1000, 56); // phnum too large
        let img = assemble_elf(&header, &[]);
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::TooShortForPhdrs));
    }

    #[test]
    fn test_too_short_for_header() {
        let img = [0u8; 32];
        assert_eq!(Elf64Image::parse(&img), Err(ElfError::TooShortForHeader));
    }

    #[test]
    fn test_multiple_load_segments() {
        let header = make_elf_header(64, 3, 56);
        let phdr1 = make_pt_load(0x1000, 0xFFFFFFFF8000_0000, 0x800, 0x800);
        let phdr2 = make_pt_load(0x2000, 0xFFFFFFFF8000_1000, 0x400, 0x400);
        let phdr3 = make_pt_load(0x3000, 0xFFFFFFFF8000_2000, 0x200, 0x200);
        let img = assemble_elf(&header, &[phdr1, phdr2, phdr3]);

        let elf = Elf64Image::parse(&img).expect("valid ELF should parse");
        let segs = elf.load_segments();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].p_vaddr, 0xFFFFFFFF8000_0000);
        assert_eq!(segs[1].p_vaddr, 0xFFFFFFFF8000_1000);
        assert_eq!(segs[2].p_vaddr, 0xFFFFFFFF8000_2000);
    }

    #[test]
    fn test_non_load_segments_skipped() {
        let header = make_elf_header(64, 2, 56);
        let phdr_load = make_pt_load(0x1000, 0xFFFFFFFF8000_0000, 0x800, 0x800);
        let mut phdr_note = [0u8; 56];
        phdr_note[0..4].copy_from_slice(&4u32.to_le_bytes()); // PT_NOTE
        let img = assemble_elf(&header, &[phdr_load, phdr_note]);

        let elf = Elf64Image::parse(&img).expect("valid ELF should parse");
        let segs = elf.load_segments();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].p_type, PT_LOAD);
    }

    #[test]
    fn test_zero_phnum() {
        let header = make_elf_header(64, 0, 56);
        let img = assemble_elf(&header, &[]);

        let elf = Elf64Image::parse(&img).expect("valid ELF should parse");
        assert_eq!(elf.load_segments().len(), 0);
    }

    #[test]
    fn test_max_load_segments_boundary() {
        let phnum = MAX_LOAD_SEGMENTS as u16;
        let header = make_elf_header(64, phnum, 56);
        let phdrs: Vec<[u8; 56]> = (0..MAX_LOAD_SEGMENTS)
            .map(|i| {
                make_pt_load(
                    0x1000 + (i as u64) * 0x1000,
                    0xFFFFFFFF8000_0000 + (i as u64) * 0x1000,
                    0x100,
                    0x100,
                )
            })
            .collect();
        let img = assemble_elf(&header, &phdrs);

        let elf = Elf64Image::parse(&img).expect("valid ELF should parse");
        assert_eq!(elf.load_segments().len(), MAX_LOAD_SEGMENTS);
    }

    #[test]
    fn test_too_many_load_segments() {
        let phnum = (MAX_LOAD_SEGMENTS + 1) as u16;
        let header = make_elf_header(64, phnum, 56);
        let phdrs: Vec<[u8; 56]> = (0..MAX_LOAD_SEGMENTS + 1)
            .map(|i| {
                make_pt_load(
                    0x1000 + (i as u64) * 0x1000,
                    0xFFFFFFFF8000_0000 + (i as u64) * 0x1000,
                    0x100,
                    0x100,
                )
            })
            .collect();
        let img = assemble_elf(&header, &phdrs);

        assert_eq!(Elf64Image::parse(&img), Err(ElfError::TooManyLoadSegments));
    }

    #[test]
    fn test_dyn_type_accepted() {
        let mut header = make_elf_header(64, 0, 56);
        header[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
        let img = assemble_elf(&header, &[]);
        assert!(Elf64Image::parse(&img).is_ok());
    }
}
