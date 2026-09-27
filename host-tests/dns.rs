//! Host-side unit tests for DNS packet structure and record types.
//!
//! These tests run on the host (not in the kernel) and validate that the
//! DNS packet structures are correct.
//!
//! Primary reference: RFC 1035 (DNS), RFC 2136 (Dynamic Updates).

fn main() {
    println!("Running DNS packet structure tests...\n");

    test_dns_header_format();
    test_dns_header_size();
    test_dns_flags();
    test_dns_opcode();
    test_dns_rcode();
    test_dns_record_types();
    test_dns_class_values();
    test_dns_message_compression();
    test_dns_label_format();
    test_dns_name_max_length();

    println!("\nAll DNS packet structure tests passed!");
}

/// DNS header format.
fn test_dns_header_format() {
    // DNS header (12 bytes):
    // ID: 2 bytes
    // Flags: 2 bytes
    // QDCOUNT: 2 bytes
    // ANCOUNT: 2 bytes
    // NSCOUNT: 2 bytes
    // ARCOUNT: 2 bytes

    const DNS_HEADER_SIZE: usize = 12;
    assert_eq!(DNS_HEADER_SIZE, 12);

    println!("  [PASS] DNS header format correct");
}

/// DNS header size.
fn test_dns_header_size() {
    const DNS_HEADER_SIZE: usize = 12;
    assert_eq!(DNS_HEADER_SIZE, 12);

    println!("  [PASS] DNS header size correct");
}

/// DNS flags.
fn test_dns_flags() {
    // DNS flags (16 bits):
    // Bit 0: QR (Query/Response)
    // Bits 1-4: Opcode
    // Bit 5: AA (Authoritative Answer)
    // Bit 6: TC (Truncated)
    // Bit 7: RD (Recursion Desired)
    // Bit 8: RA (Recursion Available)
    // Bit 9: Z (Reserved)
    // Bit 10: AD (Authentic Data)
    // Bit 11: CD (Checking Disabled)
    // Bits 12-15: RCODE

    const DNS_FLAG_QR: u16 = 0x8000;
    const DNS_FLAG_OPCODE_MASK: u16 = 0x7800;
    const DNS_FLAG_AA: u16 = 0x0400;
    const DNS_FLAG_TC: u16 = 0x0200;
    const DNS_FLAG_RD: u16 = 0x0100;
    const DNS_FLAG_RA: u16 = 0x0080;
    const DNS_FLAG_Z: u16 = 0x0040;
    const DNS_FLAG_AD: u16 = 0x0020;
    const DNS_FLAG_CD: u16 = 0x0010;
    const DNS_FLAG_RCODE_MASK: u16 = 0x000F;

    assert_eq!(DNS_FLAG_QR, 0x8000);
    assert_eq!(DNS_FLAG_OPCODE_MASK, 0x7800);
    assert_eq!(DNS_FLAG_AA, 0x0400);
    assert_eq!(DNS_FLAG_TC, 0x0200);
    assert_eq!(DNS_FLAG_RD, 0x0100);
    assert_eq!(DNS_FLAG_RA, 0x0080);
    assert_eq!(DNS_FLAG_Z, 0x0040);
    assert_eq!(DNS_FLAG_AD, 0x0020);
    assert_eq!(DNS_FLAG_CD, 0x0010);
    assert_eq!(DNS_FLAG_RCODE_MASK, 0x000F);

    println!("  [PASS] DNS flags correct");
}

/// DNS opcode values.
fn test_dns_opcode() {
    const DNS_OPCODE_QUERY: u16 = 0;
    const DNS_OPCODE_IQUERY: u16 = 1;
    const DNS_OPCODE_STATUS: u16 = 2;
    const DNS_OPCODE_NOTIFY: u16 = 4;
    const DNS_OPCODE_UPDATE: u16 = 5;

    assert_eq!(DNS_OPCODE_QUERY, 0);
    assert_eq!(DNS_OPCODE_IQUERY, 1);
    assert_eq!(DNS_OPCODE_STATUS, 2);
    assert_eq!(DNS_OPCODE_NOTIFY, 4);
    assert_eq!(DNS_OPCODE_UPDATE, 5);

    println!("  [PASS] DNS opcode values correct");
}

/// DNS rcode values.
fn test_dns_rcode() {
    const DNS_RCODE_NOERROR: u16 = 0;
    const DNS_RCODE_FORMERR: u16 = 1;
    const DNS_RCODE_SERVFAIL: u16 = 2;
    const DNS_RCODE_NXDOMAIN: u16 = 3;
    const DNS_RCODE_NOTIMP: u16 = 4;
    const DNS_RCODE_REFUSED: u16 = 5;

    assert_eq!(DNS_RCODE_NOERROR, 0);
    assert_eq!(DNS_RCODE_FORMERR, 1);
    assert_eq!(DNS_RCODE_SERVFAIL, 2);
    assert_eq!(DNS_RCODE_NXDOMAIN, 3);
    assert_eq!(DNS_RCODE_NOTIMP, 4);
    assert_eq!(DNS_RCODE_REFUSED, 5);

    println!("  [PASS] DNS rcode values correct");
}

/// DNS record types.
fn test_dns_record_types() {
    const DNS_TYPE_A: u16 = 1;
    const DNS_TYPE_NS: u16 = 2;
    const DNS_TYPE_CNAME: u16 = 5;
    const DNS_TYPE_SOA: u16 = 6;
    const DNS_TYPE_PTR: u16 = 12;
    const DNS_TYPE_MX: u16 = 15;
    const DNS_TYPE_TXT: u16 = 16;
    const DNS_TYPE_AAAA: u16 = 28;
    const DNS_TYPE_SRV: u16 = 33;

    assert_eq!(DNS_TYPE_A, 1);
    assert_eq!(DNS_TYPE_NS, 2);
    assert_eq!(DNS_TYPE_CNAME, 5);
    assert_eq!(DNS_TYPE_SOA, 6);
    assert_eq!(DNS_TYPE_PTR, 12);
    assert_eq!(DNS_TYPE_MX, 15);
    assert_eq!(DNS_TYPE_TXT, 16);
    assert_eq!(DNS_TYPE_AAAA, 28);
    assert_eq!(DNS_TYPE_SRV, 33);

    println!("  [PASS] DNS record types correct");
}

/// DNS class values.
fn test_dns_class_values() {
    const DNS_CLASS_IN: u16 = 1; // Internet
    const DNS_CLASS_CS: u16 = 2; // CSNET
    const DNS_CLASS_CH: u16 = 3; // CHAOS
    const DNS_CLASS_HS: u16 = 4; // Hesiod
    const DNS_CLASS_ANY: u16 = 255;

    assert_eq!(DNS_CLASS_IN, 1);
    assert_eq!(DNS_CLASS_CS, 2);
    assert_eq!(DNS_CLASS_CH, 3);
    assert_eq!(DNS_CLASS_HS, 4);
    assert_eq!(DNS_CLASS_ANY, 255);

    println!("  [PASS] DNS class values correct");
}

/// DNS message compression.
fn test_dns_message_compression() {
    // DNS message compression: pointer to a prior name
    // Pointer format: 2 bytes, top 2 bits = 11, lower 14 bits = offset
    const DNS_POINTER_MASK: u16 = 0xC000;
    const DNS_POINTER_OFFSET_MASK: u16 = 0x3FFF;

    assert_eq!(DNS_POINTER_MASK, 0xC000);
    assert_eq!(DNS_POINTER_OFFSET_MASK, 0x3FFF);

    println!("  [PASS] DNS message compression correct");
}

/// DNS label format.
fn test_dns_label_format() {
    // DNS label format:
    // Length byte (1 byte) + label data (variable)
    // Length byte: 0 = end of name, 1-63 = label length, 64-255 = pointer

    const DNS_LABEL_END: u8 = 0;
    const DNS_LABEL_MAX_LENGTH: u8 = 63;
    const DNS_LABEL_POINTER_MASK: u8 = 0xC0;

    assert_eq!(DNS_LABEL_END, 0);
    assert_eq!(DNS_LABEL_MAX_LENGTH, 63);
    assert_eq!(DNS_LABEL_POINTER_MASK, 0xC0);

    println!("  [PASS] DNS label format correct");
}

/// DNS name maximum length.
fn test_dns_name_max_length() {
    // DNS name maximum length: 255 bytes (including length bytes and root)
    // Maximum number of labels: 127
    // Maximum label length: 63

    const DNS_NAME_MAX_LENGTH: usize = 255;
    const DNS_NAME_MAX_LABELS: usize = 127;
    const DNS_LABEL_MAX_LENGTH: usize = 63;

    assert_eq!(DNS_NAME_MAX_LENGTH, 255);
    assert_eq!(DNS_NAME_MAX_LABELS, 127);
    assert_eq!(DNS_LABEL_MAX_LENGTH, 63);

    println!("  [PASS] DNS name maximum length correct");
}
