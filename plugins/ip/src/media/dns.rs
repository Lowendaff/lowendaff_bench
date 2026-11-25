//! Native vs DNS-hijacked (`SmartDNS` / `SNI proxy`) unlock classification for unlocked streaming
//! platforms, using differential DNS checks without external host utilities.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::Context;
use bench_api::DnsRecords;
use bench_api::HttpClient;
use bench_api::HttpFuture;
use bench_api::HttpRequest;
use bench_api::IpFamily;

use super::result::Output;
use super::result::ServiceResult;
use super::result::UnlockMode;

/// Per-query timeout for DNS unlock verification.
const DNS_TIMEOUT: Duration = Duration::from_millis(2500);

/// Concurrent DNS verification tasks in flight at once.
const DNS_CONCURRENCY: usize = 10;

/// Public IPv4 recursive resolvers used as an un-hijacked DNS baseline (`Cloudflare` and `Google`).
const BASELINE_V4: [SocketAddr; 2] = [
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 53),
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53),
];

/// Public IPv6 recursive resolvers used when IPv4 is unavailable on an IPv6-only host.
const BASELINE_V6: [SocketAddr; 2] = [
    SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111),
        53,
        0,
        0,
    )),
    SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888),
        53,
        0,
        0,
    )),
];

/// Non-existent subdomain prefix queried to detect `dnsmasq` / `SmartDNS` wildcard rules.
const WILDCARD_PREFIX: &str = "lowendaff-nx-check";

/// Wraps an [`HttpClient`] to record the unique DNS hostnames contacted during one platform probe.
pub struct HostRecorder {
    inner: Arc<dyn HttpClient>,
    hosts: Mutex<Vec<String>>,
}

impl HostRecorder {
    /// Wraps `inner` in a new [`HostRecorder`].
    #[must_use]
    pub fn new(inner: Arc<dyn HttpClient>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            hosts: Mutex::new(Vec::new()),
        })
    }

    /// Returns the deduplicated DNS hostnames observed during the probe.
    #[must_use]
    pub fn take_hosts(&self) -> Vec<String> {
        match self.hosts.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(_) => Vec::new(),
        }
    }

    fn record_url(&self, url: &str) {
        let Some(host) = extract_dns_host(url) else {
            return;
        };
        if let Ok(mut guard) = self.hosts.lock()
            && !guard.contains(&host)
        {
            guard.push(host);
        }
    }
}

impl HttpClient for HostRecorder {
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
        self.record_url(&request.url);
        Box::pin(async move {
            let outcome = self.inner.send(request).await;
            if let Ok(res) = &outcome {
                self.record_url(&res.final_url);
            }
            outcome
        })
    }
}

/// Extracts a lowercase DNS hostname from `url`, returning `None` for IP literals or malformed
/// authorities.
#[must_use]
pub fn extract_dns_host(url: &str) -> Option<String> {
    let (_, after_scheme) = url.trim().split_once("://")?;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() || authority.starts_with('[') {
        return None;
    }
    let host = match authority.find(':') {
        Some(idx) => &authority[..idx],
        None => authority,
    };
    let clean = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if clean.is_empty() || clean.parse::<IpAddr>().is_ok() {
        return None;
    }
    Some(clean)
}

/// Narrow subnet bucket (`/24` for IPv4, `/48` for IPv6) used to compare CDN edge pools and detect
/// shared SNI-proxy addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Subnet24Or48 {
    V4([u8; 3]),
    V6([u16; 3]),
}

impl Subnet24Or48 {
    fn of(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(v4) => {
                let [a, b, c, _] = v4.octets();
                Self::V4([a, b, c])
            }
            IpAddr::V6(v6) => {
                let [s0, s1, s2, ..] = v6.segments();
                Self::V6([s0, s1, s2])
            }
        }
    }
}

/// Wide network bucket (`/16` for IPv4, `/32` for IPv6) used when checking apex-domain divergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WideNet {
    V4([u8; 2]),
    V6([u16; 2]),
}

impl WideNet {
    fn of(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(v4) => {
                let [a, b, ..] = v4.octets();
                Self::V4([a, b])
            }
            IpAddr::V6(v6) => {
                let [s0, s1, ..] = v6.segments();
                Self::V6([s0, s1])
            }
        }
    }
}

/// Outcome of inspecting a single DNS hostname over one IP family.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostCheck {
    /// Definitively hijacked via `/etc/hosts`, private/CGNAT IP, wildcard rule, or CNAME strip.
    Dns,
    /// Aligned with public DNS (`CNAME` preserved or overlapping `/24`/`/48` subnet).
    Native,
    /// Apex domain without `CNAME` whose system DNS diverged beyond `/16` (`/32`) from public DNS;
    /// carries its registrable root and `/24` (`/48`) subnets to detect cross-service SNI proxies.
    DivergentApex {
        root: String,
        subnets: Vec<Subnet24Or48>,
    },
    /// System or baseline DNS lookup failed or returned no records.
    Unknown,
}

/// Stateful per-family DNS-unlock verifier across continents within one `ip.media` run.
#[derive(Default)]
pub struct FamilyDnsState {
    cache: BTreeMap<String, HostCheck>,
    /// Maps a divergent apex subnet to `(slot_index, registrable_root)` entries seen so far.
    apex_seen: BTreeMap<Subnet24Or48, Vec<(usize, String)>>,
}

impl FamilyDnsState {
    /// Verifies `targets` (`(slot_idx, hostnames)`) over `family` and updates `slots` in place,
    /// including retroactive SNI-proxy convergence upgrades across continents.
    pub async fn classify_unlocked(
        &mut self,
        ctx: &Context<Output>,
        family: IpFamily,
        ipv4_reachable: bool,
        ipv6_reachable: bool,
        targets: &[(usize, Vec<String>)],
        slots: &mut [ServiceResult],
    ) {
        let baselines: &[SocketAddr] = if ipv4_reachable {
            &BASELINE_V4
        } else if ipv6_reachable {
            &BASELINE_V6
        } else {
            return;
        };

        let mut uncached = BTreeSet::new();
        for (_, hosts) in targets {
            for host in hosts {
                if !self.cache.contains_key(host) {
                    uncached.insert(host.clone());
                }
            }
        }

        if !uncached.is_empty() {
            let cancel = ctx.cancel_token();
            let tasks = uncached.into_iter().map(|host| async move {
                let verdict = inspect_host(ctx, family, &host, baselines).await;
                (host, verdict)
            });
            let resolved = bench_api::run_bounded(DNS_CONCURRENCY, &cancel, tasks).await;
            if cancel.is_cancelled() {
                return;
            }
            for (host, check) in resolved {
                self.cache.insert(host, check);
            }
        }

        let mut retro_dns_slots = BTreeSet::new();
        for (slot_idx, hosts) in targets {
            let mode = self.evaluate_service(*slot_idx, hosts, &mut retro_dns_slots);
            set_slot_unlock(&mut slots[*slot_idx], family, mode);
        }
        for idx in retro_dns_slots {
            set_slot_unlock(&mut slots[idx], family, Some(UnlockMode::Dns));
        }
    }

    fn evaluate_service(
        &mut self,
        slot_idx: usize,
        hosts: &[String],
        retro_dns_slots: &mut BTreeSet<usize>,
    ) -> Option<UnlockMode> {
        let mut any_known = false;
        for host in hosts {
            let Some(check) = self.cache.get(host).cloned() else {
                continue;
            };
            match check {
                HostCheck::Dns => return Some(UnlockMode::Dns),
                HostCheck::Native => {
                    any_known = true;
                }
                HostCheck::DivergentApex { root, subnets } => {
                    any_known = true;
                    let mut converged = false;
                    for subnet in subnets {
                        let entries = self.apex_seen.entry(subnet).or_default();
                        for (prev_idx, prev_root) in entries.iter() {
                            if prev_root != &root {
                                converged = true;
                                retro_dns_slots.insert(*prev_idx);
                            }
                        }
                        if !entries.iter().any(|(idx, _)| *idx == slot_idx) {
                            entries.push((slot_idx, root.clone()));
                        }
                    }
                    if converged {
                        return Some(UnlockMode::Dns);
                    }
                }
                HostCheck::Unknown => {}
            }
        }
        any_known.then_some(UnlockMode::Native)
    }
}

fn set_slot_unlock(slot: &mut ServiceResult, family: IpFamily, mode: Option<UnlockMode>) {
    match family {
        IpFamily::Ipv4 => slot.ipv4_unlock = mode,
        IpFamily::Ipv6 => slot.ipv6_unlock = mode,
    }
}

async fn inspect_host(
    ctx: &Context<Output>,
    family: IpFamily,
    host: &str,
    baselines: &[SocketAddr],
) -> HostCheck {
    let Ok(sys) = ctx.dns_lookup(family, host, None, DNS_TIMEOUT).await else {
        return HostCheck::Unknown;
    };
    if sys.addresses.is_empty() {
        return HostCheck::Unknown;
    }
    if sys.from_hosts
        || sys
            .addresses
            .iter()
            .copied()
            .any(|ip| !bench_api::is_routable_endpoint_ip(ip))
    {
        return HostCheck::Dns;
    }

    let nx_qname = format!("{WILDCARD_PREFIX}.{host}");
    let (sys_wc, pub_res) = tokio::join!(
        ctx.dns_lookup(family, &nx_qname, None, DNS_TIMEOUT),
        lookup_baseline(ctx, family, host, baselines),
    );

    if let Ok(wc) = sys_wc
        && !wc.addresses.is_empty()
    {
        match lookup_baseline(ctx, family, &nx_qname, baselines).await {
            Some(pub_wc) if pub_wc.addresses.is_empty() => return HostCheck::Dns,
            None if !sys.has_cname && shares_narrow(&wc.addresses, &sys.addresses) => {
                return HostCheck::Dns;
            }
            _ => {}
        }
    }

    let Some(pub_rec) = pub_res else {
        return HostCheck::Unknown;
    };
    if pub_rec.addresses.is_empty() {
        return HostCheck::Unknown;
    }

    classify_records(host, &sys, &pub_rec)
}

async fn lookup_baseline(
    ctx: &Context<Output>,
    family: IpFamily,
    qname: &str,
    baselines: &[SocketAddr],
) -> Option<DnsRecords> {
    for &server in baselines {
        if let Ok(records) = ctx
            .dns_lookup(family, qname, Some(server), DNS_TIMEOUT)
            .await
        {
            return Some(records);
        }
    }
    None
}

fn classify_records(host: &str, sys: &DnsRecords, pub_rec: &DnsRecords) -> HostCheck {
    if shares_narrow(&sys.addresses, &pub_rec.addresses) {
        return HostCheck::Native;
    }
    if pub_rec.has_cname && !sys.has_cname {
        return HostCheck::Dns;
    }
    if !pub_rec.has_cname && !shares_wide(&sys.addresses, &pub_rec.addresses) {
        let mut subnets = Vec::new();
        for &ip in &sys.addresses {
            let sub = Subnet24Or48::of(ip);
            if !subnets.contains(&sub) {
                subnets.push(sub);
            }
        }
        return HostCheck::DivergentApex {
            root: registrable_root(host),
            subnets,
        };
    }
    HostCheck::Native
}

fn shares_narrow(left: &[IpAddr], right: &[IpAddr]) -> bool {
    for &a in left {
        let sub_a = Subnet24Or48::of(a);
        for &b in right {
            if sub_a == Subnet24Or48::of(b) {
                return true;
            }
        }
    }
    false
}

fn shares_wide(left: &[IpAddr], right: &[IpAddr]) -> bool {
    for &a in left {
        let net_a = WideNet::of(a);
        for &b in right {
            if net_a == WideNet::of(b) {
                return true;
            }
        }
    }
    false
}

/// Heuristic registrable suffix (e.g. `ani.gamer.com.tw` -> `gamer.com.tw`,
/// `www.netflix.com` -> `netflix.com`) so subdomains of the same service never trigger
/// cross-service convergence against each other.
fn registrable_root(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 {
        return host.to_owned();
    }
    let sld = labels[labels.len() - 2];
    let tld = labels[labels.len() - 1];
    let multipart_cctld = tld.len() == 2
        && matches!(
            sld,
            "co" | "com" | "net" | "org" | "gov" | "edu" | "ac" | "ne" | "or"
        );
    let keep = if multipart_cctld && labels.len() >= 3 {
        3
    } else {
        2
    };
    labels[labels.len() - keep..].join(".")
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use bench_api::DnsRecords;

    use super::HostCheck;

    #[test]
    fn extract_dns_host_and_registrable_root_handle_ports_ipv6_and_cctlds() {
        assert_eq!(
            super::extract_dns_host("https://WWW.Netflix.com:443/title/81280792").as_deref(),
            Some("www.netflix.com")
        );
        assert_eq!(super::extract_dns_host("http://127.0.0.1:18080/x"), None);
        assert_eq!(super::extract_dns_host("http://[::1]:18080/x"), None);
        assert_eq!(super::registrable_root("www.netflix.com"), "netflix.com");
        assert_eq!(super::registrable_root("ani.gamer.com.tw"), "gamer.com.tw");
        assert_eq!(super::registrable_root("www.bbc.co.uk"), "bbc.co.uk");
    }

    #[test]
    fn classify_records_detects_cname_stripping_preserves_geodns_and_flags_divergent_apex() {
        let cdn_a: IpAddr = "198.51.100.10".parse().expect("ip");
        let cdn_b: IpAddr = "203.0.113.20".parse().expect("ip");
        let sni_proxy: IpAddr = "192.0.2.99".parse().expect("ip");

        // GeoDNS: different /24s, but both preserve CNAME -> Native!
        let sys_geodns = DnsRecords {
            addresses: vec![cdn_a],
            has_cname: true,
            from_hosts: false,
        };
        let pub_cname = DnsRecords {
            addresses: vec![cdn_b],
            has_cname: true,
            from_hosts: false,
        };
        assert_eq!(
            super::classify_records("www.netflix.com", &sys_geodns, &pub_cname),
            HostCheck::Native
        );

        // SNI proxy override: public DNS has CNAME, system DNS stripped CNAME + disjoint /24.
        let sys_stripped = DnsRecords {
            addresses: vec![sni_proxy],
            has_cname: false,
            from_hosts: false,
        };
        assert_eq!(
            super::classify_records("www.netflix.com", &sys_stripped, &pub_cname),
            HostCheck::Dns
        );

        // Apex domain diverging across /16 -> DivergentApex candidate for convergence check.
        let pub_apex = DnsRecords {
            addresses: vec![cdn_b],
            has_cname: false,
            from_hosts: false,
        };
        assert!(matches!(
            super::classify_records("abema.tv", &sys_stripped, &pub_apex),
            HostCheck::DivergentApex { .. }
        ));
    }
}
