//! Firmware-independent x86-64 loader/kernel boot ABI (ADRs 0001 and 0004).
//!
//! No UEFI pointers or host-OS types appear in this wire format. Physical
//! addresses remain integers: validating metadata does not map their backing.

pub const BOOTINFO_MAGIC: u64 = 0x4942_5849_5242_4956; // "VIBRIXBI" in little endian
pub const BOOTINFO_VERSION: u32 = 2;
pub const SUPPORTED_MEMORY_DESCRIPTOR_VERSION: u32 = 1;
pub const MEMORY_DESCRIPTOR_PREFIX_BYTES: u64 = 40;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub _reserved: u32,
    pub framebuffer_base: u64,
    pub framebuffer_size: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_stride: u32,
    pub framebuffer_format: u32,
    pub rsdp: u64,
    pub memory_map: u64,
    pub memory_map_len: u64,
    pub memory_descriptor_size: u64,
    pub memory_descriptor_version: u32,
    pub _reserved_v2: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramebufferInfo {
    pub physical_base: u64,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_format: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinalMemoryMap {
    pub physical_base: u64,
    pub byte_len: usize,
    pub descriptor_size: usize,
    pub descriptor_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootInfoError {
    InvalidMagic,
    UnsupportedVersion,
    NonzeroReserved,
    InvalidFramebuffer,
    InvalidRsdp,
    InvalidMemoryMap,
    UnsupportedDescriptorVersion,
}

impl BootInfo {
    /// Construct from the *same successful final* GetMemoryMap result.
    /// The firmware map key stays loader-local and is never part of BootInfo.
    pub fn new(
        framebuffer: FramebufferInfo,
        rsdp: u64,
        map: FinalMemoryMap,
    ) -> Result<Self, BootInfoError> {
        let byte_len = u64::try_from(map.byte_len).map_err(|_| BootInfoError::InvalidMemoryMap)?;
        let descriptor_size =
            u64::try_from(map.descriptor_size).map_err(|_| BootInfoError::InvalidMemoryMap)?;
        let info = Self {
            magic: BOOTINFO_MAGIC,
            version: BOOTINFO_VERSION,
            _reserved: 0,
            framebuffer_base: framebuffer.physical_base,
            framebuffer_size: framebuffer.size_bytes,
            framebuffer_width: framebuffer.width,
            framebuffer_height: framebuffer.height,
            framebuffer_stride: framebuffer.stride,
            framebuffer_format: framebuffer.pixel_format,
            rsdp,
            memory_map: map.physical_base,
            memory_map_len: byte_len,
            memory_descriptor_size: descriptor_size,
            memory_descriptor_version: map.descriptor_version,
            _reserved_v2: 0,
        };
        info.validate()?;
        Ok(info)
    }

    /// Validate scalar metadata only. The caller must separately prove that
    /// this whole 88-byte object and its physical ranges are mapped/owned.
    /// In particular, this does not dereference the memory-map address.
    pub fn validate(&self) -> Result<(), BootInfoError> {
        if self.magic != BOOTINFO_MAGIC {
            return Err(BootInfoError::InvalidMagic);
        }
        // Version gate precedes *all* access to the v2 tail in kernel entry.
        // A future pointer-based reader must first check the mapped v1 prefix.
        if self.version != BOOTINFO_VERSION {
            return Err(BootInfoError::UnsupportedVersion);
        }
        if self._reserved != 0 || self._reserved_v2 != 0 {
            return Err(BootInfoError::NonzeroReserved);
        }
        let min_framebuffer_bytes = u64::from(self.framebuffer_stride)
            .checked_mul(u64::from(self.framebuffer_height))
            .and_then(|pixels| pixels.checked_mul(4));
        if self.framebuffer_base == 0
            || self.framebuffer_size == 0
            || self.framebuffer_width == 0
            || self.framebuffer_height == 0
            || self.framebuffer_stride < self.framebuffer_width
            || self.framebuffer_format > 2
            || min_framebuffer_bytes.is_none_or(|needed| needed > self.framebuffer_size)
            || self
                .framebuffer_base
                .checked_add(self.framebuffer_size)
                .is_none()
        {
            return Err(BootInfoError::InvalidFramebuffer);
        }
        if self.rsdp == 0 {
            return Err(BootInfoError::InvalidRsdp);
        }
        if self.memory_descriptor_version != SUPPORTED_MEMORY_DESCRIPTOR_VERSION {
            return Err(BootInfoError::UnsupportedDescriptorVersion);
        }
        if self.memory_map == 0
            || self.memory_map_len == 0
            || self.memory_descriptor_size < MEMORY_DESCRIPTOR_PREFIX_BYTES
            || !self.memory_descriptor_size.is_multiple_of(8)
            || !self.memory_map_len.is_multiple_of(self.memory_descriptor_size)
            || self.memory_map.checked_add(self.memory_map_len).is_none()
        {
            return Err(BootInfoError::InvalidMemoryMap);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    fn valid() -> BootInfo {
        BootInfo::new(
            FramebufferInfo {
                physical_base: 0xe000_0000,
                size_bytes: 1024 * 768 * 4,
                width: 1024,
                height: 768,
                stride: 1024,
                pixel_format: 1,
            },
            0x1000,
            FinalMemoryMap {
                physical_base: 0x2000,
                byte_len: 48 * 3,
                descriptor_size: 48,
                descriptor_version: 1,
            },
        )
        .unwrap()
    }

    #[test]
    fn abi_is_exactly_88_bytes_with_stable_v1_prefix() {
        assert_eq!(BOOTINFO_MAGIC.to_le_bytes(), *b"VIBRIXBI");
        assert_eq!(size_of::<BootInfo>(), 88);
        assert_eq!(align_of::<BootInfo>(), 8);
        assert_eq!(offset_of!(BootInfo, magic), 0);
        assert_eq!(offset_of!(BootInfo, version), 8);
        assert_eq!(offset_of!(BootInfo, _reserved), 12);
        assert_eq!(offset_of!(BootInfo, framebuffer_base), 16);
        assert_eq!(offset_of!(BootInfo, framebuffer_size), 24);
        assert_eq!(offset_of!(BootInfo, framebuffer_width), 32);
        assert_eq!(offset_of!(BootInfo, framebuffer_height), 36);
        assert_eq!(offset_of!(BootInfo, framebuffer_stride), 40);
        assert_eq!(offset_of!(BootInfo, framebuffer_format), 44);
        assert_eq!(offset_of!(BootInfo, rsdp), 48);
        assert_eq!(offset_of!(BootInfo, memory_map), 56);
        assert_eq!(offset_of!(BootInfo, memory_map_len), 64);
        assert_eq!(offset_of!(BootInfo, memory_descriptor_size), 72);
        assert_eq!(offset_of!(BootInfo, memory_descriptor_version), 80);
        assert_eq!(offset_of!(BootInfo, _reserved_v2), 84);
    }

    #[test]
    fn final_map_is_written_without_map_key_and_with_zero_reserved_fields() {
        let info = valid();
        assert_eq!(info.magic, BOOTINFO_MAGIC);
        assert_eq!(info.version, 2);
        assert_eq!(info._reserved, 0);
        assert_eq!(info._reserved_v2, 0);
        assert_eq!(info.memory_map_len, 144);
        assert_eq!(info.memory_descriptor_size, 48);
        assert_eq!(info.memory_descriptor_version, 1);
        assert_eq!(info.validate(), Ok(()));
    }

    #[test]
    fn corrupt_magic_version_and_reserved_fields_fail_closed() {
        for value in [0, 1, 3] {
            let mut info = valid();
            info.version = value;
            assert_eq!(info.validate(), Err(BootInfoError::UnsupportedVersion));
        }
        let mut info = valid();
        info.magic = 0;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMagic));
        let mut info = valid();
        info._reserved = 1;
        assert_eq!(info.validate(), Err(BootInfoError::NonzeroReserved));
        let mut info = valid();
        info._reserved_v2 = 1;
        assert_eq!(info.validate(), Err(BootInfoError::NonzeroReserved));
    }

    #[test]
    fn unsupported_firmware_descriptor_versions_are_rejected() {
        for version in [0, 2, u32::MAX] {
            let mut info = valid();
            info.memory_descriptor_version = version;
            assert_eq!(
                info.validate(),
                Err(BootInfoError::UnsupportedDescriptorVersion)
            );
        }
    }

    #[test]
    fn map_stride_length_and_overflow_fail_closed() {
        let mut info = valid();
        info.memory_descriptor_size = 0;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryMap));
        let mut info = valid();
        info.memory_descriptor_size = 32;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryMap));
        let mut info = valid();
        info.memory_descriptor_size = 41;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryMap));
        let mut info = valid();
        info.memory_map_len = 49;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryMap));
        let mut info = valid();
        info.memory_map = u64::MAX - 1;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryMap));
        let mut info = valid();
        info.memory_map = 0;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryMap));
    }

    #[test]
    fn framebuffer_overflow_and_invalid_rsdp_fail_closed() {
        let mut info = valid();
        info.framebuffer_size = 1;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidFramebuffer));
        let mut info = valid();
        info.framebuffer_base = u64::MAX;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidFramebuffer));
        let mut info = valid();
        info.rsdp = 0;
        assert_eq!(info.validate(), Err(BootInfoError::InvalidRsdp));
    }
}
