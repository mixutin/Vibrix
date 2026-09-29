#![allow(dead_code)]

#[path = "../kernel/src/cpu_topology.rs"]
mod cpu_topology;
#[path = "../kernel/src/per_cpu.rs"]
mod per_cpu;

#[cfg(not(test))]
fn main() {}
