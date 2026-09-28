//! Host transport for the production mapper; no alternative mapping algorithm.
use super::frames::Frames;
use super::walk::{Memory, Vm};
use std::{boxed::Box, collections::BTreeMap, vec::Vec};

pub const ROOT: u64 = 0x1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Zero(u64),
    Write(u64, usize, u64),
    Invalidate(u64),
}

pub struct Ram {
    pub pages: BTreeMap<u64, Box<[u64; 512]>>,
    pub events: Vec<Event>,
}

impl Memory for Ram {
    fn read_entry(&mut self, frame: u64, index: usize) -> u64 {
        self.pages[&frame][index]
    }

    fn write_entry(&mut self, frame: u64, index: usize, value: u64) {
        self.pages.get_mut(&frame).unwrap()[index] = value;
        self.events.push(Event::Write(frame, index, value));
    }

    fn zero_frame(&mut self, frame: u64) {
        self.pages.get_mut(&frame).unwrap().fill(0);
        self.events.push(Event::Zero(frame));
    }

    fn invalidate(&mut self, page: u64) {
        self.events.push(Event::Invalidate(page));
    }
}

pub fn memory_and_frames<const N: usize>(count: usize) -> (Ram, Frames<N>) {
    let mut frames = Frames::<N>::new(48).unwrap();
    let mut memory = Ram {
        pages: BTreeMap::new(),
        events: Vec::new(),
    };
    memory.pages.insert(ROOT, Box::new([0; 512]));
    for index in 0..count {
        let physical = (index as u64 + 2) * 4096;
        frames.register(physical).unwrap();
        // Deliberately dirty supply proves that the mapper zeros before use.
        memory.pages.insert(physical, Box::new([u64::MAX; 512]));
    }
    (memory, frames)
}

pub fn vm<const N: usize>(count: usize) -> Vm<Ram, N> {
    let (memory, frames) = memory_and_frames(count);
    Vm::new(ROOT, memory, frames).unwrap()
}
