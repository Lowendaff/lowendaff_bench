//! Metadata parsers, steady-state throughput windowing, percentile/jitter statistics, ITU-T G.107
//! E-model MOS, and Waveform bufferbloat / CV stability grading for `network.cloudflare`.

use std::cmp::Ordering;
use std::net::IpAddr;

use bench_api::CloudflareStun;
use bench_api::HttpResponse;
use bench_api::ThroughputTick;
use serde_json::Value;

use super::result::BufferbloatGrade;
use super::result::LatencyStats;
use super::result::QualityStats;
use super::result::StabilityGrade;
use super::result::ThroughputStats;
use super::result::UdpLossStats;
use super::result::UdpQuality;

const STEADY_RAMP_FRACTION: f64 = 0.20;
const STEADY_MIN_RAMP_MS: u64 = 1_000;
const MIN_SAMPLE_WINDOW_MS: u64 = 200;
const MIN_STABILITY_SAMPLES: usize = 3;

const BUFFERBLOAT_THRESHOLDS: &[(f64, BufferbloatGrade)] = &[
    (5.0, BufferbloatGrade::APlus),
    (30.0, BufferbloatGrade::A),
    (60.0, BufferbloatGrade::B),
    (200.0, BufferbloatGrade::C),
    (400.0, BufferbloatGrade::D),
    (f64::INFINITY, BufferbloatGrade::F),
];

const STABILITY_THRESHOLDS: &[(f64, StabilityGrade)] = &[
    (5.0, StabilityGrade::A),
    (10.0, StabilityGrade::B),
    (20.0, StabilityGrade::C),
    (35.0, StabilityGrade::D),
    (f64::INFINITY, StabilityGrade::F),
];

/// Parsed Cloudflare edge and client metadata fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedMeta {
    /// Client IP reported by Cloudflare.
    pub client_ip: Option<IpAddr>,
    /// Edge IATA colocation code (e.g. `LAX`).
    pub colo: Option<String>,
    /// Edge or client city from metadata headers.
    pub city: Option<String>,
    /// Country code or name.
    pub country: Option<String>,
    /// Client ASN.
    pub asn: Option<u32>,
    /// Client AS organization name.
    pub as_org: Option<String>,
}

impl ParsedMeta {
    /// Fills missing fields from `fallback`.
    pub fn merge_from(&mut self, fallback: &Self) {
        if self.client_ip.is_none() {
            self.client_ip = fallback.client_ip;
        }
        if self.colo.is_none() {
            self.colo.clone_from(&fallback.colo);
        }
        if self.city.is_none() {
            self.city.clone_from(&fallback.city);
        }
        if self.country.is_none() {
            self.country.clone_from(&fallback.country);
        }
        if self.asn.is_none() {
            self.asn = fallback.asn;
        }
        if self.as_org.is_none() {
            self.as_org.clone_from(&fallback.as_org);
        }
    }
}

/// Parses `/meta` JSON body into [`ParsedMeta`].
#[must_use]
pub fn parse_meta_json(body: &[u8]) -> ParsedMeta {
    let Ok(val) = serde_json::from_slice::<Value>(body) else {
        return ParsedMeta::default();
    };
    let Some(obj) = val.as_object() else {
        return ParsedMeta::default();
    };
    let client_ip = obj
        .get("clientIp")
        .and_then(Value::as_str)
        .and_then(|s| s.trim().parse::<IpAddr>().ok());
    let colo = non_empty_str(obj.get("colo"));
    let city = non_empty_str(obj.get("city"));
    let country = non_empty_str(obj.get("country"));
    let asn = obj.get("asn").and_then(|v| {
        v.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u32>().ok()))
    });
    let as_org = non_empty_str(obj.get("asOrganization"));
    ParsedMeta {
        client_ip,
        colo,
        city,
        country,
        asn,
        as_org,
    }
}

/// Parses `/cdn-cgi/trace` `key=value` lines into [`ParsedMeta`].
#[must_use]
pub fn parse_trace_text(text: &str) -> ParsedMeta {
    let mut meta = ParsedMeta::default();
    for line in text.lines() {
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let val = val.trim();
        if val.is_empty() {
            continue;
        }
        match key.trim() {
            "ip" => meta.client_ip = val.parse::<IpAddr>().ok(),
            "colo" => meta.colo = Some(val.to_owned()),
            "loc" => meta.country = Some(val.to_owned()),
            _ => {}
        }
    }
    meta
}

/// Extracts `cf-meta-*`, `cf-connecting-ip`, and `cf-ray` headers from an [`HttpResponse`].
#[must_use]
pub fn parse_response_headers(resp: &HttpResponse) -> ParsedMeta {
    let client_ip = resp
        .header("cf-meta-ip")
        .or_else(|| resp.header("cf-connecting-ip"))
        .and_then(|s| s.trim().parse::<IpAddr>().ok());
    let colo = resp
        .header("cf-meta-colo")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            resp.header("cf-ray")
                .and_then(|ray| ray.split('-').nth(1))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        });
    let city = resp
        .header("cf-meta-city")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let country = resp
        .header("cf-meta-country")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let asn = resp
        .header("cf-meta-asn")
        .and_then(|s| s.trim().parse::<u32>().ok());
    ParsedMeta {
        client_ip,
        colo,
        city,
        country,
        asn,
        as_org: None,
    }
}

/// Maps a 3-letter `colo` code against `/locations` JSON to `COLO - City, Country` (or `COLO`).
#[must_use]
pub fn map_colo_to_server(locations_body: Option<&[u8]>, colo: &str) -> String {
    let Some(raw) = locations_body else {
        return colo.to_owned();
    };
    let Ok(val) = serde_json::from_slice::<Value>(raw) else {
        return colo.to_owned();
    };
    if let Some(matched) = find_colo_object(&val, colo)
        && let Some(obj) = matched.as_object()
    {
        let city = non_empty_str(obj.get("city")).or_else(|| non_empty_str(obj.get("name")));
        let country = non_empty_str(obj.get("country"))
            .or_else(|| non_empty_str(obj.get("countryName")))
            .or_else(|| non_empty_str(obj.get("cca2")));
        match (city, country) {
            (Some(c), Some(k)) => return format!("{colo} - {c}, {k}"),
            (Some(c), None) => return format!("{colo} - {c}"),
            (None, Some(k)) => return format!("{colo} - {k}"),
            (None, None) => {}
        }
    }
    colo.to_owned()
}

fn find_colo_object<'a>(val: &'a Value, colo: &str) -> Option<&'a Value> {
    match val {
        Value::Array(items) => {
            for item in items {
                if let Some(hit) = find_colo_object(item, colo) {
                    return Some(hit);
                }
            }
            None
        }
        Value::Object(map) => {
            for key in ["iata", "colo", "code", "id"] {
                if map
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.eq_ignore_ascii_case(colo))
                {
                    return Some(val);
                }
            }
            for child in map.values() {
                if let Some(hit) = find_colo_object(child, colo) {
                    return Some(hit);
                }
            }
            None
        }
        _ => None,
    }
}

fn non_empty_str(val: Option<&Value>) -> Option<String> {
    val.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Computes [`LatencyStats`] from probe count and RTT samples in microseconds.
#[must_use]
pub fn compute_latency_stats(sent: u64, rtts_micros: &[u64]) -> LatencyStats {
    let received = rtts_micros.len() as u64;
    let effective_sent = sent.max(received);
    let loss_pct = if effective_sent == 0 {
        0.0
    } else {
        ((effective_sent - received) as f64) * 100.0 / (effective_sent as f64)
    };
    if rtts_micros.is_empty() {
        return LatencyStats {
            sent: effective_sent,
            received: 0,
            loss_pct,
            ..LatencyStats::default()
        };
    }

    let samples_ms: Vec<f64> = rtts_micros.iter().map(|&u| (u as f64) / 1000.0).collect();
    let mut sorted = samples_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let sum_ms: f64 = samples_ms.iter().sum();
    let mean = sum_ms / (samples_ms.len() as f64);
    let jitter = sample_stddev(&samples_ms);

    LatencyStats {
        sent: effective_sent,
        received,
        loss_pct,
        min_ms: Some(sorted[0]),
        mean_ms: Some(mean),
        median_ms: Some(percentile_sorted(&sorted, 0.50)),
        p25_ms: Some(percentile_sorted(&sorted, 0.25)),
        p75_ms: Some(percentile_sorted(&sorted, 0.75)),
        p95_ms: Some(percentile_sorted(&sorted, 0.95)),
        p99_ms: Some(percentile_sorted(&sorted, 0.99)),
        max_ms: Some(sorted[sorted.len() - 1]),
        jitter_ms: jitter,
    }
}

/// Computes steady-state [`ThroughputStats`] (and returns steady-state Mbit/s samples for CV
/// grading), excluding TCP slow-start ramp (`max(20% of duration, 1000 ms)`).
#[must_use]
pub fn compute_throughput_stats(
    total_bytes: u64,
    duration_ms: u64,
    ticks: &[ThroughputTick],
) -> (ThroughputStats, Vec<f64>) {
    let interval_mbps = tick_rates_mbps(ticks);
    let (bytes, win_ms, steady_mbps) = steady_state_window(ticks, &interval_mbps, duration_ms)
        .unwrap_or_else(|| (total_bytes, duration_ms.max(1), interval_mbps.clone()));

    let fallback_mbps = {
        let secs = (win_ms.max(1) as f64) / 1000.0;
        ((bytes as f64) * 8.0) / (secs * 1_000_000.0)
    };

    let (mean, median, p25, p75, p95, p99) = if steady_mbps.is_empty() {
        (
            fallback_mbps,
            fallback_mbps,
            fallback_mbps,
            fallback_mbps,
            fallback_mbps,
            fallback_mbps,
        )
    } else {
        let mut sorted = steady_mbps.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let sum_mbps: f64 = steady_mbps.iter().sum();
        let mean_val = sum_mbps / (steady_mbps.len() as f64);
        (
            mean_val,
            percentile_sorted(&sorted, 0.50),
            percentile_sorted(&sorted, 0.25),
            percentile_sorted(&sorted, 0.75),
            percentile_sorted(&sorted, 0.95),
            percentile_sorted(&sorted, 0.99),
        )
    };

    let bps = f64_to_u64_sat((mean.max(0.0) * 1_000_000.0).round());
    (
        ThroughputStats {
            bytes,
            duration_ms: win_ms,
            bps,
            mbps: mean,
            mean_mbps: mean,
            median_mbps: median,
            p25_mbps: p25,
            p75_mbps: p75,
            p95_mbps: p95,
            p99_mbps: p99,
        },
        steady_mbps,
    )
}

fn tick_rates_mbps(ticks: &[ThroughputTick]) -> Vec<f64> {
    let mut out = Vec::with_capacity(ticks.len());
    let mut prev_ms = 0_u64;
    let mut prev_bytes = 0_u64;
    for &tick in ticks {
        let dt_ms = tick.elapsed_ms.saturating_sub(prev_ms).max(1);
        let dbytes = tick.bytes.saturating_sub(prev_bytes);
        let secs = (dt_ms as f64) / 1000.0;
        let mbps = ((dbytes as f64) * 8.0) / (secs * 1_000_000.0);
        out.push(mbps);
        prev_ms = tick.elapsed_ms;
        prev_bytes = tick.bytes;
    }
    out
}

fn steady_state_window(
    ticks: &[ThroughputTick],
    mbps_samples: &[f64],
    total_ms: u64,
) -> Option<(u64, u64, Vec<f64>)> {
    if ticks.len() < 2 {
        return None;
    }
    let ramp_ms =
        f64_to_u64_sat(((total_ms as f64) * STEADY_RAMP_FRACTION).round()).max(STEADY_MIN_RAMP_MS);
    let start_idx = ticks.iter().position(|tick| tick.elapsed_ms >= ramp_ms)?;
    let start_tick = ticks[start_idx];
    let end_tick = *ticks.last()?;
    let window_ms = end_tick.elapsed_ms.saturating_sub(start_tick.elapsed_ms);
    if window_ms < MIN_SAMPLE_WINDOW_MS {
        return None;
    }
    let slice_start = (start_idx + 1).min(mbps_samples.len());
    Some((
        end_tick.bytes.saturating_sub(start_tick.bytes),
        window_ms,
        mbps_samples[slice_start..].to_vec(),
    ))
}

/// Computes [`UdpLossStats`] (loss %, out-of-order %, ITU-T G.107 E-model MOS, quality label).
#[must_use]
pub fn compute_udp_stats(raw: &CloudflareStun) -> UdpLossStats {
    let lat = compute_latency_stats(raw.sent, &raw.rtts_micros);
    let out_of_order_pct = if raw.received == 0 {
        0.0
    } else {
        (raw.out_of_order as f64) * 100.0 / (raw.received as f64)
    };
    let mos = match (lat.median_ms, lat.jitter_ms) {
        (Some(med), Some(jit)) => calculate_mos(med, jit, lat.loss_pct),
        (Some(med), None) => calculate_mos(med, 0.0, lat.loss_pct),
        _ => None,
    };
    UdpLossStats {
        target: raw.target.clone(),
        sent: lat.sent,
        received: lat.received,
        loss_pct: lat.loss_pct,
        out_of_order: raw.out_of_order,
        out_of_order_pct,
        median_ms: lat.median_ms,
        jitter_ms: lat.jitter_ms,
        mos,
        quality: loss_quality(lat.loss_pct),
    }
}

/// Computes Waveform bufferbloat and steady-state CV stability grades.
#[must_use]
pub fn compute_quality(
    idle_median_ms: Option<f64>,
    dl_loaded_median_ms: Option<f64>,
    ul_loaded_median_ms: Option<f64>,
    dl_steady_mbps: &[f64],
    ul_steady_mbps: &[f64],
) -> Option<QualityStats> {
    let bloat_ms = idle_median_ms.and_then(|idle| {
        let deltas: Vec<f64> = [dl_loaded_median_ms, ul_loaded_median_ms]
            .into_iter()
            .flatten()
            .map(|loaded| (loaded - idle).max(0.0))
            .collect();
        (!deltas.is_empty()).then(|| deltas.into_iter().fold(0.0_f64, f64::max))
    });

    let cv_dl = cv_percent(dl_steady_mbps);
    let cv_ul = cv_percent(ul_steady_mbps);
    let cv_worst = match (cv_dl, cv_ul) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    };

    if bloat_ms.is_none() && cv_worst.is_none() {
        return None;
    }

    let (bufferbloat_grade, bufferbloat_ms) = match bloat_ms {
        Some(ms) => (
            grade_for(ms, BUFFERBLOAT_THRESHOLDS, BufferbloatGrade::F),
            Some(ms),
        ),
        None => (BufferbloatGrade::Unavailable, None),
    };
    let (stability_grade, stability_cv_pct) = match cv_worst {
        Some(cv) => (
            grade_for(cv, STABILITY_THRESHOLDS, StabilityGrade::F),
            Some(cv),
        ),
        None => (StabilityGrade::Unavailable, None),
    };

    Some(QualityStats {
        bufferbloat_grade,
        bufferbloat_ms,
        stability_grade,
        stability_cv_pct,
    })
}

fn grade_for<T: Copy>(value: f64, thresholds: &[(f64, T)], fallback: T) -> T {
    let clamped = value.max(0.0);
    for &(limit, grade) in thresholds {
        if clamped <= limit {
            return grade;
        }
    }
    fallback
}

fn cv_percent(samples: &[f64]) -> Option<f64> {
    if samples.len() < MIN_STABILITY_SAMPLES {
        return None;
    }
    let sum: f64 = samples.iter().sum();
    let mean = sum / (samples.len() as f64);
    if mean.abs() < f64::EPSILON {
        return None;
    }
    let sd = sample_stddev(samples)?;
    Some(sd / mean * 100.0)
}

fn sample_stddev(samples: &[f64]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let n = samples.len() as f64;
    let sum: f64 = samples.iter().sum();
    let mean = sum / n;
    let sq_diff_sum: f64 = samples.iter().map(|&x| (x - mean).powi(2)).sum();
    let var = sq_diff_sum / (n - 1.0);
    Some(var.sqrt())
}

fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let clamped = p.clamp(0.0, 1.0);
    let pos = clamped * ((n - 1) as f64);
    let lower = f64_to_usize_sat(pos.floor()).min(n - 1);
    let upper = f64_to_usize_sat(pos.ceil()).min(n - 1);
    if lower == upper {
        sorted[lower]
    } else {
        let frac = pos - (lower as f64);
        sorted[lower] + frac * (sorted[upper] - sorted[lower])
    }
}

fn calculate_mos(rtt_ms: f64, jitter_ms: f64, loss_pct: f64) -> Option<f64> {
    if rtt_ms.is_nan() || jitter_ms.is_nan() || loss_pct.is_nan() {
        return None;
    }
    if rtt_ms < 0.0 || jitter_ms < 0.0 || loss_pct < 0.0 {
        return None;
    }
    let d = rtt_ms / 2.0 + 2.0 * jitter_ms;
    let ld = d.min(177.3);
    let ie_eff = 30.0 * (loss_pct / 100.0).min(1.0);
    let r = (93.2 - (ld / 40.0) - ie_eff).clamp(0.0, 100.0);
    let mos = 1.0 + 0.035 * r + 7.0e-6 * r * (r - 60.0) * (100.0 - r);
    Some(mos.clamp(1.0, 5.0))
}

fn loss_quality(loss_pct: f64) -> UdpQuality {
    if loss_pct.is_nan() {
        UdpQuality::Unknown
    } else if loss_pct == 0.0 {
        UdpQuality::Excellent
    } else if loss_pct < 1.0 {
        UdpQuality::Good
    } else if loss_pct < 2.5 {
        UdpQuality::Acceptable
    } else if loss_pct < 5.0 {
        UdpQuality::Poor
    } else {
        UdpQuality::Bad
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_to_u64_sat(value: f64) -> u64 {
    const UPPER_EXCLUSIVE: f64 = 18_446_744_073_709_551_616.0;
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= UPPER_EXCLUSIVE {
        u64::MAX
    } else {
        value as u64
    }
}

fn f64_to_usize_sat(value: f64) -> usize {
    usize::try_from(f64_to_u64_sat(value)).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use bench_api::CloudflareStun;
    use bench_api::HttpResponse;
    use bench_api::ThroughputTick;

    #[test]
    fn meta_trace_headers_and_locations_parse_and_merge_cleanly() {
        let meta_raw = concat!(
            r#"{"clientIp":"198.51.100.7","colo":"SJC","#,
            r#""asn":64500,"asOrganization":"ExampleNet"}"#
        )
        .as_bytes();
        let mut meta = super::parse_meta_json(meta_raw);
        let expected_ip: IpAddr = "198.51.100.7".parse().expect("ip");
        assert_eq!(meta.client_ip, Some(expected_ip));
        assert_eq!(meta.colo.as_deref(), Some("SJC"));
        assert_eq!(meta.asn, Some(64500));

        let trace = super::parse_trace_text("ip=198.51.100.9\ncolo=LAX\nloc=US\n");
        meta.merge_from(&trace);
        assert_eq!(
            meta.colo.as_deref(),
            Some("SJC"),
            "keeps existing /meta colo"
        );
        assert_eq!(meta.country.as_deref(), Some("US"), "fills missing country");

        let resp = HttpResponse {
            status: 200,
            final_url: "https://speed.example/__down".to_owned(),
            headers: vec![("cf-ray".to_owned(), "89abcdef01234567-NRT".to_owned())],
            body: Vec::new(),
        };
        let hdr = super::parse_response_headers(&resp);
        assert_eq!(hdr.colo.as_deref(), Some("NRT"));

        let locs = br#"[{"iata":"SJC","city":"San Jose","country":"United States"}]"#;
        assert_eq!(
            super::map_colo_to_server(Some(locs), "SJC"),
            "SJC - San Jose, United States"
        );
    }

    #[test]
    fn steady_state_throughput_mos_and_quality_grades_match_reference_thresholds() {
        // 10 x 1000 ms ticks: slow ramp in first 2 s, then steady 1_000_000 bytes/s (8 Mbit/s).
        let ticks = vec![
            ThroughputTick {
                elapsed_ms: 1_000,
                bytes: 50_000,
            },
            ThroughputTick {
                elapsed_ms: 2_000,
                bytes: 200_000,
            },
            ThroughputTick {
                elapsed_ms: 3_000,
                bytes: 1_200_000,
            },
            ThroughputTick {
                elapsed_ms: 4_000,
                bytes: 2_200_000,
            },
            ThroughputTick {
                elapsed_ms: 5_000,
                bytes: 3_200_000,
            },
            ThroughputTick {
                elapsed_ms: 6_000,
                bytes: 4_200_000,
            },
            ThroughputTick {
                elapsed_ms: 7_000,
                bytes: 5_200_000,
            },
            ThroughputTick {
                elapsed_ms: 8_000,
                bytes: 6_200_000,
            },
            ThroughputTick {
                elapsed_ms: 9_000,
                bytes: 7_200_000,
            },
            ThroughputTick {
                elapsed_ms: 10_000,
                bytes: 8_200_000,
            },
        ];
        let (stats, steady) = super::compute_throughput_stats(8_200_000, 10_000, &ticks);
        assert_eq!(stats.bytes, 8_000_000);
        assert_eq!(stats.duration_ms, 8_000);
        assert!((stats.mbps - 8.0).abs() < 1e-6);

        let q = super::compute_quality(Some(20.0), Some(24.0), Some(45.0), &steady, &steady)
            .expect("quality");
        assert_eq!(q.bufferbloat_grade, super::BufferbloatGrade::A);
        assert_eq!(q.stability_grade, super::StabilityGrade::A);

        let udp = super::compute_udp_stats(&CloudflareStun {
            target: "stun:stun.example:3478".to_owned(),
            sent: 10,
            received: 10,
            out_of_order: 1,
            rtts_micros: vec![15_000; 10],
        });
        assert_eq!(udp.loss_pct, 0.0);
        assert_eq!(udp.out_of_order_pct, 10.0);
        assert_eq!(udp.quality, super::UdpQuality::Excellent);
        assert!(udp.mos.expect("mos") > 4.0);
    }
}
