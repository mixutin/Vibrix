const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;

pub struct ElfInfo {
    pub entry: u64,
    pub program_headers: u16,
    pub load_segments: u16,
}

#[derive(Clone, Copy)]
pub enum ElfError {
    Truncated,
    BadMagic,
    WrongClass,
    WrongEndian,
    WrongVersion,
    WrongType,
    WrongMachine,
    BadHeaderSize,
    BadProgramHeaderSize,
    MissingProgramHeaders,
    ProgramHeaderTableOutOfBounds,
    InvalidLoadSegment,
    NoLoadSegments,
}

impl ElfError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Truncated => "ELF error: truncated file\r\n",
            Self::BadMagic => "ELF error: bad magic\r\n",
            Self::WrongClass => "ELF error: not ELF64\r\n",
            Self::WrongEndian => "ELF error: not little-endian\r\n",
            Self::WrongVersion => "ELF error: unsupported version\r\n",
            Self::WrongType => "ELF error: kernel is not ET_EXEC\r\n",
            Self::WrongMachine => "ELF error: kernel is not x86_64\r\n",
            Self::BadHeaderSize => "ELF error: bad header size\r\n",
            Self::BadProgramHeaderSize => "ELF error: bad program header size\r\n",
            Self::MissingProgramHeaders => "ELF error: no program headers\r\n",
            Self::ProgramHeaderTableOutOfBounds => {
                "ELF error: program header table out of bounds\r\n"
            }
            Self::InvalidLoadSegment => "ELF error: invalid PT_LOAD segment\r\n",
            Self::NoLoadSegments => "ELF error: no PT_LOAD segments\r\n",
        }
    }
}

pub fn validate(data: &[u8]) -> Result<ElfInfo, ElfError> {
    if data.len() < ELF_HEADER_SIZE {
        return Err(ElfError::Truncated);
    }
    if &data[0..4] != b"\x7fELF" {
        return Err(ElfError::BadMagic);
    }
    if data[4] != ELFCLASS64 {
        return Err(ElfError::WrongClass);
    }
    if data[5] != ELFDATA2LSB {
        return Err(ElfError::WrongEndian);
    }
    if data[6] != EV_CURRENT {
        return Err(ElfError::WrongVersion);
    }

    let elf_type = read_u16(data, 16)?;
    let machine = read_u16(data, 18)?;
    let version = read_u32(data, 20)?;
    let entry = read_u64(data, 24)?;
    let phoff = read_u64(data, 32)?;
    let ehsize = read_u16(data, 52)?;
    let phentsize = read_u16(data, 54)?;
    let phnum = read_u16(data, 56)?;

    if elf_type != ET_EXEC {
        return Err(ElfError::WrongType);
    }
    if machine != EM_X86_64 {
        return Err(ElfError::WrongMachine);
    }
    if version != EV_CURRENT as u32 {
        return Err(ElfError::WrongVersion);
    }
    if ehsize as usize != ELF_HEADER_SIZE {
        return Err(ElfError::BadHeaderSize);
    }
    if phentsize as usize != PROGRAM_HEADER_SIZE {
        return Err(ElfError::BadProgramHeaderSize);
    }
    if phnum == 0 {
        return Err(ElfError::MissingProgramHeaders);
    }

    let phoff = usize::try_from(phoff).map_err(|_| ElfError::ProgramHeaderTableOutOfBounds)?;
    let table_size = (phnum as usize)
        .checked_mul(PROGRAM_HEADER_SIZE)
        .ok_or(ElfError::ProgramHeaderTableOutOfBounds)?;
    let table_end = phoff
        .checked_add(table_size)
        .ok_or(ElfError::ProgramHeaderTableOutOfBounds)?;
    if table_end > data.len() {
        return Err(ElfError::ProgramHeaderTableOutOfBounds);
    }

    let mut load_segments = 0u16;
    for index in 0..phnum as usize {
        let base = phoff + index * PROGRAM_HEADER_SIZE;
        let p_type = read_u32(data, base)?;
        if p_type != PT_LOAD {
            continue;
        }

        let p_offset = read_u64(data, base + 8)?;
        let p_vaddr = read_u64(data, base + 16)?;
        let p_filesz = read_u64(data, base + 32)?;
        let p_memsz = read_u64(data, base + 40)?;
        let p_align = read_u64(data, base + 48)?;

        if p_filesz > p_memsz {
            return Err(ElfError::InvalidLoadSegment);
        }

        let file_start = usize::try_from(p_offset).map_err(|_| ElfError::InvalidLoadSegment)?;
        let file_len = usize::try_from(p_filesz).map_err(|_| ElfError::InvalidLoadSegment)?;
        let file_end = file_start
            .checked_add(file_len)
            .ok_or(ElfError::InvalidLoadSegment)?;
        if file_end > data.len() {
            return Err(ElfError::InvalidLoadSegment);
        }

        if p_align > 1 {
            if !p_align.is_power_of_two() || (p_vaddr % p_align) != (p_offset % p_align) {
                return Err(ElfError::InvalidLoadSegment);
            }
        }

        load_segments = load_segments
            .checked_add(1)
            .ok_or(ElfError::InvalidLoadSegment)?;
    }

    if load_segments == 0 {
        return Err(ElfError::NoLoadSegments);
    }

    Ok(ElfInfo {
        entry,
        program_headers: phnum,
        load_segments,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, ElfError> {
    let bytes = data.get(offset..offset + 2).ok_or(ElfError::Truncated)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, ElfError> {
    let bytes = data.get(offset..offset + 4).ok_or(ElfError::Truncated)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, ElfError> {
    let bytes = data.get(offset..offset + 8).ok_or(ElfError::Truncated)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}
