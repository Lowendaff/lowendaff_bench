//! Unit tests for `bench_api::is_routable_endpoint_ip`.

use std::net::IpAddr;

#[test]
fn routable_endpoint_ip_rejects_internal_loopback_metadata_and_special_ranges() {
    for allowed in [
        "1.1.1.1",
        "8.8.8.8",
        "100.63.255.255",
        "100.128.0.1",
        "198.51.100.10",
        "203.0.113.7",
        "2606:4700:4700::1111",
        "2001:db8::10",
    ] {
        let ip: IpAddr = allowed.parse().expect("valid IP");
        assert!(
            bench_api::is_routable_endpoint_ip(ip),
            "{allowed} should be allowed"
        );
    }

    for rejected in [
        "0.0.0.0",
        "0.1.2.3",
        "127.0.0.1",
        "127.0.0.53",
        "10.0.0.1",
        "172.16.0.1",
        "172.31.255.254",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "100.100.100.200",
        "192.0.0.1",
        "192.88.99.1",
        "198.18.0.1",
        "198.19.255.254",
        "224.0.0.1",
        "240.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "64:ff9b::a00:1",
        "2001::1",
        "2001:2::1",
        "2001:10::1",
        "2001:20::1",
        "2002:7f00:1::1",
        "2002:a9fe:a9fe::1",
        "fc00::1",
        "fd12:3456::1",
        "fe80::1",
        "ff02::1",
    ] {
        let ip: IpAddr = rejected.parse().expect("valid IP");
        assert!(
            !bench_api::is_routable_endpoint_ip(ip),
            "{rejected} must be rejected"
        );
    }
}
