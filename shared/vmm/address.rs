//! The managed arena never overlaps the loader, kernel image or scratch window.
use super::Error;

pub const PAGE_BYTES: u64 = 4096;
pub const ARENA_BASE: u64 = 0xffff_d000_0000_0000;
pub const ARENA_BYTES: u64 = 1 << 39;
pub const ARENA_SLOT: usize = 416;
pub const USER_SLOT: usize = 0;
pub const USER_MIN: u64 = 0x1000;
pub const USER_SLOT_END: u64 = 1 << 39;
pub const MAX_RANGE_PAGES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page(u64);

impl Page {
    /// Construct one page in the kernel-owned managed arena.
    pub fn new(address: u64) -> Result<Self, Error> {
        if !address.is_multiple_of(PAGE_BYTES)
            || !(ARENA_BASE..ARENA_BASE + ARENA_BYTES).contains(&address)
        {
            return Err(Error::InvalidAddress);
        }
        Ok(Self(address))
    }

    /// Construct one lower-half userspace page in PML4 slot zero.
    ///
    /// This separate constructor deliberately does not widen `Page::new`;
    /// kernel managed-VM callers therefore keep their original arena boundary.
    pub fn new_user(address: u64) -> Result<Self, Error> {
        if !address.is_multiple_of(PAGE_BYTES) || !(USER_MIN..USER_SLOT_END).contains(&address) {
            return Err(Error::InvalidAddress);
        }
        Ok(Self(address))
    }

    pub const fn address(self) -> u64 {
        self.0
    }

    pub(crate) fn checked_add(self, bytes: u64) -> Result<Self, Error> {
        let address = self.0.checked_add(bytes).ok_or(Error::InvalidRange)?;
        if (ARENA_BASE..ARENA_BASE + ARENA_BYTES).contains(&self.0) {
            Self::new(address).map_err(|_| Error::InvalidRange)
        } else if (USER_MIN..USER_SLOT_END).contains(&self.0) {
            Self::new_user(address).map_err(|_| Error::InvalidRange)
        } else {
            Err(Error::InvalidRange)
        }
    }

    pub fn indices(self) -> [usize; 4] {
        [39, 30, 21, 12].map(|shift| ((self.0 >> shift) & 511) as usize)
    }
}

/// A validated number, not an allocation or ownership token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalFrame(u64);

impl PhysicalFrame {
    pub fn new(address: u64, physical_bits: u8) -> Result<Self, Error> {
        if !(36..=52).contains(&physical_bits) {
            return Err(Error::InvalidWidth);
        }
        if address == 0 || !address.is_multiple_of(PAGE_BYTES) || address >= (1u64 << physical_bits)
        {
            return Err(Error::InvalidFrame);
        }
        Ok(Self(address))
    }

    pub const fn address(self) -> u64 {
        self.0
    }
}

/// Writable/executable is deliberately unrepresentable. Cache policy remains
/// write-back; privilege is represented separately so adding a user mapping
/// never weakens the address-range/ownership contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permissions {
    ReadOnly,
    ReadWrite,
    ReadExecute,
}

impl Permissions {
    pub const fn writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }

    pub const fn executable(self) -> bool {
        matches!(self, Self::ReadExecute)
    }
}

/// CPU privilege permitted to traverse a mapping. This does not change the
/// owned virtual arena: user pages remain inside the same explicitly owned
/// PML4 slot until M5 introduces independent address spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privilege {
    Supervisor,
    User,
}

impl Privilege {
    pub const fn user(self) -> bool {
        matches!(self, Self::User)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRange {
    start: Page,
    count: usize,
}

impl PageRange {
    pub fn new(start: Page, count: usize) -> Result<Self, Error> {
        if count == 0 || count > MAX_RANGE_PAGES {
            return Err(Error::InvalidRange);
        }
        start.checked_add((count as u64 - 1) * PAGE_BYTES)?;
        Ok(Self { start, count })
    }

    pub const fn count(self) -> usize {
        self.count
    }

    pub fn page(self, index: usize) -> Option<Page> {
        (index < self.count).then(|| Page(self.start.address() + index as u64 * PAGE_BYTES))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_excludes_scratch_kernel_userspace_and_canonical_hole() {
        assert_eq!(Page::new(ARENA_BASE).unwrap().indices(), [416, 0, 0, 0]);
        assert_eq!(
            Page::new(ARENA_BASE + ARENA_BYTES - PAGE_BYTES)
                .unwrap()
                .indices(),
            [416, 511, 511, 511]
        );
        for address in [
            0,
            0x8000_0000_0000,
            0xffff_c000_0000_0000,
            0xffff_ffff_8000_0000,
            ARENA_BASE - PAGE_BYTES,
            ARENA_BASE + ARENA_BYTES,
            ARENA_BASE + 1,
        ] {
            assert_eq!(Page::new(address), Err(Error::InvalidAddress));
        }
    }

    #[test]
    fn lower_user_constructor_is_separate_and_slot_zero_bounded() {
        assert!(Page::new(0x4000_0000).is_err());
        assert_eq!(Page::new_user(0x4000_0000).unwrap().indices()[0], USER_SLOT);
        for address in [0, PAGE_BYTES - 1, USER_SLOT_END, USER_SLOT_END + PAGE_BYTES] {
            assert_eq!(Page::new_user(address), Err(Error::InvalidAddress));
        }
        let last = Page::new_user(USER_SLOT_END - PAGE_BYTES).unwrap();
        assert_eq!(PageRange::new(last, 2), Err(Error::InvalidRange));
    }

    #[test]
    fn physical_width_alignment_and_zero_are_checked() {
        for bits in [36, 48, 52] {
            assert!(PhysicalFrame::new((1u64 << bits) - PAGE_BYTES, bits).is_ok());
            assert_eq!(
                PhysicalFrame::new(1u64 << bits, bits),
                Err(Error::InvalidFrame)
            );
        }
        for address in [0, 1, 4095, 4097, u64::MAX] {
            assert_eq!(PhysicalFrame::new(address, 48), Err(Error::InvalidFrame));
        }
        for bits in [0, 35, 53, 64, 255] {
            assert_eq!(PhysicalFrame::new(4096, bits), Err(Error::InvalidWidth));
        }
    }

    #[test]
    fn range_is_bounded_and_never_crosses_arena_end() {
        let start = Page::new(ARENA_BASE).unwrap();
        assert_eq!(PageRange::new(start, 0), Err(Error::InvalidRange));
        assert_eq!(PageRange::new(start, 65), Err(Error::InvalidRange));
        let range = PageRange::new(start, 64).unwrap();
        assert_eq!(range.count(), 64);
        assert_eq!(range.page(63).unwrap().address(), ARENA_BASE + 63 * 4096);
        assert_eq!(range.page(64), None);
        let last = Page::new(ARENA_BASE + ARENA_BYTES - PAGE_BYTES).unwrap();
        assert!(PageRange::new(last, 1).is_ok());
        assert_eq!(PageRange::new(last, 2), Err(Error::InvalidRange));
    }

    #[test]
    fn privilege_is_explicit_and_does_not_change_arena_validation() {
        assert!(!Privilege::Supervisor.user());
        assert!(Privilege::User.user());
        assert!(Page::new(ARENA_BASE).is_ok());
        assert_eq!(Page::new(0x4000_0000), Err(Error::InvalidAddress));
    }

    #[test]
    fn permission_states_never_allow_write_and_execute() {
        for permission in [
            Permissions::ReadOnly,
            Permissions::ReadWrite,
            Permissions::ReadExecute,
        ] {
            assert!(!(permission.writable() && permission.executable()));
        }
    }
}
