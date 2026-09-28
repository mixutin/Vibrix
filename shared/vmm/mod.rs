//! Bounded single-CPU supervisor VM, independent of firmware and transport.
//! Numeric addresses do not grant physical ownership or Rust references.
pub mod address;

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
    AlreadyMapped,
    NotMapped,
    CorruptEntry,
    ForeignTable,
    OutOfFrames,
    InvalidRoot,
}
