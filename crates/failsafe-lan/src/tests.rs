use std::net::Ipv4Addr;

use super::relay::{parse_ipv4_dest, parse_ipv4_source};

#[test]
fn parses_ipv4_header_fields() {
    // Minimal IPv4 header: version 4, IHL 5, total length 20, src 100.64.1.2, dst 100.64.1.3
    let packet = [
        0x45, 0x00, 0x00, 0x14, 0x00, 0x00, 0x40, 0x00, 0x40, 0x01, 0x00, 0x00, 100, 64, 1, 2,
        100, 64, 1, 3,
    ];

    assert_eq!(
        parse_ipv4_source(&packet),
        Some(Ipv4Addr::new(100, 64, 1, 2))
    );
    assert_eq!(
        parse_ipv4_dest(&packet),
        Some(Ipv4Addr::new(100, 64, 1, 3))
    );
}
