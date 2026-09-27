//! Production-linked preflight tests; no privileged transition is invoked.
#![allow(dead_code)]
#[path = "../boot/src/uefi.rs"]
mod uefi;
#[path = "../boot/src/transition.rs"]
mod transition;
