//! Pure parsers for RIPEstat `bgp-state`, `rpki-validation`, and `as-overview` JSON payloads.

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::Deserialize;
use serde_json::Value;

use super::asns;
use super::result::BgpPath;
use super::result::OriginSummary;
use super::result::RpkiStatus;
use super::result::UpstreamShare;

/// Maximum number of distinct `AS_PATH` entries retained per prefix, matching `peer.as`.
pub const MAX_PATHS: usize = 128;

/// Parsed BGP routing state for one prefix before RPKI and ASN-name enrichment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBgpState {
    /// Whether any RIS route covered the queried IP.
    pub announced: bool,
    /// Selected most-specific CIDR string, such as `"1.1.1.0/24"`.
    pub prefix: Option<String>,
    /// Parsed network base address of `prefix`.
    pub prefix_base: Option<IpAddr>,
    /// Prefix length in bits.
    pub prefix_len: Option<u8>,
    /// Primary origin ASN.
    pub origin_asn: Option<u32>,
    /// All origin ASNs ordered by peer count descending.
    pub origins: Vec<OriginSummary>,
    /// Total RIS peer observations on the selected prefix.
    pub total_peers: u32,
    /// Total distinct clean paths before truncation to [`MAX_PATHS`].
    pub distinct_paths: usize,
    /// Direct upstream ASNs immediately before the origin, ordered by peer count descending.
    pub upstreams: Vec<UpstreamShare>,
    /// Top distinct paths (up to [`MAX_PATHS`]).
    pub paths: Vec<BgpPath>,
}

#[derive(Deserialize)]
struct BgpStateEnvelope {
    status: Option<String>,
    data: Option<BgpStateData>,
}

#[derive(Deserialize)]
struct BgpStateData {
    resource: Option<String>,
    #[serde(default)]
    bgp_state: Vec<RawRoute>,
}

#[derive(Deserialize)]
struct RawRoute {
    target_prefix: Option<String>,
    #[serde(default)]
    path: Vec<Value>,
}

#[derive(Deserialize)]
struct RpkiEnvelope {
    data: Option<RpkiData>,
}

#[derive(Deserialize)]
struct RpkiData {
    status: Option<String>,
}

#[derive(Deserialize)]
struct AsOverviewEnvelope {
    data: Option<AsOverviewData>,
}

#[derive(Deserialize)]
struct AsOverviewData {
    holder: Option<String>,
}

#[derive(Default)]
struct PathBucket {
    peers: u32,
    raw_variants: BTreeMap<Vec<u32>, u32>,
}

/// Parses a CIDR string such as `"203.0.113.0/24"` or `"2001:db8::/48"`.
#[must_use]
pub fn parse_cidr(text: &str) -> Option<(IpAddr, u8)> {
    let (addr_str, len_str) = text.trim().split_once('/')?;
    let addr: IpAddr = addr_str.parse().ok()?;
    let len: u8 = len_str.parse().ok()?;
    let max_len = if addr.is_ipv6() { 128 } else { 32 };
    (len <= max_len).then_some((addr, len))
}

/// Collapses consecutive duplicate ASNs (AS-PATH prepending) while preserving hop order.
#[must_use]
pub fn clean_path(raw: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(raw.len());
    for &asn in raw {
        if out.last().copied() != Some(asn) {
            out.push(asn);
        }
    }
    out
}

/// Parses a RIPEstat `bgp-state` JSON payload into deduplicated paths, upstreams, and origins.
pub fn parse_bgp_state(body: &[u8]) -> Result<ParsedBgpState, String> {
    let env: BgpStateEnvelope =
        serde_json::from_slice(body).map_err(|err| format!("invalid bgp-state JSON: {err}"))?;
    if let Some(status) = env.status.as_deref()
        && status != "ok"
    {
        return Err(format!("bgp-state status `{status}`"));
    }
    let Some(data) = env.data else {
        return Err("bgp-state payload lacks `data`".to_owned());
    };

    // Group routes by target_prefix so that, when both a covering supernet and a more specific
    // subnet appear, we evaluate the most specific prefix (longest mask, then most routes).
    let mut by_prefix: BTreeMap<String, Vec<Vec<u32>>> = BTreeMap::new();
    let fallback_prefix = data
        .resource
        .as_deref()
        .filter(|res| parse_cidr(res).is_some())
        .map(str::to_owned);

    for route in data.bgp_state {
        let raw_asns = extract_asns(&route.path);
        if raw_asns.is_empty() {
            continue;
        }
        let Some(px) = route.target_prefix.or_else(|| fallback_prefix.clone()) else {
            continue;
        };
        if parse_cidr(&px).is_none() {
            continue;
        }
        by_prefix.entry(px).or_default().push(raw_asns);
    }

    let Some((chosen_prefix, raw_routes)) =
        by_prefix
            .into_iter()
            .max_by(|(px_a, routes_a), (px_b, routes_b)| {
                let len_a = parse_cidr(px_a).map_or(0, |(_, l)| l);
                let len_b = parse_cidr(px_b).map_or(0, |(_, l)| l);
                len_a
                    .cmp(&len_b)
                    .then_with(|| routes_a.len().cmp(&routes_b.len()))
                    .then_with(|| px_b.cmp(px_a))
            })
    else {
        return Ok(ParsedBgpState {
            announced: false,
            prefix: fallback_prefix.clone(),
            prefix_base: fallback_prefix
                .as_deref()
                .and_then(|p| parse_cidr(p).map(|c| c.0)),
            prefix_len: fallback_prefix
                .as_deref()
                .and_then(|p| parse_cidr(p).map(|c| c.1)),
            origin_asn: None,
            origins: Vec::new(),
            total_peers: 0,
            distinct_paths: 0,
            upstreams: Vec::new(),
            paths: Vec::new(),
        });
    };

    let (prefix_base, prefix_len) = parse_cidr(&chosen_prefix)
        .map(|(ip, len)| (Some(ip), Some(len)))
        .unwrap_or((None, None));
    let total_peers = u32::try_from(raw_routes.len()).unwrap_or(u32::MAX);

    // Aggregate by clean_path: count total peers and keep the most frequent raw variant (so
    // prepending is preserved accurately when a majority of peers see a prepended path).
    let mut path_groups: BTreeMap<Vec<u32>, PathBucket> = BTreeMap::new();
    let mut origin_counts: BTreeMap<u32, u32> = BTreeMap::new();
    let mut upstream_counts: BTreeMap<u32, u32> = BTreeMap::new();

    for raw in raw_routes {
        let clean = clean_path(&raw);
        let Some(&origin) = clean.last() else {
            continue;
        };
        *origin_counts.entry(origin).or_insert(0) += 1;
        if clean.len() >= 2 {
            let upstream = clean[clean.len() - 2];
            if upstream != origin {
                *upstream_counts.entry(upstream).or_insert(0) += 1;
            }
        }
        let bucket = path_groups.entry(clean).or_default();
        bucket.peers += 1;
        *bucket.raw_variants.entry(raw).or_insert(0) += 1;
    }

    let distinct_paths = path_groups.len();
    let mut paths: Vec<BgpPath> = path_groups
        .into_iter()
        .map(|(asns, bucket)| {
            let peers = bucket.peers;
            let asns_raw = bucket
                .raw_variants
                .into_iter()
                .max_by(|(raw_a, cnt_a), (raw_b, cnt_b)| {
                    cnt_a
                        .cmp(cnt_b)
                        .then_with(|| raw_b.len().cmp(&raw_a.len()))
                        .then_with(|| raw_b.cmp(raw_a))
                })
                .map_or_else(|| asns.clone(), |(raw, _)| raw);
            BgpPath {
                asns,
                asns_raw,
                peers,
                is_best: false,
            }
        })
        .collect();

    paths.sort_by(|a, b| {
        b.peers
            .cmp(&a.peers)
            .then_with(|| a.asns.len().cmp(&b.asns.len()))
            .then_with(|| a.asns.cmp(&b.asns))
    });
    if let Some(first) = paths.first_mut() {
        first.is_best = true;
    }
    paths.truncate(MAX_PATHS);

    let mut origins: Vec<OriginSummary> = origin_counts
        .into_iter()
        .map(|(asn, peers)| OriginSummary { asn, peers })
        .collect();
    origins.sort_by(|a, b| b.peers.cmp(&a.peers).then_with(|| a.asn.cmp(&b.asn)));
    let origin_asn = origins.first().map(|o| o.asn);

    let mut upstreams: Vec<UpstreamShare> = upstream_counts
        .into_iter()
        .map(|(asn, peers)| UpstreamShare { asn, peers })
        .collect();
    upstreams.sort_by(|a, b| b.peers.cmp(&a.peers).then_with(|| a.asn.cmp(&b.asn)));

    Ok(ParsedBgpState {
        announced: true,
        prefix: Some(chosen_prefix),
        prefix_base,
        prefix_len,
        origin_asn,
        origins,
        total_peers,
        distinct_paths,
        upstreams,
        paths,
    })
}

/// Extracts non-zero 32-bit ASN numbers from a RIPEstat JSON `path` array, flattening any legacy
/// `AS_SET` array element to its first valid ASN.
fn extract_asns(values: &[Value]) -> Vec<u32> {
    let mut out = Vec::with_capacity(values.len());
    for item in values {
        let maybe_asn = match item {
            Value::Number(num) => num.as_u64().and_then(|n| u32::try_from(n).ok()),
            Value::Array(set) => set
                .iter()
                .find_map(|el| el.as_u64().and_then(|n| u32::try_from(n).ok())),
            _ => None,
        };
        if let Some(asn) = maybe_asn
            && asn > 0
        {
            out.push(asn);
        }
    }
    out
}

/// Parses a RIPEstat `rpki-validation` JSON payload into [`RpkiStatus`].
#[must_use]
pub fn parse_rpki(body: &[u8]) -> Option<RpkiStatus> {
    let env: RpkiEnvelope = serde_json::from_slice(body).ok()?;
    let raw = env.data?.status?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "valid" => Some(RpkiStatus::Valid),
        "invalid" | "invalid_asn" | "invalid_length" => Some(RpkiStatus::Invalid),
        "unknown" | "not-found" | "not_found" => Some(RpkiStatus::NotFound),
        _ => None,
    }
}

/// Parses a RIPEstat `as-overview` JSON payload into a cleaned short holder name.
#[must_use]
pub fn parse_as_overview(body: &[u8]) -> Option<String> {
    let env: AsOverviewEnvelope = serde_json::from_slice(body).ok()?;
    let holder = env.data?.holder?;
    let cleaned = asns::clean_holder(&holder);
    (!cleaned.is_empty()).then_some(cleaned)
}
