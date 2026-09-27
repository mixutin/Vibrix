const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;

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
    EntryNotExecutable,
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
            Self::EntryNotExecutable => {
                "ELF error: entry outside file-backed executable PT_LOAD\r\n"
            },
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
    let mut executable_entry = false;
    for index in 0..phnum as usize {
        let base = phoff + index * PROGRAM_HEADER_SIZE;
        let p_type = read_u32(data, base)?;
        if p_type != PT_LOAD {
            continue;
        }

        let p_flags = read_u32(data, base + 4)?;
        let p_offset = read_u64(data, base + 8)?;
        let p_vaddr = read_u64(data, base + 16)?;
        let p_filesz = read_u64(data, base + 32)?;
        let p_memsz = read_u64(data, base + 40)?;
        let p_align = read_u64(data, base + 48)?;

        if p_filesz > p_memsz {
            return Err(ElfError::InvalidLoadSegment);
        }
        // The complete memory image must have a representable half-open virtual range.
        // Do not allow a malicious PT_LOAD to wrap when segments are mapped later.
        if p_vaddr.checked_add(p_memsz).is_none() {
            return Err(ElfError::InvalidLoadSegment);
        }
        let file_virtual_end = p_vaddr
            .checked_add(p_filesz)
            .ok_or(ElfError::InvalidLoadSegment)?;

        let file_start = usize::try_from(p_offset).map_err(|_| ElfError::InvalidLoadSegment)?;
        let file_len = usize::try_from(p_filesz).map_err(|_| ElfError::InvalidLoadSegment)?;
        let file_end = file_start
            .checked_add(file_len)
            .ok_or(ElfError::InvalidLoadSegment)?;
        if file_end > data.len() {
            return Err(ElfError::InvalidLoadSegment);
        }

        if p_align > 1
            && (!p_align.is_power_of_two() || (p_vaddr % p_align) != (p_offset % p_align))
        {
            return Err(ElfError::InvalidLoadSegment);
        }

        // A future handoff must never jump to a non-executable or BSS-only address.
        if p_flags & PF_X != 0 && (p_vaddr..file_virtual_end).contains(&entry) {
            executable_entry = true;
        }

        load_segments = load_segments
            .checked_add(1)
            .ok_or(ElfError::InvalidLoadSegment)?;
    }

    if load_segments == 0 {
        return Err(ElfError::NoLoadSegments);
    }
    if !executable_entry {
        return Err(ElfError::EntryNotExecutable);
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

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD_OFFSET: usize = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;

    fn valid_elf() -> Vec<u8> {
        let mut data = vec![0u8; PAYLOAD_OFFSET + 4];
        data[0..4].copy_from_slice(b"\x7fELF");
        data[4] = ELFCLASS64;
        data[5] = ELFDATA2LSB;
        data[6] = EV_CURRENT;
        data[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        data[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        data[20..24].copy_from_slice(&(EV_CURRENT as u32).to_le_bytes());
        data[24..32].copy_from_slice(&0x1000u64.to_le_bytes());
        data[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
        data[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
        data[54..56].copy_from_slice(&(PROGRAM_HEADER_SIZE as u16).to_le_bytes());
        data[56..58].copy_from_slice(&1u16.to_le_bytes());

        let ph = ELF_HEADER_SIZE;
        data[ph..ph + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        data[ph + 4..ph + 8].copy_from_slice(&PF_X.to_le_bytes());
        data[ph + 8..ph + 16].copy_from_slice(&(PAYLOAD_OFFSET as u64).to_le_bytes());
        data[ph + 16..ph + 24].copy_from_slice(&0x1000u64.to_le_bytes());
        data[ph + 32..ph + 40].copy_from_slice(&4u64.to_le_bytes());
        data[ph + 40..ph + 48].copy_from_slice(&8u64.to_le_bytes());
        data[ph + 48..ph + 56].copy_from_slice(&1u64.to_le_bytes());
        data[PAYLOAD_OFFSET..].copy_from_slice(&[1, 2, 3, 4]);
        data
    }

    #[test]
    fn accepts_valid_elf64_with_bss() {
        let info = validate(&valid_elf()).unwrap_or_else(|_| panic!("valid fixture rejected"));
        assert_eq!(info.entry, 0x1000);
        assert_eq!(info.program_headers, 1);
        assert_eq!(info.load_segments, 1);
    }

    #[test]
    fn rejects_truncated_elf_header() {
        assert!(matches!(validate(&[0u8; 63]), Err(ElfError::Truncated)));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = valid_elf();
        data[0] = 0;
        assert!(matches!(validate(&data), Err(ElfError::BadMagic)));
    }

    #[test]
    fn rejects_wrong_class_and_endianness() {
        let mut data = valid_elf();
        data[4] = 1;
        assert!(matches!(validate(&data), Err(ElfError::WrongClass)));
        data[4] = ELFCLASS64;
        data[5] = 2;
        assert!(matches!(validate(&data), Err(ElfError::WrongEndian)));
    }

    #[test]
    fn rejects_wrong_machine() {
        let mut data = valid_elf();
        data[18..20].copy_from_slice(&3u16.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::WrongMachine)));
    }

    #[test]
    fn rejects_wrong_program_header_size() {
        let mut data = valid_elf();
        data[54..56].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            validate(&data),
            Err(ElfError::BadProgramHeaderSize)
        ));
    }

    #[test]
    fn rejects_program_header_table_outside_file() {
        let mut data = valid_elf();
        let outside_table_offset = (data.len() - 55) as u64;
        data[32..40].copy_from_slice(&outside_table_offset.to_le_bytes());
        assert!(matches!(
            validate(&data),
            Err(ElfError::ProgramHeaderTableOutOfBounds)
        ));
    }

    #[test]
    fn rejects_program_header_table_offset_overflow() {
        let mut data = valid_elf();
        data[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            validate(&data),
            Err(ElfError::ProgramHeaderTableOutOfBounds)
        ));
    }

    #[test]
    fn rejects_missing_load_segments() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE..ELF_HEADER_SIZE + 4].copy_from_slice(&4u32.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::NoLoadSegments)));
    }

    #[test]
    fn rejects_file_image_larger_than_memory_image() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 40..ELF_HEADER_SIZE + 48].copy_from_slice(&3u64.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::InvalidLoadSegment)));
    }

    #[test]
    fn rejects_segment_file_range_outside_image() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 32..ELF_HEADER_SIZE + 40].copy_from_slice(&5u64.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::InvalidLoadSegment)));
    }

    #[test]
    fn rejects_segment_file_offset_overflow() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 8..ELF_HEADER_SIZE + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::InvalidLoadSegment)));
    }

    #[test]
    fn rejects_non_power_of_two_alignment() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 48..ELF_HEADER_SIZE + 56].copy_from_slice(&3u64.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::InvalidLoadSegment)));
    }

    #[test]
    fn rejects_incongruent_file_and_memory_alignment() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 48..ELF_HEADER_SIZE + 56].copy_from_slice(&4096u64.to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::InvalidLoadSegment)));
    }

    #[test]
    fn accepts_entry_inside_file_backed_executable_code() {
        let mut data = valid_elf();
        data[24..32].copy_from_slice(&0x1003u64.to_le_bytes());
        assert!(validate(&data).is_ok());
    }

    #[test]
    fn rejects_entry_outside_load_segments() {
        let mut data = valid_elf();
        data[24..32].copy_from_slice(&0x2000u64.to_le_bytes());
        assert!(matches!(
            validate(&data),
            Err(ElfError::EntryNotExecutable)
        ));
    }

    #[test]
    fn rejects_entry_in_bss_even_if_segment_is_executable() {
        let mut data = valid_elf();
        data[24..32].copy_from_slice(&0x1004u64.to_le_bytes());
        assert!(matches!(
            validate(&data),
            Err(ElfError::EntryNotExecutable)
        ));
    }

    #[test]
    fn rejects_entry_without_execute_permission() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 4..ELF_HEADER_SIZE + 8]
            .copy_from_slice(&4u32.to_le_bytes());
        assert!(matches!(
            validate(&data),
            Err(ElfError::EntryNotExecutable)
        ));
    }

    #[test]
    fn rejects_wrapping_virtual_segment_range() {
        let mut data = valid_elf();
        data[ELF_HEADER_SIZE + 16..ELF_HEADER_SIZE + 24]
            .copy_from_slice(&(u64::MAX - 3).to_le_bytes());
        assert!(matches!(validate(&data), Err(ElfError::InvalidLoadSegment)));
    }
}
