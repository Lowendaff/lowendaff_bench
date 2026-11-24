//! Curated DNSBL (DNS-based Blackhole List) zones, IPv4/IPv6 reverse-label construction (RFC
//! 5782), and policy-refusal filtering (`127.255.255.0/24`).

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;

use bench_api::Context;
use bench_api::DnsAnswer;
use bench_api::NetError;

use super::result::DnsblSummary;
use super::result::Output;

/// Per-zone DNSBL lookup timeout.
const DNSBL_TIMEOUT: Duration = Duration::from_secs(4);

/// Concurrent DNSBL queries in flight at once.
pub const DNSBL_CONCURRENCY: usize = 12;

/// Static specification of one public DNSBL zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsblZone {
    /// Short display name shown when listed.
    pub name: &'static str,
    /// DNS zone suffix, such as `zen.spamhaus.org`.
    pub zone: &'static str,
    /// Whether the zone supports IPv4 (`a.b.c.d`) lookups.
    pub ipv4: bool,
    /// Whether the zone supports IPv6 nibble lookups.
    pub ipv6: bool,
}

/// Outcome of querying one [`DnsblZone`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneVerdict {
    /// `NXDOMAIN` or `NODATA`: not listed.
    Clean,
    /// Returned a valid `127.x.y.z` listing address outside `127.255.255.0/24`.
    Listed,
    /// Resolver refused (`127.255.255.0/24`), hijacked non-`127.*`, or timed out.
    Refused,
}

/// 25 curated, actively maintained public DNSBL zones (24 covering IPv4, 9 covering IPv6).
pub const ZONES: [DnsblZone; 25] = [
    DnsblZone {
        name: "Spamhaus ZEN",
        zone: "zen.spamhaus.org",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "SpamCop",
        zone: "bl.spamcop.net",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "Barracuda",
        zone: "b.barracudacentral.org",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "SORBS",
        zone: "dnsbl.sorbs.net",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "DroneBL",
        zone: "dnsbl.dronebl.org",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "Blocklist.de",
        zone: "bl.blocklist.de",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "PSBL",
        zone: "psbl.surriel.com",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "S5H",
        zone: "all.s5h.net",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "InterServer",
        zone: "rbl.interserver.net",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "SpamRats Dyna",
        zone: "dyna.spamrats.com",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "SpamRats NoPTR",
        zone: "noptr.spamrats.com",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "SpamRats Spam",
        zone: "spam.spamrats.com",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "UCEPROTECT L1",
        zone: "dnsbl-1.uceprotect.net",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "LashBack UBL",
        zone: "ubl.unsubscore.com",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "Backscatterer",
        zone: "ips.backscatterer.org",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "0spam",
        zone: "bl.0spam.org",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "NiX Spam",
        zone: "ix.dnsbl.manitu.net",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "Mailspike",
        zone: "bl.mailspike.net",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "WPBL",
        zone: "db.wpbl.info",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "SPFBL",
        zone: "dnsbl.spfbl.net",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "SEM-BLACK",
        zone: "bl.spameatingmonkey.net",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "SEM-IPV6",
        zone: "bl.ipv6.spameatingmonkey.net",
        ipv4: false,
        ipv6: true,
    },
    DnsblZone {
        name: "NordSpam",
        zone: "bl.nordspam.com",
        ipv4: true,
        ipv6: true,
    },
    DnsblZone {
        name: "WormRBL",
        zone: "wormrbl.imp.ch",
        ipv4: true,
        ipv6: false,
    },
    DnsblZone {
        name: "GBUdb Truncate",
        zone: "truncate.gbudb.net",
        ipv4: true,
        ipv6: false,
    },
];

/// Zones applicable to `ip`'s family (24 for IPv4, 9 for IPv6).
#[must_use]
pub fn zones_for(ip: IpAddr) -> Vec<DnsblZone> {
    let ipv6 = ip.is_ipv6();
    ZONES
        .into_iter()
        .filter(|zone| if ipv6 { zone.ipv6 } else { zone.ipv4 })
        .collect()
}

/// Formats the reversed DNSBL query name for `ip` under `zone` per RFC 5782.
#[must_use]
pub fn query_name(ip: IpAddr, zone: &str) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, d] = v4.octets();
            format!("{d}.{c}.{b}.{a}.{zone}")
        }
        IpAddr::V6(v6) => {
            let mut out = String::with_capacity(64 + zone.len());
            let octets = v6.octets();
            let reversed = octets.iter().rev();
            for &byte in reversed {
                let low = byte & 0x0f;
                let high = byte >> 4;
                out.push(hex_nibble(low));
                out.push('.');
                out.push(hex_nibble(high));
                out.push('.');
            }
            out.push_str(zone);
            out
        }
    }
}

const fn hex_nibble(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + (nibble - 10)) as char,
    }
}

/// Classifies a DNSBL lookup result, filtering out `127.255.255.0/24` open-resolver policy codes
/// and non-`127.0.0.0/8` hijacked responses so neither causes a false blacklist hit.
#[must_use]
pub fn classify(result: &Result<DnsAnswer, NetError>) -> ZoneVerdict {
    match result {
        Ok(DnsAnswer::NotFound) => ZoneVerdict::Clean,
        Ok(DnsAnswer::Addresses(addrs)) => {
            if addrs.iter().copied().any(is_listing_address) {
                ZoneVerdict::Listed
            } else {
                ZoneVerdict::Refused
            }
        }
        Err(_) => ZoneVerdict::Refused,
    }
}

/// Whether `addr` is an authentic DNSBL listing code (`127.0.0.2..=127.255.254.255`).
#[must_use]
pub const fn is_listing_address(addr: Ipv4Addr) -> bool {
    let [a, b, c, d] = addr.octets();
    if a != 127 {
        return false;
    }
    if b == 0 && c == 0 && d <= 1 {
        return false;
    }
    !(b == 255 && c == 255)
}

/// Queries one [`DnsblZone`] for `ip`.
pub async fn check_zone(
    ctx: &Context<Output>,
    ip: IpAddr,
    spec: DnsblZone,
    dns_server: Option<SocketAddr>,
) -> (&'static str, ZoneVerdict) {
    let qname = query_name(ip, spec.zone);
    let outcome = ctx.dns_a(&qname, dns_server, DNSBL_TIMEOUT).await;
    (spec.name, classify(&outcome))
}

/// Summarizes zone verdicts in roster order.
#[must_use]
pub fn summarize(outcomes: Vec<(&'static str, ZoneVerdict)>) -> DnsblSummary {
    let mut checked = 0;
    let mut refused = 0;
    let mut listed = Vec::new();
    for (name, verdict) in outcomes {
        match verdict {
            ZoneVerdict::Clean => checked += 1,
            ZoneVerdict::Listed => {
                checked += 1;
                listed.push(name);
            }
            ZoneVerdict::Refused => refused += 1,
        }
    }
    DnsblSummary {
        checked,
        refused,
        listed,
    }
}
