//! Fixed-capacity single-BSP cooperative kernel threads.
//!
//! This is not a preemptive scheduler. All switches are explicit while IF=0,
//! every thread uses the same CR3/kernel address space, and stacks are static
//! kernel BSS. No user mode, TLS, FPU ownership, SMP or blocking primitives.

use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::ptr::{addr_of_mut, write_bytes};
#[cfg(feature = "qemu-debugcon")]
use core::sync::atomic::{AtomicU8, Ordering};

pub const MAX_THREADS: usize = 4;
pub const STACK_BYTES: usize = 16 * 1024;
const BOOT_CONTEXT: usize = usize::MAX;
const SAVED_WORDS: usize = 8;
const SWITCH_WORDS: usize = 7;

global_asm!(
    r#"
    .section .text
    .global vibrix_context_switch
    .type vibrix_context_switch,@function
vibrix_context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
    .size vibrix_context_switch, .-vibrix_context_switch
"#
);

unsafe extern "C" {
    fn vibrix_context_switch(old_rsp: *mut usize, new_rsp: usize);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadId(usize);

impl ThreadId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Empty,
    Ready,
    Running,
    Exited,
}

#[derive(Clone, Copy)]
struct Thread {
    rsp: usize,
    entry: Option<fn()>,
    state: State,
}

impl Thread {
    const EMPTY: Self = Self {
        rsp: 0,
        entry: None,
        state: State::Empty,
    };
}

struct Scheduler {
    threads: [Thread; MAX_THREADS],
    boot_rsp: usize,
    current: usize,
    switches: u64,
    completions: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            threads: [Thread::EMPTY; MAX_THREADS],
            boot_rsp: 0,
            current: BOOT_CONTEXT,
            switches: 0,
            completions: 0,
        }
    }
}

struct SchedulerCell(UnsafeCell<Scheduler>);

// SAFETY: every public mutation requires one BSP with IF=0. SMP/IRQ use is a
// documented unsupported state and is rejected before touching the scheduler.
unsafe impl Sync for SchedulerCell {}

static SCHEDULER: SchedulerCell = SchedulerCell(UnsafeCell::new(Scheduler::new()));

#[repr(align(16))]
#[derive(Clone, Copy)]
struct Stack([u8; STACK_BYTES]);

struct StackCell(UnsafeCell<[Stack; MAX_THREADS]>);

// SAFETY: same sole-BSP/IF=0 ownership as SCHEDULER. A stack belongs to exactly
// one Ready/Running thread and is only recycled after that thread is Exited.
unsafe impl Sync for StackCell {}

static STACKS: StackCell = StackCell(UnsafeCell::new([Stack([0; STACK_BYTES]); MAX_THREADS]));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InterruptsEnabled,
    WrongContext,
    Capacity,
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunStats {
    pub switches: u64,
    pub completed: usize,
}

fn interrupts_disabled() -> bool {
    let flags: u64;
    // SAFETY: pushfq/pop reads the current BSP flags and restores RSP.
    unsafe { asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags)) };
    flags & (1 << 9) == 0
}

fn initial_rsp(stack_end: usize) -> usize {
    // Six saved callee-saved registers + synthetic RIP + one alignment word.
    // The context switch consumes 7 words, leaving RSP = end - 8 at the new
    // function entry, matching the SysV x86-64 requirement RSP % 16 == 8.
    stack_end - SAVED_WORDS * core::mem::size_of::<usize>()
}

unsafe fn prepare_stack(index: usize) -> usize {
    let stacks = STACKS.0.get();
    // SAFETY: caller owns this non-running slot on the sole BSP.
    let start = unsafe { addr_of_mut!((*stacks)[index].0).cast::<u8>() };
    // Clear prior thread contents before reusing the stack.
    unsafe { write_bytes(start, 0, STACK_BYTES) };
    let end = start as usize + STACK_BYTES;
    let rsp = initial_rsp(end);
    debug_assert_eq!(rsp & 0xf, 0);
    let frame = rsp as *mut usize;
    // Words 0..5 are r15,r14,r13,r12,rbx,rbp after switch restore.
    // Word 6 is consumed by RET. Word 7 remains alignment padding.
    unsafe {
        frame
            .add(SWITCH_WORDS - 1)
            .write(thread_bootstrap as *const () as usize)
    };
    rsp
}

fn first_ready(threads: &[Thread; MAX_THREADS]) -> Option<usize> {
    threads
        .iter()
        .position(|thread| thread.state == State::Ready)
}

fn next_ready(threads: &[Thread; MAX_THREADS], after: usize) -> Option<usize> {
    for step in 1..MAX_THREADS {
        let index = (after + step) % MAX_THREADS;
        if threads[index].state == State::Ready {
            return Some(index);
        }
    }
    None
}

unsafe fn switch_to(next: usize) {
    let scheduler = SCHEDULER.0.get();
    // SAFETY: sole BSP, IF=0, and no Rust reference to scheduler survives the
    // actual stack switch. Raw field pointers stay in static kernel storage.
    let current = unsafe { (*scheduler).current };
    let old_rsp = if current == BOOT_CONTEXT {
        unsafe { addr_of_mut!((*scheduler).boot_rsp) }
    } else {
        unsafe { addr_of_mut!((*scheduler).threads[current].rsp) }
    };
    let new_rsp = if next == BOOT_CONTEXT {
        unsafe { (*scheduler).boot_rsp }
    } else {
        unsafe {
            (*scheduler).threads[next].state = State::Running;
            (*scheduler).threads[next].rsp
        }
    };
    unsafe {
        (*scheduler).current = next;
        (*scheduler).switches = (*scheduler).switches.saturating_add(1);
    }
    // SAFETY: old_rsp is permanent scheduler storage. new_rsp is either the
    // previously saved boot/thread stack or a synthetic ABI-correct frame.
    unsafe { vibrix_context_switch(old_rsp, new_rsp) };
}

pub fn spawn(entry: fn()) -> Result<ThreadId, Error> {
    if !interrupts_disabled() {
        return Err(Error::InterruptsEnabled);
    }
    let scheduler = SCHEDULER.0.get();
    if unsafe { (*scheduler).current } != BOOT_CONTEXT {
        return Err(Error::WrongContext);
    }
    let index = unsafe {
        (*scheduler)
            .threads
            .iter()
            .position(|thread| matches!(thread.state, State::Empty | State::Exited))
    }
    .ok_or(Error::Capacity)?;
    let rsp = unsafe { prepare_stack(index) };
    unsafe {
        (*scheduler).threads[index] = Thread {
            rsp,
            entry: Some(entry),
            state: State::Ready,
        };
    }
    Ok(ThreadId(index))
}

pub fn yield_now() -> Result<(), Error> {
    if !interrupts_disabled() {
        return Err(Error::InterruptsEnabled);
    }
    let scheduler = SCHEDULER.0.get();
    let current = unsafe { (*scheduler).current };
    if current == BOOT_CONTEXT || current >= MAX_THREADS {
        return Err(Error::WrongContext);
    }
    unsafe { (*scheduler).threads[current].state = State::Ready };
    let next = unsafe { next_ready(&(*scheduler).threads, current) }.unwrap_or(BOOT_CONTEXT);
    unsafe { switch_to(next) };
    Ok(())
}

pub fn run() -> Result<RunStats, Error> {
    if !interrupts_disabled() {
        return Err(Error::InterruptsEnabled);
    }
    let scheduler = SCHEDULER.0.get();
    if unsafe { (*scheduler).current } != BOOT_CONTEXT {
        return Err(Error::WrongContext);
    }
    let switches_before = unsafe { (*scheduler).switches };
    let completions_before = unsafe { (*scheduler).completions };
    loop {
        let next = unsafe { first_ready(&(*scheduler).threads) };
        let Some(next) = next else {
            break;
        };
        unsafe { switch_to(next) };
        if unsafe { (*scheduler).current } != BOOT_CONTEXT {
            return Err(Error::Invariant);
        }
    }
    Ok(RunStats {
        switches: unsafe { (*scheduler).switches }.saturating_sub(switches_before),
        completed: usize::try_from(
            unsafe { (*scheduler).completions }.saturating_sub(completions_before),
        )
        .unwrap_or(usize::MAX),
    })
}

unsafe fn finish_current() -> ! {
    let scheduler = SCHEDULER.0.get();
    let current = unsafe { (*scheduler).current };
    if current == BOOT_CONTEXT || current >= MAX_THREADS {
        panic!("kernel thread exited outside a thread context");
    }
    unsafe {
        (*scheduler).threads[current].state = State::Exited;
        (*scheduler).threads[current].entry = None;
        (*scheduler).completions = (*scheduler).completions.saturating_add(1);
    }
    let next = unsafe { next_ready(&(*scheduler).threads, current) }.unwrap_or(BOOT_CONTEXT);
    unsafe { switch_to(next) };
    panic!("exited kernel thread was resumed");
}

extern "C" fn thread_bootstrap() -> ! {
    let scheduler = SCHEDULER.0.get();
    let current = unsafe { (*scheduler).current };
    if current >= MAX_THREADS {
        panic!("kernel thread bootstrap has invalid current id");
    }
    let entry = unsafe { (*scheduler).threads[current].entry }
        .unwrap_or_else(|| panic!("kernel thread has no entry point"));
    entry();
    unsafe { finish_current() }
}

#[cfg(feature = "qemu-debugcon")]
static SMOKE_PHASE: AtomicU8 = AtomicU8::new(0);

#[cfg(feature = "qemu-debugcon")]
fn smoke_a() {
    if SMOKE_PHASE
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        panic!("thread A phase 1 executed out of order");
    }
    crate::debugcon::write("VIBRIX: kernel thread A phase 1\r\n");
    yield_now().unwrap_or_else(|_| panic!("thread A yield failed"));
    if SMOKE_PHASE
        .compare_exchange(2, 3, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        panic!("thread A phase 2 executed out of order");
    }
    crate::debugcon::write("VIBRIX: kernel thread A phase 2\r\n");
}

#[cfg(feature = "qemu-debugcon")]
fn smoke_b() {
    if SMOKE_PHASE
        .compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        panic!("thread B phase 1 executed out of order");
    }
    crate::debugcon::write("VIBRIX: kernel thread B phase 1\r\n");
    yield_now().unwrap_or_else(|_| panic!("thread B yield failed"));
    if SMOKE_PHASE
        .compare_exchange(3, 4, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        panic!("thread B phase 2 executed out of order");
    }
    crate::debugcon::write("VIBRIX: kernel thread B phase 2\r\n");
}

#[cfg(feature = "qemu-debugcon")]
pub fn smoke_test() -> Result<RunStats, Error> {
    if !interrupts_disabled() {
        return Err(Error::InterruptsEnabled);
    }
    SMOKE_PHASE.store(0, Ordering::SeqCst);
    let a = spawn(smoke_a)?;
    let b = spawn(smoke_b)?;
    if a.index() == b.index() {
        return Err(Error::Invariant);
    }
    let stats = run()?;
    if SMOKE_PHASE.load(Ordering::SeqCst) != 4 || stats.switches != 5 || stats.completed != 2 {
        return Err(Error::Invariant);
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_entry_frame_has_sysv_alignment() {
        let end = 0x10_000usize;
        let rsp = initial_rsp(end);
        assert_eq!(rsp & 0xf, 0);
        assert_eq!(
            rsp + SWITCH_WORDS * core::mem::size_of::<usize>(),
            end - core::mem::size_of::<usize>()
        );
        assert_eq!((end - core::mem::size_of::<usize>()) & 0xf, 8);
    }

    #[test]
    fn scheduler_skips_current_and_wraps_ready_threads() {
        let mut threads = [Thread::EMPTY; MAX_THREADS];
        threads[0].state = State::Ready;
        threads[2].state = State::Ready;
        assert_eq!(first_ready(&threads), Some(0));
        assert_eq!(next_ready(&threads, 0), Some(2));
        assert_eq!(next_ready(&threads, 2), Some(0));
        threads[0].state = State::Running;
        assert_eq!(next_ready(&threads, 2), None);
    }

    #[test]
    fn exited_slots_are_reusable_candidates() {
        let mut threads = [Thread::EMPTY; MAX_THREADS];
        threads[0].state = State::Exited;
        threads[1].state = State::Running;
        assert!(matches!(threads[0].state, State::Empty | State::Exited));
        assert!(!matches!(threads[1].state, State::Empty | State::Exited));
    }
}
