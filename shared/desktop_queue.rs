//! Allocation-free FIFO for input captured while the BSP repaints the display.
pub struct Queue<T: Copy, const N: usize> {
    items: [Option<T>; N],
    head: usize,
    len: usize,
}
impl<T: Copy, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        Self {
            items: [None; N],
            head: 0,
            len: 0,
        }
    }
    pub fn full(&self) -> bool {
        self.len == N
    }
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.full() {
            return Err(value);
        }
        self.items[(self.head + self.len) % N] = Some(value);
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let value = self.items[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        value
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_queue_never_overwrites_clicks_or_keystrokes() {
        let mut queue = Queue::<u32, 3>::new();
        for value in 0..3 {
            assert_eq!(queue.push(value), Ok(()));
        }
        assert_eq!(queue.push(99), Err(99));
        assert_eq!(queue.pop(), Some(0));
        assert_eq!(queue.push(3), Ok(()));
        for value in 1..4 {
            assert_eq!(queue.pop(), Some(value));
        }
        assert_eq!(queue.pop(), None);
    }
    #[test]
    fn repeated_wrap_and_zero_capacity_remain_bounded() {
        let mut queue = Queue::<u32, 2>::new();
        for value in 0..1000 {
            queue.push(value).unwrap();
            assert_eq!(queue.pop(), Some(value));
        }
        let mut empty = Queue::<u32, 0>::new();
        assert_eq!(empty.push(1), Err(1));
        assert_eq!(empty.pop(), None);
    }
}
