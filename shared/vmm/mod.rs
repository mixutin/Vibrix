//! Bounded single-CPU supervisor VM, independent of firmware and transport.
//! Numeric addresses do not grant physical ownership or Rust references.
pub mod address;
pub mod frames;
mod guarded;
mod map;
mod protect;
mod region;
mod unmap;
pub mod walk;

pub use guarded::{GuardedId, GuardedLayout, GuardedVm};
pub use walk::{Memory, Translation, Vm};

#[cfg(test)]
pub(crate) mod test_support;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidAddress,
    InvalidFrame,
    InvalidWidth,
    InvalidRange,
    Capacity,
    DuplicateFrame,
    UnknownFrame,
    DoubleFree,
    WrongFrameUse,
    AlreadyMapped,
    NotMapped,
    CorruptEntry,
    ForeignTable,
    OutOfFrames,
    InvalidRoot,
    AddressInUse,
    InvalidAllocation,
    ActiveAllocations,
}
