//! BootInfo v3's reserved early mapping window (ADR 0009).
pub const BASE: u64 = 0xffff_c000_0000_0000;
pub const PAGES: usize = 512;
pub const PAGE_BYTES: u64 = 4096;
