//! The network module against a local HTTP server that stands in for the address sources,
//! through the real reqwest client: answers that count and those that do not, the order of the
//! sources, the limits on time and size, and the family of each client. Then reachability, and
//! the pure functions that read, check and mask addresses.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench::network;
use bench::network::Source;
use bench_host::network::Address;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::network::Why;
use common::Captured;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

mod common;

/// The requests the server has seen, by path.
type Seen = Arc<Mutex<BTreeMap<String, usize>>>;

/// Answers on 127.0.0.1: `/trace` and `/whole` with a public address in each source's format,
/// `/unavailable` with HTTP 503, `/garbage`, `/private` and `/long` with answers that do not
/// count, and `/slow` never.
async fn serve() -> (SocketAddr, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a local server");
    let address = listener
        .local_addr()
        .expect("a bound listener has an address");
    let seen: Seen = Arc::default();
    let counted = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let counted = Arc::clone(&counted);
            tokio::spawn(async move {
                let mut request = vec![0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..read]).into_owned();
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                *counted
                    .lock()
                    .expect("no test thread panics while holding the counts")
                    .entry(path.clone())
                    .or_default() += 1;
                let (status, body) = match path.as_str() {
                    "/trace" => ("200 OK", "fl=1\nh=1.1.1.1\nip=8.8.8.8\nts=1\n".to_owned()),
                    "/whole" => ("200 OK", "8.8.4.4\n".to_owned()),
                    "/unavailable" => ("503 Service Unavailable", String::new()),
                    "/garbage" => ("200 OK", "<html>busy</html>".to_owned()),
                    "/private" => ("200 OK", "10.1.2.3".to_owned()),
                    // The address comes only after the first 4 KiB.
                    "/long" => ("200 OK", format!("x={}\nip=8.8.8.8\n", "a".repeat(5000))),
                    "/slow" => {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        return;
                    }
                    _ => ("404 Not Found", String::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(body.as_bytes()).await;
            });
        }
    });
    (address, seen)
}

/// Probes as the program does, with the log captured at the debug level, so that no test logs
/// while another listens (see `common`). Returns the network and the log's lines.
async fn probe(
    ipv4: &[SocketAddr],
    ipv6: &[SocketAddr],
    sources: &[Source],
) -> (Network, Vec<String>) {
    let (log, capturing) = Captured::start(true);
    let network = network::probe_with(ipv4, ipv6, sources).await;
    drop(capturing);
    (network, log.lines())
}

/// A source at `path` of the server, for both families.
fn source(name: &'static str, server: SocketAddr, path: &str, trace: bool) -> Source {
    let url: &'static str = Box::leak(format!("http://{server}{path}").into_boxed_str());
    Source {
        name,
        ipv4: Some(url),
        ipv6: Some(url),
        read: if trace {
            network::trace
        } else {
            network::whole
        },
    }
}

fn ip(text: &str) -> IpAddr {
    text.parse().expect("a valid address")
}

#[tokio::test]
async fn the_first_valid_answer_wins_and_names_its_source() {
    let (server, seen) = serve().await;
    let sources = [
        source("down", server, "/unavailable", false),
        source("busy", server, "/garbage", false),
        source("inside", server, "/private", false),
        source("traced", server, "/trace", true),
        source("never", server, "/whole", false),
    ];

    let (network, _) = probe(&[server], &[], &sources).await;

    assert_eq!(
        network.ipv4.address,
        Address::Found {
            address: ip("8.8.8.8"),
            source: "traced"
        }
    );
    let seen = seen.lock().expect("the counts").clone();
    assert_eq!(
        seen.get("/whole"),
        None,
        "the source after the answer is not asked"
    );
    assert_eq!(
        network.ipv6.address,
        Address::NotAsked,
        "no IPv6 target answered"
    );
    assert!(!network.ipv6.reachable());
}

#[tokio::test]
async fn verbose_logs_each_answer_with_the_address_masked() {
    let (server, _) = serve().await;
    let sources = [
        source("down", server, "/unavailable", false),
        source("traced", server, "/trace", true),
    ];

    let (network, log) = probe(&[server], &[], &sources).await;

    assert!(network.ipv4.reachable());
    assert_eq!(
        log,
        ["网络：down 返回 HTTP 503", "网络：traced 返回 IPv4 8.8.*.*"]
    );
}

#[tokio::test]
async fn when_every_source_fails_each_one_says_why() {
    let (server, _) = serve().await;
    let sources = [
        source("down", server, "/unavailable", false),
        source("busy", server, "/garbage", false),
        source("inside", server, "/private", false),
        source("long", server, "/long", true),
        source("slow", server, "/slow", false),
    ];

    let (network, _) = probe(&[server], &[], &sources).await;

    assert_eq!(
        network.ipv4.address,
        Address::NotFound(vec![
            ("down", Why::Status(503)),
            ("busy", Why::NotAnAddress),
            ("inside", Why::NotPublic),
            ("long", Why::NotAnAddress),
            ("slow", Why::TimedOut),
        ])
    );
}

#[tokio::test]
async fn a_client_reaches_addresses_of_its_own_family_only() {
    let (server, _) = serve().await;
    let port = server.port();
    let literal = [source("literal", server, "/whole", false)];
    let named: &'static str = Box::leak(format!("http://localhost:{port}/whole").into_boxed_str());
    let named = [Source {
        name: "named",
        ipv4: Some(named),
        ipv6: Some(named),
        read: network::whole,
    }];

    // The local server stands in for an IPv6 target too, so that both families ask.
    for sources in [literal, named] {
        let (network, _) = probe(&[server], &[server], &sources).await;
        assert_eq!(
            network.ipv4.address,
            Address::Found {
                address: ip("8.8.4.4"),
                source: sources[0].name
            }
        );
        // 127.0.0.1 is no IPv6 address; localhost may also be ::1, where nothing listens.
        let expected: &[Why] = if sources[0].name == "literal" {
            &[Why::NoAddress]
        } else {
            &[Why::NoAddress, Why::Unreachable]
        };
        assert!(
            matches!(
                &network.ipv6.address,
                Address::NotFound(failed)
                    if matches!(failed.as_slice(), [(_, why)] if expected.contains(why))
            ),
            "{:?}",
            network.ipv6.address
        );
    }
}

#[tokio::test]
async fn a_source_for_one_family_only_is_skipped_for_the_other() {
    let (server, seen) = serve().await;
    let only_v6 = Source {
        ipv4: None,
        ..source("v6", server, "/garbage", false)
    };
    let sources = [only_v6, source("v4", server, "/whole", false)];

    let (network, _) = probe(&[server], &[], &sources).await;

    assert_eq!(
        network.ipv4.address,
        Address::Found {
            address: ip("8.8.4.4"),
            source: "v4"
        }
    );
    let seen = seen.lock().expect("the counts").clone();
    assert_eq!(seen.get("/garbage"), None);
}

#[tokio::test]
async fn reachability_stops_at_the_first_target_that_connects() {
    let open = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let open = open.local_addr().unwrap();
    let closed = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();

    assert_eq!(
        network::reach(&[closed, open]).await,
        [(closed, Reach::Refused), (open, Reach::Connected)]
    );
    assert_eq!(
        network::reach(&[open, closed]).await,
        [(open, Reach::Connected)]
    );
    assert_eq!(network::reach(&[closed]).await, [(closed, Reach::Refused)]);
}

#[test]
fn a_failed_connection_says_whether_the_host_has_a_route() {
    for kind in [
        ErrorKind::HostUnreachable,
        ErrorKind::NetworkUnreachable,
        ErrorKind::AddrNotAvailable,
        ErrorKind::Unsupported,
    ] {
        assert_eq!(network::failure(kind), Reach::NoRoute, "{kind:?}");
    }
    assert_eq!(
        network::failure(ErrorKind::ConnectionRefused),
        Reach::Refused
    );
    assert_eq!(network::failure(ErrorKind::ConnectionReset), Reach::Failed);
}

#[test]
fn the_readers_take_the_address_out_of_each_format() {
    assert_eq!(
        network::trace("fl=1\nh=1.1.1.1\nip=203.0.113.7\nts=1\n"),
        Some("203.0.113.7")
    );
    assert_eq!(network::trace("fl=1\nh=1.1.1.1\n"), None);
    assert_eq!(network::whole("2001:db8::7\n"), Some("2001:db8::7"));
    assert_eq!(network::whole(" 203.0.113.7 "), Some("203.0.113.7"));
}

#[test]
fn only_public_addresses_of_the_family_count() {
    for public in [
        "8.8.8.8",
        "1.1.1.1",
        "61.228.1.2",
        "100.63.255.255",
        "100.128.0.1",
    ] {
        assert_eq!(network::parse(public, false), Ok(ip(public)), "{public}");
    }
    for inside in [
        "10.1.2.3",
        "172.16.0.1",
        "192.168.1.1",
        "127.0.0.1",
        "169.254.1.1",
        "100.64.0.1",
        "100.127.255.255",
        "0.1.2.3",
        "192.0.0.9",
        "192.0.2.1",
        "198.18.0.1",
        "198.19.255.255",
        "198.51.100.7",
        "203.0.113.7",
        "224.0.0.1",
        "240.0.0.1",
        "255.255.255.255",
    ] {
        assert_eq!(
            network::parse(inside, false),
            Err(Why::NotPublic),
            "{inside}"
        );
    }
    for public in [
        "2a01:4f8:c0c:1234::1",
        "2606:4700:4700::1111",
        "3fff:1000::1",
    ] {
        assert_eq!(network::parse(public, true), Ok(ip(public)), "{public}");
    }
    for inside in [
        "2001:db8::1",
        "3fff::1",
        "3fff:fff::1",
        "fe80::1",
        "fd00::1",
        "::1",
        "::ffff:8.8.8.8",
        "ff02::1",
    ] {
        assert_eq!(
            network::parse(inside, true),
            Err(Why::NotPublic),
            "{inside}"
        );
    }
    assert_eq!(network::parse("8.8.8.8", true), Err(Why::NotAnAddress));
    assert_eq!(
        network::parse("2606:4700::1", false),
        Err(Why::NotAnAddress)
    );
    assert_eq!(
        network::parse("not an address", false),
        Err(Why::NotAnAddress)
    );
    assert_eq!(network::parse("", true), Err(Why::NotAnAddress));
}

#[test]
fn masking_keeps_two_octets_or_two_groups() {
    assert_eq!(network::mask(ip("203.0.113.7")), "203.0.*.*");
    assert_eq!(
        network::mask(ip("2001:db8:85a3::8a2e:370:7334")),
        "2001:db8:*"
    );
    assert_eq!(network::mask(ip("2a01::1")), "2a01:0:*");
}
