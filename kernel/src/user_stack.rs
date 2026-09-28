//! Initial userspace argv/environment stack construction.
//!
//! The entry contract is intentionally small and architecture-neutral at the
//! byte-layout level. RSP points at argc, followed by argv pointers + NULL,
//! envp pointers + NULL, with NUL-terminated strings above the vector table.

pub const MAX_ARGS: usize = 16;
pub const MAX_ENV: usize = 16;
pub const MAX_STRING_BYTES: usize = 2048;
pub const USER_MIN: u64 = 0x1000;
pub const USER_END: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialStack {
    pub rsp: u64,
    pub argc: usize,
    pub argv: u64,
    pub envp: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooManyArguments,
    TooManyEnvironment,
    StringBytes,
    EmbeddedNul,
    StackTooSmall,
    AddressOverflow,
    UserAddress,
}

fn checked_strings(values: &[&[u8]]) -> Result<usize, Error> {
    let mut total = 0usize;
    for value in values {
        if value.contains(&0) {
            return Err(Error::EmbeddedNul);
        }
        total = total
            .checked_add(value.len().checked_add(1).ok_or(Error::StringBytes)?)
            .ok_or(Error::StringBytes)?;
    }
    Ok(total)
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

pub fn build(
    stack: &mut [u8],
    stack_base: u64,
    argv: &[&[u8]],
    env: &[&[u8]],
) -> Result<InitialStack, Error> {
    if argv.len() > MAX_ARGS {
        return Err(Error::TooManyArguments);
    }
    if env.len() > MAX_ENV {
        return Err(Error::TooManyEnvironment);
    }

    let argv_bytes = checked_strings(argv)?;
    let env_bytes = checked_strings(env)?;
    let string_bytes = argv_bytes
        .checked_add(env_bytes)
        .ok_or(Error::StringBytes)?;
    if string_bytes > MAX_STRING_BYTES {
        return Err(Error::StringBytes);
    }

    let stack_end = stack_base
        .checked_add(u64::try_from(stack.len()).map_err(|_| Error::AddressOverflow)?)
        .ok_or(Error::AddressOverflow)?;
    if !(USER_MIN..USER_END).contains(&stack_base)
        || stack_end > USER_END
        || stack_end <= stack_base
    {
        return Err(Error::UserAddress);
    }

    let words = 1usize
        .checked_add(argv.len())
        .and_then(|value| value.checked_add(1))
        .and_then(|value| value.checked_add(env.len()))
        .and_then(|value| value.checked_add(1))
        .ok_or(Error::StackTooSmall)?;
    let table_bytes = words.checked_mul(8).ok_or(Error::StackTooSmall)?;
    let needed = string_bytes
        .checked_add(table_bytes)
        .and_then(|value| value.checked_add(15))
        .ok_or(Error::StackTooSmall)?;
    if needed > stack.len() {
        return Err(Error::StackTooSmall);
    }

    let mut argv_ptrs = [0u64; MAX_ARGS];
    let mut env_ptrs = [0u64; MAX_ENV];
    let mut top = stack.len();

    for (index, value) in env.iter().enumerate().rev() {
        top -= value.len() + 1;
        stack[top..top + value.len()].copy_from_slice(value);
        stack[top + value.len()] = 0;
        env_ptrs[index] = stack_base
            .checked_add(u64::try_from(top).map_err(|_| Error::AddressOverflow)?)
            .ok_or(Error::AddressOverflow)?;
    }
    for (index, value) in argv.iter().enumerate().rev() {
        top -= value.len() + 1;
        stack[top..top + value.len()].copy_from_slice(value);
        stack[top + value.len()] = 0;
        argv_ptrs[index] = stack_base
            .checked_add(u64::try_from(top).map_err(|_| Error::AddressOverflow)?)
            .ok_or(Error::AddressOverflow)?;
    }

    let unaligned = top.checked_sub(table_bytes).ok_or(Error::StackTooSmall)?;
    let table = unaligned & !0xf;
    stack[table..top].fill(0);

    let mut cursor = table;
    write_u64(stack, cursor, argv.len() as u64);
    cursor += 8;
    let argv_address = stack_base
        .checked_add(u64::try_from(cursor).map_err(|_| Error::AddressOverflow)?)
        .ok_or(Error::AddressOverflow)?;
    for pointer in argv_ptrs.iter().take(argv.len()) {
        write_u64(stack, cursor, *pointer);
        cursor += 8;
    }
    write_u64(stack, cursor, 0);
    cursor += 8;

    let envp_address = stack_base
        .checked_add(u64::try_from(cursor).map_err(|_| Error::AddressOverflow)?)
        .ok_or(Error::AddressOverflow)?;
    for pointer in env_ptrs.iter().take(env.len()) {
        write_u64(stack, cursor, *pointer);
        cursor += 8;
    }
    write_u64(stack, cursor, 0);

    let rsp = stack_base
        .checked_add(u64::try_from(table).map_err(|_| Error::AddressOverflow)?)
        .ok_or(Error::AddressOverflow)?;
    debug_assert_eq!(rsp & 0xf, 0);

    Ok(InitialStack {
        rsp,
        argc: argv.len(),
        argv: argv_address,
        envp: envp_address,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u64(stack: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(stack[offset..offset + 8].try_into().unwrap())
    }

    #[test]
    fn builds_aligned_argc_argv_envp_and_strings() {
        let mut stack = [0xa5u8; 512];
        let base = 0x7000_0000u64;
        let initial = build(
            &mut stack,
            base,
            &[b"init", b"--safe"],
            &[b"TERM=vibrix", b"HOME=/"],
        )
        .unwrap();

        assert_eq!(initial.rsp & 0xf, 0);
        assert_eq!(initial.argc, 2);
        let table = usize::try_from(initial.rsp - base).unwrap();
        assert_eq!(read_u64(&stack, table), 2);
        assert_eq!(initial.argv, initial.rsp + 8);
        assert_eq!(read_u64(&stack, table + 24), 0);
        assert_eq!(initial.envp, initial.rsp + 32);
        assert_eq!(read_u64(&stack, table + 48), 0);

        let arg0 = usize::try_from(read_u64(&stack, table + 8) - base).unwrap();
        let arg1 = usize::try_from(read_u64(&stack, table + 16) - base).unwrap();
        let env0 = usize::try_from(read_u64(&stack, table + 32) - base).unwrap();
        let env1 = usize::try_from(read_u64(&stack, table + 40) - base).unwrap();
        assert_eq!(&stack[arg0..arg0 + 5], b"init\0");
        assert_eq!(&stack[arg1..arg1 + 7], b"--safe\0");
        assert_eq!(&stack[env0..env0 + 12], b"TERM=vibrix\0");
        assert_eq!(&stack[env1..env1 + 7], b"HOME=/\0");
    }

    #[test]
    fn empty_vectors_still_have_null_terminators() {
        let mut stack = [0u8; 128];
        let base = 0x2000u64;
        let initial = build(&mut stack, base, &[], &[]).unwrap();
        let offset = usize::try_from(initial.rsp - base).unwrap();
        assert_eq!(initial.argc, 0);
        assert_eq!(read_u64(&stack, offset), 0);
        assert_eq!(read_u64(&stack, offset + 8), 0);
        assert_eq!(read_u64(&stack, offset + 16), 0);
        assert_eq!(initial.argv, initial.rsp + 8);
        assert_eq!(initial.envp, initial.rsp + 16);
    }

    #[test]
    fn invalid_input_is_rejected_before_mutation() {
        let original = [0x5au8; 64];
        for result in [
            {
                let mut stack = original;
                let args = [b"a".as_slice(); MAX_ARGS + 1];
                (build(&mut stack, 0x4000, &args, &[]), stack)
            },
            {
                let mut stack = original;
                (build(&mut stack, 0x4000, &[b"a\0b"], &[]), stack)
            },
            {
                let mut stack = original;
                (build(&mut stack, USER_END, &[b"a"], &[]), stack)
            },
        ] {
            assert!(result.0.is_err());
            assert_eq!(result.1, original);
        }
    }

    #[test]
    fn insufficient_stack_and_string_budget_fail_closed() {
        let mut tiny = [0x33u8; 31];
        let before = tiny;
        assert_eq!(
            build(&mut tiny, 0x4000, &[b"hello"], &[b"A=B"]),
            Err(Error::StackTooSmall)
        );
        assert_eq!(tiny, before);

        let huge = [b'x'; MAX_STRING_BYTES + 1];
        let mut stack = [0u8; 4096];
        assert_eq!(
            build(&mut stack, 0x8000, &[huge.as_slice()], &[]),
            Err(Error::StringBytes)
        );
    }
}
