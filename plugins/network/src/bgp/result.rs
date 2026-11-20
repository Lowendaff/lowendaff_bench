//! Structured data returned by `network.bgp`: announced prefix, origin ASNs, RPKI validity,
//! direct upstreams, deduplicated `AS_PATH` observations, and failure reasons.

use std::collections::BTreeMap;
use std::net::IpAddr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Serialize;

/// Runtime settings recorded in [`Output`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Whether `base_url` was overridden (for local loopback smoke runs).
    pub custom_endpoints: bool,
}

/// RPKI Route Origin Authorization (ROA) status for a prefix and origin ASN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RpkiStatus {
    /// Covered by a matching valid ROA.
    Valid,
    /// Conflicts with a covering ROA's ASN or max-length.
    Invalid,
    /// No covering ROA exists (`unknown` / `not-found` in RPKI validators).
    NotFound,
    /// RPKI lookup was not performed or did not answer.
    Unknown,
}

/// One origin Autonomous System seen announcing the prefix, with its collector peer count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OriginSummary {
    /// Autonomous System number.
    pub asn: u32,
    /// Number of RIS collector peers observing this origin on the prefix.
    pub peers: u32,
}

/// One direct upstream Autonomous System immediately preceding the origin ASN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamShare {
    /// Upstream Autonomous System number.
    pub asn: u32,
    /// Number of RIS collector peers whose route enters the origin through `asn`.
    pub peers: u32,
}

/// One deduplicated `AS_PATH` observed for the prefix, ordered from collector peer (`0`) to origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BgpPath {
    /// Clean ASN sequence with consecutive AS-PATH prepends collapsed.
    pub asns: Vec<u32>,
    /// Representative raw ASN sequence preserving AS-PATH prepends.
    pub asns_raw: Vec<u32>,
    /// Number of RIS collector peers observing this clean path.
    pub peers: u32,
    /// Whether this path has the highest peer support (tie-broken by shorter length).
    pub is_best: bool,
}

/// BGP routing state collected for one public IP address family (IPv4 or IPv6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyBgp {
    /// Queried public address of the host.
    pub address: IpAddr,
    /// Whether at least one collector route covers `address`.
    pub announced: bool,
    /// Encompassing announced CIDR prefix, such as `"203.0.113.0/24"`, when announced.
    pub prefix: Option<String>,
    /// Network base address of `prefix`, used by the reporter to mask host prefixes cleanly.
    #[serde(skip)]
    pub prefix_base: Option<IpAddr>,
    /// Prefix length in bits (`0..=32` for IPv4, `0..=128` for IPv6), when announced.
    pub prefix_len: Option<u8>,
    /// Primary origin ASN (highest collector peer count), when announced.
    pub origin_asn: Option<u32>,
    /// Every origin ASN seen announcing `prefix` (more than one indicates MOAS).
    pub origins: Vec<OriginSummary>,
    /// RPKI validation outcome for `(origin_asn, prefix)`.
    pub rpki: RpkiStatus,
    /// Total RIS collector peer observations on `prefix`.
    pub total_peers: u32,
    /// Total distinct clean `AS_PATH`s observed before capping `paths`.
    pub distinct_paths: usize,
    /// Direct upstream ASNs immediately before the origin, ordered by `peers` descending.
    pub upstreams: Vec<UpstreamShare>,
    /// Distinct `AS_PATH`s ordered by `peers` descending then length ascending (up to 128).
    pub paths: Vec<BgpPath>,
    /// Concise display names keyed by ASN for nodes appearing in `origins`, `upstreams` or `paths`.
    pub asn_names: BTreeMap<u32, String>,
}

/// Complete output of `network.bgp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// Effective settings of the run.
    pub settings: Settings,
    /// Whether IPv4 reached the Internet during host profiling.
    pub ipv4_reachable: bool,
    /// Whether IPv6 reached the Internet during host profiling.
    pub ipv6_reachable: bool,
    /// BGP state for the host's public IPv4 address, when available.
    pub ipv4: Option<Box<FamilyBgp>>,
    /// BGP state for the host's public IPv6 address, when available.
    pub ipv6: Option<Box<FamilyBgp>>,
    /// Per-family notes `(ipv6, reason)` when one family's lookup failed while the other succeeded.
    pub notes: Vec<(bool, String)>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `network.bgp` skipped or failed without producing a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The configured `base_url` override is neither `https://` nor loopback `http://`.
    InvalidEndpoint(String),
    /// Neither IPv4 nor IPv6 is reachable on the host.
    NoFamily,
    /// Network is reachable, but no public IPv4 or IPv6 address was discovered.
    NoPublicAddress,
    /// Every attempted `bgp-state` query failed or returned malformed JSON.
    QueryFailed(String),
    /// Cancelled by timeout or `Ctrl-C`.
    Cancelled,
}
