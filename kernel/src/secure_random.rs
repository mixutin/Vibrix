//! Bounded secure-random service backed by x86-64 RDSEED.
//!
//! The production path has no timer/device-identifier fallback. RDSEED support
//! is checked with CPUID, CF is checked for every sample, retries are bounded,
//! and consecutive duplicate 64-bit samples fail closed. Requests are
//! transactional: the caller buffer changes only after every sample succeeds.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const MAX_REQUEST_BYTES: usize = 256;
pub const RETRIES_PER_WORD: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Unsupported,
    RequestTooLarge,
    Unavailable,
    Repetition,
}

static LOCKED: AtomicBool = AtomicBool::new(false);
static PREVIOUS_VALID: AtomicBool = AtomicBool::new(false);
static PREVIOUS_WORD: AtomicU64 = AtomicU64::new(0);

struct LockGuard;

impl LockGuard {
    fn acquire() -> Self {
        while LOCKED
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Self
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        LOCKED.store(false, Ordering::Release);
    }
}

fn fill_with(
    output: &mut [u8],
    mut previous: Option<u64>,
    mut sample: impl FnMut() -> Option<u64>,
) -> Result<Option<u64>, Error> {
    if output.len() > MAX_REQUEST_BYTES {
        return Err(Error::RequestTooLarge);
    }

    let mut scratch = [0u8; MAX_REQUEST_BYTES];
    let mut written = 0usize;
    while written < output.len() {
        let mut candidate = None;
        for _ in 0..RETRIES_PER_WORD {
            if let Some(word) = sample() {
                candidate = Some(word);
                break;
            }
            core::hint::spin_loop();
        }
        let word = candidate.ok_or(Error::Unavailable)?;
        if previous == Some(word) {
            return Err(Error::Repetition);
        }

        let bytes = word.to_le_bytes();
        let remaining = output.len() - written;
        let take = remaining.min(bytes.len());
        scratch[written..written + take].copy_from_slice(&bytes[..take]);
        written += take;
        previous = Some(word);
    }

    output.copy_from_slice(&scratch[..output.len()]);
    Ok(previous)
}

#[cfg(target_os = "none")]
pub fn supported() -> bool {
    use core::arch::x86_64::__cpuid_count;

    let root = __cpuid_count(0, 0);
    if root.eax < 7 {
        return false;
    }
    __cpuid_count(7, 0).ebx & (1 << 18) != 0
}

#[cfg(target_os = "none")]
fn rdseed64() -> Option<u64> {
    use core::arch::asm;

    let value: u64;
    let ok: u8;
    // SAFETY: caller checks CPUID RDSEED support before this function is used.
    // RDSEED is unprivileged; SETC captures the architectural success flag.
    unsafe {
        asm!(
            "rdseed {value}",
            "setc {ok}",
            value = out(reg) value,
            ok = out(reg_byte) ok,
            options(nomem, nostack)
        );
    }
    (ok != 0).then_some(value)
}

/// Fill a bounded buffer with cryptographically strong random bytes.
///
/// On the current x86-64 platform this requires RDSEED. Unsupported hardware,
/// temporary exhaustion after bounded retries, or a continuous duplicate-word
/// health failure returns an error and leaves the caller buffer unchanged.
#[cfg(target_os = "none")]
pub fn fill(output: &mut [u8]) -> Result<(), Error> {
    if output.len() > MAX_REQUEST_BYTES {
        return Err(Error::RequestTooLarge);
    }
    if !supported() {
        return Err(Error::Unsupported);
    }

    let _guard = LockGuard::acquire();
    let previous = PREVIOUS_VALID
        .load(Ordering::Acquire)
        .then(|| PREVIOUS_WORD.load(Ordering::Relaxed));
    let last = fill_with(output, previous, rdseed64)?;
    if let Some(word) = last {
        PREVIOUS_WORD.store(word, Ordering::Relaxed);
        PREVIOUS_VALID.store(true, Ordering::Release);
    }
    Ok(())
}

#[cfg(not(target_os = "none"))]
pub const fn supported() -> bool {
    false
}

#[cfg(not(target_os = "none"))]
pub fn fill(_output: &mut [u8]) -> Result<(), Error> {
    Err(Error::Unsupported)
}

#[cfg(target_os = "none")]
pub fn self_test() -> Result<(), Error> {
    let mut probe = [0u8; 32];
    fill(&mut probe)
}

#[cfg(not(target_os = "none"))]
pub fn self_test() -> Result<(), Error> {
    Err(Error::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_full_words_and_tail_transactionally() {
        let words = [0x0706_0504_0302_0100, 0x0f0e_0d0c_0b0a_0908];
        let mut index = 0usize;
        let mut output = [0xa5; 13];
        let last = fill_with(&mut output, None, || {
            let value = words.get(index).copied();
            index += 1;
            value
        })
        .unwrap();

        assert_eq!(output, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(last, Some(words[1]));
    }

    #[test]
    fn bounded_retry_failure_leaves_output_unchanged() {
        let mut output = [0x5a; 9];
        assert_eq!(
            fill_with(&mut output, None, || None),
            Err(Error::Unavailable)
        );
        assert_eq!(output, [0x5a; 9]);
    }

    #[test]
    fn repeated_word_fails_closed_without_partial_output() {
        let repeated = 0x1122_3344_5566_7788;
        let mut output = [0xcc; 16];
        assert_eq!(
            fill_with(&mut output, Some(repeated), || Some(repeated)),
            Err(Error::Repetition)
        );
        assert_eq!(output, [0xcc; 16]);
    }

    #[test]
    fn oversized_request_is_rejected_before_sampling() {
        let mut output = [0u8; MAX_REQUEST_BYTES + 1];
        let mut sampled = false;
        assert_eq!(
            fill_with(&mut output, None, || {
                sampled = true;
                Some(1)
            }),
            Err(Error::RequestTooLarge)
        );
        assert!(!sampled);
    }

    #[test]
    fn host_public_api_never_invents_entropy() {
        assert!(!supported());
        let mut output = [0x44; 8];
        assert_eq!(fill(&mut output), Err(Error::Unsupported));
        assert_eq!(output, [0x44; 8]);
    }
}
