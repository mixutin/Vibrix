#![no_std]

pub mod fetch;
pub mod manual;
pub mod parser;
pub mod path;
pub mod runtime;
pub mod system;
pub mod text;

pub use manual::Builtin;
pub use parser::{Command, LINE_BYTES, MAX_ARGS, ParseError};
pub use runtime::Shell;

#[cfg(test)]
mod tests;
