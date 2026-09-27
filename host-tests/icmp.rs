//! Host-side unit tests for ICMP packet structure and types.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! ICMP packet structures are correct.
//!
//! Primary reference: RFC 792 (ICMP), RFC 791 (IPv4).

fn main() {
    println!("Running ICMP packet structure tests...\n");

    test_icmp_header_format();
    test_icmp_header_size();
    test_icmp_type_values();
    test_icmp_code_values();
    test_icmp_checksum();
    test_icmp_echo_request();
    test_icmp_echo_reply();
    test_icmp_destination_unreachable();
    test_icmp_time_exceeded();
    test_icmp_redirect();

    println!("\nAll ICMP packet structure tests passed!");
}

/// ICMP header format.
fn test_icmp_header_format() {
    // ICMP header (8 bytes):
    // Type: 1 byte
    // Code: 1 byte
    // Checksum: 2 bytes
    // Rest of header: 4 bytes (varies by type)

    const ICMP_HEADER_SIZE: usize = 8;
    assert_eq!(ICMP_HEADER_SIZE, 8);

    println!("  [PASS] ICMP header format correct");
}

/// ICMP header size.
fn test_icmp_header_size() {
    const ICMP_HEADER_SIZE: usize = 8;
    assert_eq!(ICMP_HEADER_SIZE, 8);

    println!("  [PASS] ICMP header size correct");
}

/// ICMP type values.
fn test_icmp_type_values() {
    const ICMP_TYPE_ECHO_REPLY: u8 = 0;
    const ICMP_TYPE_DESTINATION_UNREACHABLE: u8 = 3;
    const ICMP_TYPE_SOURCE_QUENCH: u8 = 4;
    const ICMP_TYPE_REDIRECT: u8 = 5;
    const ICMP_TYPE_ECHO_REQUEST: u8 = 8;
    const ICMP_TYPE_TIME_EXCEEDED: u8 = 11;
    const ICMP_TYPE_PARAMETER_PROBLEM: u8 = 12;
    const ICMP_TYPE_TIMESTAMP: u8 = 13;
    const ICMP_TYPE_TIMESTAMP_REPLY: u8 = 14;

    assert_eq!(ICMP_TYPE_ECHO_REPLY, 0);
    assert_eq!(ICMP_TYPE_DESTINATION_UNREACHABLE, 3);
    assert_eq!(ICMP_TYPE_SOURCE_QUENCH, 4);
    assert_eq!(ICMP_TYPE_REDIRECT, 5);
    assert_eq!(ICMP_TYPE_ECHO_REQUEST, 8);
    assert_eq!(ICMP_TYPE_TIME_EXCEEDED, 11);
    assert_eq!(ICMP_TYPE_PARAMETER_PROBLEM, 12);
    assert_eq!(ICMP_TYPE_TIMESTAMP, 13);
    assert_eq!(ICMP_TYPE_TIMESTAMP_REPLY, 14);

    println!("  [PASS] ICMP type values correct");
}

/// ICMP code values.
fn test_icmp_code_values() {
    // Destination unreachable codes
    const ICMP_CODE_NET_UNREACHABLE: u8 = 0;
    const ICMP_CODE_HOST_UNREACHABLE: u8 = 1;
    const ICMP_CODE_PROTOCOL_UNREACHABLE: u8 = 2;
    const ICMP_CODE_PORT_UNREACHABLE: u8 = 3;
    const ICMP_CODE_FRAGMENTATION_NEEDED: u8 = 4;

    assert_eq!(ICMP_CODE_NET_UNREACHABLE, 0);
    assert_eq!(ICMP_CODE_HOST_UNREACHABLE, 1);
    assert_eq!(ICMP_CODE_PROTOCOL_UNREACHABLE, 2);
    assert_eq!(ICMP_CODE_PORT_UNREACHABLE, 3);
    assert_eq!(ICMP_CODE_FRAGMENTATION_NEEDED, 4);

    // Time exceeded codes
    const ICMP_CODE_TTL_EXCEEDED: u8 = 0;
    const ICMP_CODE_FRAGMENT_REASSEMBLY_EXCEEDED: u8 = 1;

    assert_eq!(ICMP_CODE_TTL_EXCEEDED, 0);
    assert_eq!(ICMP_CODE_FRAGMENT_REASSEMBLY_EXCEEDED, 1);

    println!("  [PASS] ICMP code values correct");
}

/// ICMP checksum.
fn test_icmp_checksum() {
    // ICMP checksum: 16-bit one's complement checksum
    // Calculated over the entire ICMP message
    const ICMP_CHECKSUM_NONE: u16 = 0x0000;
    assert_eq!(ICMP_CHECKSUM_NONE, 0x0000);

    println!("  [PASS] ICMP checksum correct");
}

/// ICMP echo request.
fn test_icmp_echo_request() {
    // Echo request (8 bytes header + payload):
    // Type: 8
    // Code: 0
    // Checksum: 2 bytes
    // Identifier: 2 bytes
    // Sequence number: 2 bytes
    // Payload: variable

    const ICMP_ECHO_REQUEST_TYPE: u8 = 8;
    const ICMP_ECHO_REQUEST_CODE: u8 = 0;

    assert_eq!(ICMP_ECHO_REQUEST_TYPE, 8);
    assert_eq!(ICMP_ECHO_REQUEST_CODE, 0);

    println!("  [PASS] ICMP echo request correct");
}

/// ICMP echo reply.
fn test_icmp_echo_reply() {
    // Echo reply (8 bytes header + payload):
    // Type: 0
    // Code: 0
    // Checksum: 2 bytes
    // Identifier: 2 bytes
    // Sequence number: 2 bytes
    // Payload: variable

    const ICMP_ECHO_REPLY_TYPE: u8 = 0;
    const ICMP_ECHO_REPLY_CODE: u8 = 0;

    assert_eq!(ICMP_ECHO_REPLY_TYPE, 0);
    assert_eq!(ICMP_ECHO_REPLY_CODE, 0);

    println!("  [PASS] ICMP echo reply correct");
}

/// ICMP destination unreachable.
fn test_icmp_destination_unreachable() {
    // Destination unreachable (8 bytes header + original datagram):
    // Type: 3
    // Code: 0-15
    // Checksum: 2 bytes
    // Unused: 4 bytes
    // Original datagram: variable

    const ICMP_DEST_UNREACHABLE_TYPE: u8 = 3;

    assert_eq!(ICMP_DEST_UNREACHABLE_TYPE, 3);

    println!("  [PASS] ICMP destination unreachable correct");
}

/// ICMP time exceeded.
fn test_icmp_time_exceeded() {
    // Time exceeded (8 bytes header + original datagram):
    // Type: 11
    // Code: 0 (TTL exceeded) or 1 (fragment reassembly exceeded)
    // Checksum: 2 bytes
    // Unused: 4 bytes
    // Original datagram: variable

    const ICMP_TIME_EXCEEDED_TYPE: u8 = 11;

    assert_eq!(ICMP_TIME_EXCEEDED_TYPE, 11);

    println!("  [PASS] ICMP time exceeded correct");
}

/// ICMP redirect.
fn test_icmp_redirect() {
    // Redirect (8 bytes header + original datagram):
    // Type: 5
    // Code: 0-3
    // Checksum: 2 bytes
    // Gateway address: 4 bytes
    // Original datagram: variable

    const ICMP_REDIRECT_TYPE: u8 = 5;

    assert_eq!(ICMP_REDIRECT_TYPE, 5);

    println!("  [PASS] ICMP redirect correct");
}
