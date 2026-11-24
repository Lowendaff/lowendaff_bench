//! The network module: whether each address family reaches the Internet, and the public address
//! that outside services see. It is the only part of the host profile that connects anywhere,
//! and it fills in the profile's network part (`bench_host::network`).
//!
//! Reachability is a TCP handshake with port 443 of any target, 3 s each, the targets of a
//! family one after another and the two families at once. For a reachable family the sources
//! are asked in order, until one answers with a public address of that family: one client per
//! family, bound to the family's unspecified local address, so that it connects to addresses of
//! that family only, and without a proxy, which would report its own address.

use std::io::ErrorKind;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::network::Why;
use reqwest::Client;
use reqwest::Url;
use tokio::net::TcpStream;

use crate::fetcher;
use crate::locale;
use crate::report;

/// Public DNS anycast addresses, Cloudflare's and AliDNS's, on the HTTPS port.
pub const IPV4_TARGETS: [SocketAddr; 2] = [
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443),
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(223, 5, 5, 5)), 443),
];

/// The same services over IPv6.
pub const IPV6_TARGETS: [SocketAddr; 2] = [
    SocketAddr::new(
        IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111)),
        443,
    ),
    SocketAddr::new(
        IpAddr::V6(Ipv6Addr::new(0x2400, 0x3200, 0, 0, 0, 0, 0, 1)),
        443,
    ),
];

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long a source has to answer.
const SOURCE_TIMEOUT: Duration = Duration::from_secs(5);

/// How much of an answer is read; an address is far shorter.
const BODY_LIMIT: usize = 4096;

/// A service that answers with the address it sees.
#[derive(Debug, Clone, Copy)]
pub struct Source {
    /// The name the report gives.
    pub name: &'static str,
    /// Where to ask over IPv4, when the service answers over IPv4.
    pub ipv4: Option<&'static str>,
    /// Where to ask over IPv6, when the service answers over IPv6.
    pub ipv6: Option<&'static str>,
    /// Takes the address's text out of an answer.
    pub read: fn(&str) -> Option<&str>,
}

/// The sources, in the order they are asked. Cloudflare's addresses need no DNS.
pub const SOURCES: [Source; 3] = [
    Source {
        name: "Cloudflare",
        ipv4: Some("https://1.1.1.1/cdn-cgi/trace"),
        ipv6: Some("https://[2606:4700:4700::1111]/cdn-cgi/trace"),
        read: trace,
    },
    Source {
        name: "ipify",
        ipv4: Some("https://api.ipify.org"),
        ipv6: Some("https://api6.ipify.org"),
        read: whole,
    },
    Source {
        name: "icanhazip",
        ipv4: Some("https://ipv4.icanhazip.com"),
        ipv6: Some("https://ipv6.icanhazip.com"),
        read: whole,
    },
];

/// Cloudflare's trace: one `key=value` per line, and the address is the `ip=` line.
#[must_use]
pub fn trace(body: &str) -> Option<&str> {
    body.lines()
        .find_map(|line| line.strip_prefix("ip="))
        .map(str::trim)
}

/// The whole answer is the address, as ipify and icanhazip give it.
#[must_use]
pub fn whole(body: &str) -> Option<&str> {
    Some(body.trim())
}

/// Probes both families at once, with the program's targets and sources.
pub async fn probe() -> Network {
    probe_until(&CancellationToken::new()).await
}

/// Probes both families at once, stopping early when `cancel` fires.
pub async fn probe_until(cancel: &CancellationToken) -> Network {
    probe_with_until(&IPV4_TARGETS, &IPV6_TARGETS, &SOURCES, cancel).await
}

/// Probes both families at once.
pub async fn probe_with(ipv4: &[SocketAddr], ipv6: &[SocketAddr], sources: &[Source]) -> Network {
    probe_with_until(ipv4, ipv6, sources, &CancellationToken::new()).await
}

/// Probes both families at once, stopping early when `cancel` fires.
pub async fn probe_with_until(
    ipv4: &[SocketAddr],
    ipv6: &[SocketAddr],
    sources: &[Source],
    cancel: &CancellationToken,
) -> Network {
    let (ipv4, ipv6) = tokio::join!(
        family(false, ipv4, sources, cancel),
        family(true, ipv6, sources, cancel)
    );
    Network { ipv4, ipv6 }
}

async fn family(
    v6: bool,
    targets: &[SocketAddr],
    sources: &[Source],
    cancel: &CancellationToken,
) -> Family {
    let targets = reach_until(targets, cancel).await;
    let address =
        if !cancel.is_cancelled() && targets.iter().any(|(_, reach)| *reach == Reach::Connected) {
            address(v6, sources, cancel).await
        } else {
            Address::NotAsked
        };
    Family { targets, address }
}

/// Tries the targets one after another, until a handshake completes.
pub async fn reach(targets: &[SocketAddr]) -> Vec<(SocketAddr, Reach)> {
    reach_until(targets, &CancellationToken::new()).await
}

async fn reach_until(
    targets: &[SocketAddr],
    cancel: &CancellationToken,
) -> Vec<(SocketAddr, Reach)> {
    let mut results = Vec::new();
    for target in targets {
        if cancel.is_cancelled() {
            break;
        }
        let attempt = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(target));
        let reach = tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            res = attempt => match res {
                Ok(Ok(_)) => Reach::Connected,
                Err(_) => Reach::TimedOut,
                Ok(Err(err)) => failure(err.kind()),
            },
        };
        results.push((*target, reach));
        if reach == Reach::Connected {
            break;
        }
    }
    results
}

/// What a failed connection says about the target.
#[must_use]
pub fn failure(kind: ErrorKind) -> Reach {
    match kind {
        ErrorKind::ConnectionRefused => Reach::Refused,
        // A family without an address or a route on this host.
        ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::AddrNotAvailable
        | ErrorKind::Unsupported => Reach::NoRoute,
        _ => Reach::Failed,
    }
}

/// Asks the sources of a family in order, until one gives a public address.
async fn address(v6: bool, sources: &[Source], cancel: &CancellationToken) -> Address {
    let client = client(v6);
    let mut failed = Vec::new();
    for source in sources {
        if cancel.is_cancelled() {
            break;
        }
        let Some(url) = (if v6 { source.ipv6 } else { source.ipv4 }) else {
            continue;
        };
        let answer = match &client {
            Some(client) => tokio::select! {
                biased;
                () = cancel.cancelled() => break,
                res = ask(client, url, source.read, v6) => res,
            },
            None => Err(Why::Unreachable),
        };
        let name = if v6 { "IPv6" } else { "IPv4" };
        match answer {
            Ok(address) => {
                let masked = mask(address);
                tracing::debug!(
                    "{}",
                    locale::words().network_source(source.name, name, &masked)
                );
                return Address::Found {
                    address,
                    source: source.name,
                };
            }
            Err(why) => {
                let text = report::terminal::why_text(source.name, why, name);
                tracing::debug!("{}", locale::words().network_note(&text));
                failed.push((source.name, why));
            }
        }
    }
    if failed.is_empty() {
        Address::NotAsked
    } else {
        Address::NotFound(failed)
    }
}

/// The family's base client builder; `None` when its TLS configuration is refused.
fn client_builder(v6: bool) -> Option<reqwest::ClientBuilder> {
    let local = if v6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };
    Some(
        fetcher::builder()
            .ok()?
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .local_address(local),
    )
}

/// The family's client; `None` when its TLS configuration is refused, which leaves every
/// source unreachable.
fn client(v6: bool) -> Option<Client> {
    client_builder(v6)?.build().ok()
}

async fn ask(
    client: &Client,
    url: &str,
    read: fn(&str) -> Option<&str>,
    v6: bool,
) -> Result<IpAddr, Why> {
    let body = tokio::time::timeout(SOURCE_TIMEOUT, fetch(client, url, v6))
        .await
        .map_err(|_| Why::TimedOut)??;
    parse(read(&body).unwrap_or_default(), v6)
}

/// The first [`BODY_LIMIT`] bytes of the answer, pinning resolved DNS addresses so name lookup is
/// performed once and `https://` sources never connect to non-public addresses.
async fn fetch(client: &Client, url: &str, v6: bool) -> Result<String, Why> {
    let parsed = Url::parse(url).map_err(|_| Why::Unreachable)?;
    let require_public = match parsed.scheme() {
        "https" => true,
        "http" => false,
        _ => return Err(Why::Unreachable),
    };
    let pinned_client;
    let active_client = match resolve_source_addrs(&parsed, v6, require_public).await? {
        SourceTarget::Literal => client,
        SourceTarget::Pinned { host, addrs } => {
            pinned_client = client_builder(v6)
                .ok_or(Why::Unreachable)?
                .resolve_to_addrs(&host, &addrs)
                .build()
                .map_err(|_| Why::Unreachable)?;
            &pinned_client
        }
    };
    let mut response = active_client
        .get(parsed)
        .send()
        .await
        .map_err(|_| Why::Unreachable)?;
    let status = response.status();
    if !status.is_success() {
        return Err(Why::Status(status.as_u16()));
    }
    let mut body = Vec::new();
    while body.len() < BODY_LIMIT {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let remaining = BODY_LIMIT - body.len();
                let take = remaining.min(chunk.len());
                body.extend_from_slice(&chunk[..take]);
            }
            Ok(None) => break,
            Err(_) => return Err(Why::Unreachable),
        }
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

enum SourceTarget {
    Literal,
    Pinned {
        host: String,
        addrs: Vec<SocketAddr>,
    },
}

/// Resolves and validates `url`'s target addresses for `v6` once.
async fn resolve_source_addrs(
    url: &Url,
    v6: bool,
    require_public: bool,
) -> Result<SourceTarget, Why> {
    let clean = url
        .host_str()
        .ok_or(Why::Unreachable)?
        .trim_matches(['[', ']']);
    if let Ok(ip) = clean.parse::<IpAddr>() {
        if ip.is_ipv6() != v6 {
            return Err(Why::NoAddress);
        }
        let permitted = if require_public {
            bench_api::is_routable_endpoint_ip(ip)
        } else {
            ip.is_loopback()
        };
        if !permitted {
            return Err(Why::Unreachable);
        }
        return Ok(SourceTarget::Literal);
    }
    let port = url.port_or_known_default().ok_or(Why::Unreachable)?;
    let family_addrs: Vec<SocketAddr> = tokio::net::lookup_host((clean, port))
        .await
        .map_err(|_| Why::Unreachable)?
        .filter(|address| address.is_ipv6() == v6)
        .collect();
    if family_addrs.is_empty() {
        return Err(Why::NoAddress);
    }
    let allowed: Vec<SocketAddr> = family_addrs
        .into_iter()
        .filter(|addr| {
            if require_public {
                bench_api::is_routable_endpoint_ip(addr.ip())
            } else {
                addr.ip().is_loopback()
            }
        })
        .collect();
    if allowed.is_empty() {
        return Err(Why::Unreachable);
    }
    Ok(SourceTarget::Pinned {
        host: clean.to_owned(),
        addrs: allowed,
    })
}

/// The address in an answer, when it is a public address of the family.
pub fn parse(text: &str, v6: bool) -> Result<IpAddr, Why> {
    let address: IpAddr = text.parse().map_err(|_| Why::NotAnAddress)?;
    if address.is_ipv6() != v6 {
        Err(Why::NotAnAddress)
    } else if public(address) {
        Ok(address)
    } else {
        Err(Why::NotPublic)
    }
}

/// Whether the Internet routes the address to a host: not a private, shared, loopback,
/// link-local, documentation, benchmarking, multicast or reserved address. IPv6 must be global
/// unicast, `2000::/3`, outside the documentation prefixes `2001:db8::/32` and `3fff::/20`.
#[must_use]
pub fn public(address: IpAddr) -> bool {
    if !bench_api::is_routable_endpoint_ip(address) {
        return false;
    }
    match address {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_documentation()
                // IETF protocol assignments, 192.0.0.0/24.
                || (a == 192 && b == 0 && c == 0)
                // Benchmarking, 198.18.0.0/15.
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => {
            let [first, second, ..] = v6.segments();
            !(first == 0x2001 && (second < 0x0200 || second == 0x0db8))
                && !(first == 0x3fff && second < 0x1000)
                && first != 0x2002
        }
    }
}

/// An address with its host part hidden: IPv4 keeps two octets (`203.0.*.*`) and IPv6 its first
/// two groups, 32 bits (`2001:db8:*`). The report and the progress log both write addresses
/// through it.
#[must_use]
pub fn mask(address: IpAddr) -> String {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            format!("{a}.{b}.*.*")
        }
        IpAddr::V6(v6) => {
            let [first, second, ..] = v6.segments();
            format!("{first:x}:{second:x}:*")
        }
    }
}
