//! Shared public-IP check for unpinned remote server and target lists.

use std::net::IpAddr;

/// Whether `address` is permitted in a remote server or target list: rejects unspecified,
/// loopback, RFC 1918 private, link-local, CGNAT/metadata (`100.64.0.0/10`), multicast, reserved,
/// and non-global IPv6 addresses while preserving IANA documentation prefixes used by offline
/// fixtures.
pub(crate) fn is_routable_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || a == 0
                || a >= 224
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            let [first, ..] = v6.segments();
            first & 0xe000 == 0x2000
        }
    }
}
