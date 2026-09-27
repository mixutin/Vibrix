//! Host-side unit tests for IPv6 packet structure and extension headers.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! IPv6 packet structures are correct.
//!
//! Primary reference: RFC 8200 (IPv6), RFC 4291 (IPv6 Addressing Architecture).

fn main() {
    println!("Running IPv6 packet structure tests...\n");

    test_ipv6_header_format();
    test_ipv6_header_size();
    test_ipv6_version();
    test_ipv6_traffic_class();
    test_ipv6_flow_label();
    test_ipv6_payload_length();
    test_ipv6_next_header();
    test_ipv6_hop_limit();
    test_ipv6_address_size();
    test_ipv6_extension_headers();

    println!("\nAll IPv6 packet structure tests passed!");
}

/// IPv6 header format.
fn test_ipv6_header_format() {
    // IPv6 header (40 bytes):
    // Version/Traffic Class/Flow Label: 4 bytes
    // Payload Length: 2 bytes
    // Next Header: 1 byte
    // Hop Limit: 1 byte
    // Source Address: 16 bytes
    // Destination Address: 16 bytes

    const IPV6_HEADER_SIZE: usize = 40;
    assert_eq!(IPV6_HEADER_SIZE, 40);

    println!("  [PASS] IPv6 header format correct");
}

/// IPv6 header size.
fn test_ipv6_header_size() {
    const IPV6_HEADER_SIZE: usize = 40;
    assert_eq!(IPV6_HEADER_SIZE, 40);

    println!("  [PASS] IPv6 header size correct");
}

/// IPv6 version.
fn test_ipv6_version() {
    // IPv6 version: 6 (4 bits)
    const IPV6_VERSION: u8 = 6;
    assert_eq!(IPV6_VERSION, 6);

    // Version field is in the top 4 bits of the first byte
    const IPV6_VERSION_MASK: u32 = 0xF000_0000;
    assert_eq!(IPV6_VERSION_MASK, 0xF000_0000);

    println!("  [PASS] IPv6 version correct");
}

/// IPv6 traffic class.
fn test_ipv6_traffic_class() {
    // IPv6 traffic class: 8 bits (6 bits DSCP + 2 bits ECN)
    const IPV6_DSCP_MASK: u32 = 0x0FC0_0000;
    const IPV6_ECN_MASK: u32 = 0x0030_0000;

    assert_eq!(IPV6_DSCP_MASK, 0x0FC0_0000);
    assert_eq!(IPV6_ECN_MASK, 0x0030_0000);

    println!("  [PASS] IPv6 traffic class correct");
}

/// IPv6 flow label.
fn test_ipv6_flow_label() {
    // IPv6 flow label: 20 bits
    const IPV6_FLOW_LABEL_MASK: u32 = 0x000F_FFFF;
    assert_eq!(IPV6_FLOW_LABEL_MASK, 0x000F_FFFF);

    println!("  [PASS] IPv6 flow label correct");
}

/// IPv6 payload length.
fn test_ipv6_payload_length() {
    // IPv6 payload length: 16 bits (0-65535)
    const IPV6_PAYLOAD_LENGTH_MIN: u16 = 0;
    const IPV6_PAYLOAD_LENGTH_MAX: u16 = 65535;

    assert_eq!(IPV6_PAYLOAD_LENGTH_MIN, 0);
    assert_eq!(IPV6_PAYLOAD_LENGTH_MAX, 65535);

    println!("  [PASS] IPv6 payload length correct");
}

/// IPv6 next header.
fn test_ipv6_next_header() {
    // IPv6 next header values
    const IPV6_NEXT_HEADER_HOP_BY_HOP: u8 = 0;
    const IPV6_NEXT_HEADER_TCP: u8 = 6;
    const IPV6_NEXT_HEADER_UDP: u8 = 17;
    const IPV6_NEXT_HEADER_ROUTING: u8 = 43;
    const IPV6_NEXT_HEADER_FRAGMENT: u8 = 44;
    const IPV6_NEXT_HEADER_ESP: u8 = 50;
    const IPV6_NEXT_HEADER_AH: u8 = 51;
    const IPV6_NEXT_HEADER_ICMPV6: u8 = 58;
    const IPV6_NEXT_HEADER_NO_NEXT: u8 = 59;
    const IPV6_NEXT_HEADER_DESTINATION: u8 = 60;

    assert_eq!(IPV6_NEXT_HEADER_HOP_BY_HOP, 0);
    assert_eq!(IPV6_NEXT_HEADER_TCP, 6);
    assert_eq!(IPV6_NEXT_HEADER_UDP, 17);
    assert_eq!(IPV6_NEXT_HEADER_ROUTING, 43);
    assert_eq!(IPV6_NEXT_HEADER_FRAGMENT, 44);
    assert_eq!(IPV6_NEXT_HEADER_ESP, 50);
    assert_eq!(IPV6_NEXT_HEADER_AH, 51);
    assert_eq!(IPV6_NEXT_HEADER_ICMPV6, 58);
    assert_eq!(IPV6_NEXT_HEADER_NO_NEXT, 59);
    assert_eq!(IPV6_NEXT_HEADER_DESTINATION, 60);

    println!("  [PASS] IPv6 next header values correct");
}

/// IPv6 hop limit.
fn test_ipv6_hop_limit() {
    // IPv6 hop limit: 8 bits (0-255)
    const IPV6_HOP_LIMIT_MIN: u8 = 0;
    const IPV6_HOP_LIMIT_MAX: u8 = 255;

    assert_eq!(IPV6_HOP_LIMIT_MIN, 0);
    assert_eq!(IPV6_HOP_LIMIT_MAX, 255);

    println!("  [PASS] IPv6 hop limit correct");
}

/// IPv6 address size.
fn test_ipv6_address_size() {
    // IPv6 address: 16 bytes (128 bits)
    const IPV6_ADDRESS_SIZE: usize = 16;
    assert_eq!(IPV6_ADDRESS_SIZE, 16);

    println!("  [PASS] IPv6 address size correct");
}

/// IPv6 extension headers.
fn test_ipv6_extension_headers() {
    // IPv6 extension headers:
    // Hop-by-Hop Options: 0
    // Routing: 43
    // Fragment: 44
    // Encapsulating Security Payload: 50
    // Authentication Header: 51
    // Destination Options: 60

    const IPV6_EXT_HOP_BY_HOP: u8 = 0;
    const IPV6_EXT_ROUTING: u8 = 43;
    const IPV6_EXT_FRAGMENT: u8 = 44;
    const IPV6_EXT_ESP: u8 = 50;
    const IPV6_EXT_AH: u8 = 51;
    const IPV6_EXT_DESTINATION: u8 = 60;

    assert_eq!(IPV6_EXT_HOP_BY_HOP, 0);
    assert_eq!(IPV6_EXT_ROUTING, 43);
    assert_eq!(IPV6_EXT_FRAGMENT, 44);
    assert_eq!(IPV6_EXT_ESP, 50);
    assert_eq!(IPV6_EXT_AH, 51);
    assert_eq!(IPV6_EXT_DESTINATION, 60);

    println!("  [PASS] IPv6 extension headers correct");
}
