//! x86-64 page-fault diagnostics.
//!
//! Clean-room implementation from the Intel 64 and IA-32 Architectures
//! Software Developer's Manual, Volume 3, Chapter 4 (Paging) and
//! Chapter 6 (Interrupt and Exception Handling).

use core::arch::asm;

pub mod error_code {
    pub const P: u64 = 1 << 0;
    pub const WR: u64 = 1 << 1;
    pub const US: u64 = 1 << 2;
    pub const RSVD: u64 = 1 << 3;
    pub const ID: u64 = 1 << 4;
    pub const PK: u64 = 1 << 5;
    pub const SS: u64 = 1 << 6;
    pub const SGX: u64 = 1 << 7;
}

pub fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) value, options(nomem, nostack));
    }
    value
}

pub struct PageFaultError {
    pub not_present: bool,
    pub write: bool,
    pub user: bool,
    pub reserved_bit: bool,
    pub instruction_fetch: bool,
    pub protection_key: bool,
    pub shadow_stack: bool,
    pub sgx: bool,
}

impl PageFaultError {
    pub fn decode(code: u64) -> Self {
        Self {
            not_present: code & error_code::P != 0,
            write: code & error_code::WR != 0,
            user: code & error_code::US != 0,
            reserved_bit: code & error_code::RSVD != 0,
            instruction_fetch: code & error_code::ID != 0,
            protection_key: code & error_code::PK != 0,
            shadow_stack: code & error_code::SS != 0,
            sgx: code & error_code::SGX != 0,
        }
    }

    pub fn description(&self) -> &'static str {
        if self.not_present {
            if self.user {
                "user-mode read from non-present page"
            } else if self.write {
                "supervisor write to non-present page"
            } else {
                "supervisor read from non-present page"
            }
        } else {
            if self.user {
                "user-mode protection violation"
            } else if self.write {
                "supervisor write protection violation"
            } else {
                "supervisor read protection violation"
            }
        }
    }
}

impl core::fmt::Display for PageFaultError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.description())?;
        if self.instruction_fetch {
            write!(f, " (instruction fetch)")?;
        }
        if self.reserved_bit {
            write!(f, " (reserved bit)")?;
        }
        if self.protection_key {
            write!(f, " (protection key)")?;
        }
        if self.shadow_stack {
            write!(f, " (shadow stack)")?;
        }
        if self.sgx {
            write!(f, " (SGX)")?;
        }
        Ok(())
    }
}

pub fn handle_page_fault(error_code: u64, faulting_address: u64) {
    let err = PageFaultError::decode(error_code);

    crate::println!("PAGE FAULT at {:#018x}", faulting_address);
    crate::println!("  Error code: {:#06x}", error_code);
    crate::println!("  Type: {}", err.description());
    crate::println!("  P={} W/R={} U/S={} I/D={}",
        err.not_present as u8, err.write as u8, err.user as u8, err.instruction_fetch as u8);

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_not_present() {
        let err = PageFaultError::decode(0);
        assert!(err.not_present);
        assert!(!err.write);
        assert!(!err.user);
        assert!(!err.instruction_fetch);
    }

    #[test]
    fn test_decode_write_user() {
        let err = PageFaultError::decode(error_code::WR | error_code::US);
        assert!(!err.not_present);
        assert!(err.write);
        assert!(err.user);
    }

    #[test]
    fn test_decode_instruction_fetch() {
        let err = PageFaultError::decode(error_code::ID);
        assert!(err.instruction_fetch);
    }

    #[test]
    fn test_decode_reserved_bit() {
        let err = PageFaultError::decode(error_code::RSVD);
        assert!(err.reserved_bit);
    }

    #[test]
    fn test_decode_protection_key() {
        let err = PageFaultError::decode(error_code::PK);
        assert!(err.protection_key);
    }

    #[test]
    fn test_decode_shadow_stack() {
        let err = PageFaultError::decode(error_code::SS);
        assert!(err.shadow_stack);
    }

    #[test]
    fn test_decode_sgx() {
        let err = PageFaultError::decode(error_code::SGX);
        assert!(err.sgx);
    }

    #[test]
    fn test_description_not_present() {
        let err = PageFaultError::decode(0);
        assert!(err.description().contains("non-present"));
    }

    #[test]
    fn test_description_protection_violation() {
        let err = PageFaultError::decode(error_code::WR);
        assert!(err.description().contains("protection violation"));
    }
}
