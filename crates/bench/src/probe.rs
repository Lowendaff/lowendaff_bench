//! Host facts for `Env`, probed once before anything runs: architecture, platform,
//! IPv4/IPv6 reachability and the C library.

use std::time::Duration;

use bench_api::Env;
use bench_host::Root;
use bench_host::glibc;
use bench_host::platform;
use tokio::net::TcpStream;

/// Public DNS anycast addresses (Cloudflare, AliDNS) on the HTTPS port; reaching any
/// one target of a family counts as reachable.
const IPV4_TARGETS: [&str; 2] = ["1.1.1.1:443", "223.5.5.5:443"];
const IPV6_TARGETS: [&str; 2] = ["[2606:4700:4700::1111]:443", "[2400:3200::1]:443"];
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn probe() -> Env {
    let (ipv4, ipv6) = tokio::join!(reachable(&IPV4_TARGETS), reachable(&IPV6_TARGETS));
    Env {
        arch: std::env::consts::ARCH.to_owned(),
        ipv4,
        ipv6,
        platform: platform::detect(&Root::system(), platform::cpuid().as_ref()),
        libc: glibc::read(&Root::system(), std::env::consts::ARCH),
        ..Env::default()
    }
}

/// True when a TCP handshake with any target completes within the timeout.
pub async fn reachable(targets: &[&str]) -> bool {
    for target in targets {
        if let Ok(Ok(_)) = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(*target)).await
        {
            return true;
        }
    }
    false
}
