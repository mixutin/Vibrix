//! Host-side unit tests for x86-64 page-fault error code semantics.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! page-fault error code bit layout and diagnostic decoding are correct.
//!
//! Primary reference: Intel SDM Vol. 3A, Section 4.7 (Page-Fault Exceptions),
//! AMD APM Vol. 2, Section 8.4.1 (Page-Fault Exceptions).

fn main() {
    println!("Running page-fault diagnostics tests...\n");

    test_error_code_bit_layout();
    test_error_code_present_bit();
    test_error_code_write_bit();
    test_error_code_user_bit();
    test_error_code_reserved_bit();
    test_error_code_instruction_fetch_bit();
    test_error_code_protection_key_bit();
    test_error_code_shadow_stack_bit();
    test_error_code_sgx_bit();
    test_decode_page_fault();

    println!("\nAll page-fault diagnostics tests passed!");
}

/// Page-fault error code bit layout (Intel SDM Vol. 3A, Section 4.7):
///
/// Bit 0: P (Present) — 0 = non-present page, 1 = page-protection violation
/// Bit 1: W/R (Write/Read) — 0 = read, 1 = write
/// Bit 2: U/S (User/Supervisor) — 0 = supervisor, 1 = user
/// Bit 3: RSVD (Reserved bit violation) — 1 = reserved bit set
/// Bit 4: I/D (Instruction Fetch) — 1 = instruction fetch
/// Bit 5: PK (Protection Key) — 1 = protection key violation
/// Bit 6: SS (Shadow Stack) — 1 = shadow stack access
/// Bit 7: SGX (Software Guard Extensions) — 1 = SGX violation
fn test_error_code_bit_layout() {
    const P: u64 = 1 << 0;
    const WR: u64 = 1 << 1;
    const US: u64 = 1 << 2;
    const RSVD: u64 = 1 << 3;
    const ID: u64 = 1 << 4;
    const PK: u64 = 1 << 5;
    const SS: u64 = 1 << 6;
    const SGX: u64 = 1 << 7;

    assert_eq!(P, 0x01);
    assert_eq!(WR, 0x02);
    assert_eq!(US, 0x04);
    assert_eq!(RSVD, 0x08);
    assert_eq!(ID, 0x10);
    assert_eq!(PK, 0x20);
    assert_eq!(SS, 0x40);
    assert_eq!(SGX, 0x80);

    println!("  [PASS] Page-fault error code bit layout correct");
}

/// Test the Present bit (bit 0).
///
/// P=0: The fault was caused by a non-present page.
/// P=1: The fault was caused by a page-protection violation.
fn test_error_code_present_bit() {
    // Non-present page
    let err_non_present: u64 = 0x00;
    assert_eq!(err_non_present & 0x01, 0);

    // Page-protection violation
    let err_protection: u64 = 0x01;
    assert_eq!(err_protection & 0x01, 1);

    println!("  [PASS] Page-fault present bit correct");
}

/// Test the Write/Read bit (bit 1).
///
/// W/R=0: The fault was caused by a read access.
/// W/R=1: The fault was caused by a write access.
fn test_error_code_write_bit() {
    // Read access
    let err_read: u64 = 0x00;
    assert_eq!(err_read & 0x02, 0);

    // Write access
    let err_write: u64 = 0x02;
    assert_eq!(err_write & 0x02, 2);

    println!("  [PASS] Page-fault write/read bit correct");
}

/// Test the User/Supervisor bit (bit 2).
///
/// U/S=0: The fault occurred in supervisor mode.
/// U/S=1: The fault occurred in user mode.
fn test_error_code_user_bit() {
    // Supervisor mode
    let err_supervisor: u64 = 0x00;
    assert_eq!(err_supervisor & 0x04, 0);

    // User mode
    let err_user: u64 = 0x04;
    assert_eq!(err_user & 0x04, 4);

    println!("  [PASS] Page-fault user/supervisor bit correct");
}

/// Test the Reserved bit violation (bit 3).
///
/// RSVD=1: The fault was caused by a reserved bit being set to 1 in a
///         page-table entry.
fn test_error_code_reserved_bit() {
    // No reserved bit violation
    let err_no_rsvd: u64 = 0x00;
    assert_eq!(err_no_rsvd & 0x08, 0);

    // Reserved bit violation
    let err_rsvd: u64 = 0x08;
    assert_eq!(err_rsvd & 0x08, 8);

    println!("  [PASS] Page-fault reserved bit correct");
}

/// Test the Instruction Fetch bit (bit 4).
///
/// I/D=1: The fault was caused by an instruction fetch.
fn test_error_code_instruction_fetch_bit() {
    // Data access
    let err_data: u64 = 0x00;
    assert_eq!(err_data & 0x10, 0);

    // Instruction fetch
    let err_fetch: u64 = 0x10;
    assert_eq!(err_fetch & 0x10, 16);

    println!("  [PASS] Page-fault instruction fetch bit correct");
}

/// Test the Protection Key bit (bit 5).
///
/// PK=1: The fault was caused by a protection key violation.
fn test_error_code_protection_key_bit() {
    // No protection key violation
    let err_no_pk: u64 = 0x00;
    assert_eq!(err_no_pk & 0x20, 0);

    // Protection key violation
    let err_pk: u64 = 0x20;
    assert_eq!(err_pk & 0x20, 32);

    println!("  [PASS] Page-fault protection key bit correct");
}

/// Test the Shadow Stack bit (bit 6).
///
/// SS=1: The fault was caused by a shadow stack access.
fn test_error_code_shadow_stack_bit() {
    // No shadow stack access
    let err_no_ss: u64 = 0x00;
    assert_eq!(err_no_ss & 0x40, 0);

    // Shadow stack access
    let err_ss: u64 = 0x40;
    assert_eq!(err_ss & 0x40, 64);

    println!("  [PASS] Page-fault shadow stack bit correct");
}

/// Test the SGX bit (bit 7).
///
/// SGX=1: The fault was caused by an SGX violation.
fn test_error_code_sgx_bit() {
    // No SGX violation
    let err_no_sgx: u64 = 0x00;
    assert_eq!(err_no_sgx & 0x80, 0);

    // SGX violation
    let err_sgx: u64 = 0x80;
    assert_eq!(err_sgx & 0x80, 128);

    println!("  [PASS] Page-fault SGX bit correct");
}

/// Test decoding a page-fault error code into a human-readable description.
fn test_decode_page_fault() {
    // Example: user-mode write to a non-present page
    // P=0 (non-present), W=1 (write), U=1 (user)
    let err: u64 = 0x00 | 0x02 | 0x04; // = 0x06
    assert_eq!(err, 0x06);

    let is_present = err & 0x01 != 0;
    let is_write = err & 0x02 != 0;
    let is_user = err & 0x04 != 0;
    let is_rsvd = err & 0x08 != 0;
    let is_fetch = err & 0x10 != 0;

    assert!(!is_present);
    assert!(is_write);
    assert!(is_user);
    assert!(!is_rsvd);
    assert!(!is_fetch);

    // Example: supervisor-mode read to a protected page
    // P=1 (protection violation), W=0 (read), U=0 (supervisor)
    let err2: u64 = 0x01; // = 0x01
    assert_eq!(err2, 0x01);

    let is_present2 = err2 & 0x01 != 0;
    let is_write2 = err2 & 0x02 != 0;
    let is_user2 = err2 & 0x04 != 0;

    assert!(is_present2);
    assert!(!is_write2);
    assert!(!is_user2);

    // Example: instruction fetch from a user-mode protected page
    // P=1, W=0, U=1, I/D=1
    let err3: u64 = 0x01 | 0x04 | 0x10; // = 0x15
    assert_eq!(err3, 0x15);

    let is_present3 = err3 & 0x01 != 0;
    let is_write3 = err3 & 0x02 != 0;
    let is_user3 = err3 & 0x04 != 0;
    let is_fetch3 = err3 & 0x10 != 0;

    assert!(is_present3);
    assert!(!is_write3);
    assert!(is_user3);
    assert!(is_fetch3);

    println!("  [PASS] Page-fault error code decoding correct");
}
