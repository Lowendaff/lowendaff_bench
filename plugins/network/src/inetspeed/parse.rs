//! Size parser, DoH answer parser, candidate ordering, and latency statistics for
//! `network.inetspeed`, matching `iNetSpeed-CLI`.

use std::cmp::Ordering;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;

use bench_api::FamilyFilter;
use serde_json::Value;

use super::result::CandidateEndpoint;
use super::result::LatencyStats;

/// Why a transfer-cap string could not be parsed by [`parse_size`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeError {
    /// Input string was empty or whitespace-only.
    Empty,
    /// Numeric prefix was missing or malformed.
    Format,
    /// Numeric value was zero, negative, or non-finite.
    NonPositive,
    /// Unknown byte unit suffix.
    UnknownUnit,
    /// Scaled byte count fell outside `1..=u64::MAX`.
    OutOfRange,
}

/// Parses human-readable transfer cap strings (`2G`, `500M`, `1GiB`, `1048576`) into positive byte
/// counts, matching `iNetSpeed-CLI/internal/config/config.go`.
///
/// # Errors
///
/// Returns [`SizeError`] when `raw` is empty, has an unknown unit, or overflows `1..=u64::MAX`.
pub fn parse_size(raw: &str) -> Result<u64, SizeError> {
    const U64_UPPER_EXCLUSIVE: f64 = 18_446_744_073_709_551_616.0;

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SizeError::Empty);
    }
    let split_at = trimmed
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .unwrap_or(trimmed.len());
    let num_part = trimmed[..split_at].trim();
    let unit_part = trimmed[split_at..].trim().to_ascii_lowercase();
    if num_part.is_empty() {
        return Err(SizeError::Format);
    }
    let num: f64 = num_part.parse().map_err(|_| SizeError::Format)?;
    if !num.is_finite() || num <= 0.0 {
        return Err(SizeError::NonPositive);
    }
    let mul: f64 = match unit_part.as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1_000.0,
        "m" | "mb" => 1_000_000.0,
        "g" | "gb" => 1_000_000_000.0,
        "t" | "tb" => 1_000_000_000_000.0,
        "kib" => 1_024.0,
        "mib" => 1_048_576.0,
        "gib" => 1_073_741_824.0,
        "tib" => 1_099_511_627_776.0,
        _ => return Err(SizeError::UnknownUnit),
    };
    let product = num * mul;
    if !product.is_finite() || !(1.0..U64_UPPER_EXCLUSIVE).contains(&product) {
        return Err(SizeError::OutOfRange);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bytes = product as u64;
    Ok(bytes)
}

/// Extracts hostname from an `http://` or `https://` URL without brackets.
#[must_use]
pub fn host_from_url(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if let Some(stripped) = authority.strip_prefix('[') {
        let (v6_raw, tail) = stripped.split_once(']')?;
        let valid_port = tail.is_empty()
            || tail
                .strip_prefix(':')
                .and_then(|port| port.parse::<u16>().ok())
                .is_some_and(|port| port > 0);
        if !valid_port || v6_raw.parse::<Ipv6Addr>().is_err() {
            return None;
        }
        return Some(v6_raw.to_ascii_lowercase());
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if let Some(port) = port {
        let parsed = port.parse::<u16>().ok()?;
        if parsed == 0 {
            return None;
        }
    }
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower.parse::<Ipv4Addr>().is_ok() {
        return Some(lower);
    }
    let valid_labels = lower.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    });
    let last_label = lower.rsplit('.').next()?;
    let numeric_tail = last_label
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_digit());
    (valid_labels && !numeric_tail).then_some(lower)
}

/// Extracts deduplicated `IpAddr`s from a Cloudflare or AliDNS DoH response body (`Answer[].data`
/// JSON or plain-text IP lines), filtered by `families`.
#[must_use]
pub fn extract_doh_ips(body: &[u8], families: FamilyFilter) -> Vec<IpAddr> {
    let mut out = Vec::new();
    if let Ok(val) = serde_json::from_slice::<Value>(body) {
        if val.is_object() {
            let rcode = val.get("Status").and_then(Value::as_u64).unwrap_or(0);
            if rcode != 0 {
                return out;
            }
            if let Some(answers) = val.get("Answer").and_then(Value::as_array) {
                for item in answers {
                    let rr_ok = item
                        .get("type")
                        .and_then(Value::as_u64)
                        .is_none_or(|rr| matches!(rr, 1 | 28));
                    if !rr_ok {
                        continue;
                    }
                    if let Some(data) = item.get("data").and_then(Value::as_str) {
                        push_filtered_ip(&mut out, data.trim(), families);
                    }
                }
            }
            return out;
        }
        if let Some(arr) = val.as_array() {
            for item in arr {
                if let Some(data) = item.as_str() {
                    push_filtered_ip(&mut out, data.trim(), families);
                }
            }
            return out;
        }
    }

    let text = String::from_utf8_lossy(body);
    for line in text.lines() {
        push_filtered_ip(&mut out, line.trim(), families);
    }
    out
}

/// Merges multiple IP slices preserving first-seen discovery order.
#[must_use]
pub fn merge_unique_ips(lists: &[Vec<IpAddr>]) -> Vec<IpAddr> {
    let mut out = Vec::new();
    for list in lists {
        for &ip in list {
            if !out.contains(&ip) {
                out.push(ip);
            }
        }
    }
    out
}

fn push_filtered_ip(out: &mut Vec<IpAddr>, raw: &str, families: FamilyFilter) {
    let parsed: Result<IpAddr, _> = raw.parse();
    let Ok(ip) = parsed else {
        return;
    };
    let allowed = ((ip.is_ipv4() && families.ipv4) || (ip.is_ipv6() && families.ipv6))
        && bench_api::is_routable_endpoint_ip(ip);
    if allowed && !out.contains(&ip) {
        out.push(ip);
    }
}

/// Orders candidates with healthy (`ok == true`) first sorted by ascending RTT, preserving
/// discovery order for ties and failed candidates.
#[must_use]
pub fn order_candidates(candidates: &[CandidateEndpoint]) -> Vec<CandidateEndpoint> {
    let clean_rtt = |rtt: Option<f64>| {
        rtt.filter(|val| val.is_finite() && *val >= 0.0)
            .unwrap_or(f64::INFINITY)
    };
    let mut ordered = candidates.to_vec();
    ordered.sort_by(|left, right| match (left.ok, right.ok) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => clean_rtt(left.rtt_ms).total_cmp(&clean_rtt(right.rtt_ms)),
        (false, false) => Ordering::Equal,
    });
    ordered
}

/// Auto-selects the lowest-RTT healthy candidate, or falls back to the first discovered candidate
/// in original order when all probes failed.
#[must_use]
pub fn choose_auto<'a>(
    original: &'a [CandidateEndpoint],
    ordered: &'a [CandidateEndpoint],
) -> Option<&'a CandidateEndpoint> {
    ordered.iter().find(|c| c.ok).or_else(|| original.first())
}

/// Computes `min`, `avg`, `median`, `max`, and `jitter` in milliseconds (rounded to 2 decimals)
/// from microsecond RTT samples, matching `iNetSpeed-CLI/internal/latency/latency.go`.
#[must_use]
pub fn compute_latency(rtts_micros: &[u64]) -> Option<LatencyStats> {
    if rtts_micros.is_empty() {
        return None;
    }
    let samples_ms: Vec<f64> = rtts_micros
        .iter()
        .map(|&u| ((u as f64) / 1000.0).max(0.01))
        .collect();
    let mut sorted = samples_ms.clone();
    sorted.sort_by(f64::total_cmp);

    let n = samples_ms.len();
    let sum: f64 = samples_ms.iter().sum();
    let avg = sum / (n as f64);
    let min = sorted[0];
    let max = sorted[n - 1];
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    let jitter = if n > 1 {
        let mut diff_sum = 0.0;
        for idx in 1..n {
            diff_sum += (samples_ms[idx] - samples_ms[idx - 1]).abs();
        }
        diff_sum / ((n - 1) as f64)
    } else {
        0.0
    };

    Some(LatencyStats {
        samples: n,
        min_ms: round2(min),
        avg_ms: round2(avg),
        median_ms: round2(median),
        max_ms: round2(max),
        jitter_ms: round2(jitter),
    })
}

fn round2(val: f64) -> f64 {
    (val * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use bench_api::FamilyFilter;

    use super::CandidateEndpoint;
    use super::SizeError;
    use crate::inetspeed::EndpointSource;
    use crate::inetspeed::ProbeFault;

    #[test]
    fn parse_size_handles_decimal_and_binary_units() {
        assert_eq!(super::parse_size("2G"), Ok(2_000_000_000));
        assert_eq!(super::parse_size("500M"), Ok(500_000_000));
        assert_eq!(super::parse_size("1GiB"), Ok(1_073_741_824));
        assert_eq!(super::parse_size("64KiB"), Ok(65_536));
        assert_eq!(super::parse_size("0"), Err(SizeError::NonPositive));
        assert_eq!(super::parse_size("abc"), Err(SizeError::Format));
        assert_eq!(super::parse_size("10XB"), Err(SizeError::UnknownUnit));
        assert_eq!(super::parse_size("16777216TiB"), Err(SizeError::OutOfRange));
    }

    #[test]
    fn doh_extraction_candidate_ordering_and_latency_stats_match_reference() {
        let cf_json = concat!(
            r#"{"Answer":[{"data":"cname.example."},"#,
            r#"{"data":"198.51.100.10"},{"data":"2001:db8::10"}]}"#
        )
        .as_bytes();
        let v4_filter = FamilyFilter {
            ipv4: true,
            ipv6: false,
        };
        let dual_filter = FamilyFilter {
            ipv4: true,
            ipv6: true,
        };
        let ip_10: IpAddr = "198.51.100.10".parse().expect("10");
        let ip_11: IpAddr = "198.51.100.11".parse().expect("11");
        let ip_12: IpAddr = "198.51.100.12".parse().expect("12");

        let v4_only = super::extract_doh_ips(cf_json, v4_filter);
        assert_eq!(v4_only, vec![ip_10]);
        let dual = super::extract_doh_ips(cf_json, dual_filter);
        assert_eq!(dual.len(), 2);

        let ali_short =
            br#"["198.51.100.11","198.51.100.10","127.0.0.1","10.0.0.1","169.254.169.254"]"#;
        let ali_ips = super::extract_doh_ips(ali_short, v4_filter);
        let merged = super::merge_unique_ips(&[v4_only, ali_ips]);
        assert_eq!(
            merged,
            vec![ip_10, ip_11],
            "loopback, RFC1918, and link-local/metadata IPs in DoH answers must be discarded"
        );
        let poisoned_v6 = br#"{"Answer":[{"data":"::1"},{"data":"fe80::1"},{"data":"fd00::1"}]}"#;
        assert!(super::extract_doh_ips(poisoned_v6, dual_filter).is_empty());

        let raw_cands = vec![
            CandidateEndpoint {
                ip: ip_10,
                source: EndpointSource::Doh,
                rtt_ms: None,
                ok: false,
                error: Some(ProbeFault::TimedOut),
            },
            CandidateEndpoint {
                ip: ip_11,
                source: EndpointSource::Doh,
                rtt_ms: Some(18.5),
                ok: true,
                error: None,
            },
            CandidateEndpoint {
                ip: ip_12,
                source: EndpointSource::Doh,
                rtt_ms: Some(9.2),
                ok: true,
                error: None,
            },
        ];
        let ordered = super::order_candidates(&raw_cands);
        let chosen = super::choose_auto(&raw_cands, &ordered).expect("chosen");
        assert_eq!(chosen.ip, ip_12);

        let lat = super::compute_latency(&[10_000, 40_000, 10_000, 40_000]).expect("lat");
        assert_eq!(lat.samples, 4);
        assert_eq!(lat.min_ms, 10.0);
        assert_eq!(lat.median_ms, 25.0);
        assert_eq!(lat.max_ms, 40.0);
        assert_eq!(lat.jitter_ms, 30.0);

        let nodata_json = br#"{"Status":0,"Comment":"198.51.100.99"}"#;
        assert!(super::extract_doh_ips(nodata_json, v4_filter).is_empty());
        let html_banner = b"<html><hr>openresty/1.25.3.1</hr></html>";
        assert!(super::extract_doh_ips(html_banner, v4_filter).is_empty());
        assert_eq!(
            super::host_from_url("https://[2001:db8::1]:8443/path").as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(super::host_from_url("ftp://cdn.example/a"), None);
        assert_eq!(super::host_from_url("https://user@cdn.example/a"), None);
    }
}
