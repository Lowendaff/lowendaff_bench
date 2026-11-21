//! Typed results and failure reasons for `network.cloudflare`.

use std::net::IpAddr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Serialize;

/// Effective measurement parameters recorded in [`Output`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Duration per throughput direction in seconds (`10` by default).
    pub seconds: u16,
    /// Concurrent HTTP workers per throughput direction (`6` by default).
    pub concurrency: u8,
    /// Whether the RFC 5389 STUN UDP packet-loss probe is enabled (`true` by default).
    pub udp: bool,
    /// Number of UDP STUN Binding Request packets (`50` by default).
    pub udp_packets: u16,
    /// Idle latency window in milliseconds (`2000` by default).
    pub idle_ms: u16,
    /// Whether custom HTTP or STUN endpoints overrode the defaults.
    pub custom_endpoints: bool,
}

impl Settings {
    /// Default seconds per throughput direction.
    pub const DEFAULT_SECONDS: u16 = 10;
    /// Default concurrent HTTP worker count.
    pub const DEFAULT_CONCURRENCY: u8 = 6;
    /// Default STUN UDP probe packet count.
    pub const DEFAULT_UDP_PACKETS: u16 = 50;
    /// Default idle latency window in milliseconds.
    pub const DEFAULT_IDLE_MS: u16 = 2_000;
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            seconds: Self::DEFAULT_SECONDS,
            concurrency: Self::DEFAULT_CONCURRENCY,
            udp: true,
            udp_packets: Self::DEFAULT_UDP_PACKETS,
            idle_ms: Self::DEFAULT_IDLE_MS,
            custom_endpoints: false,
        }
    }
}

/// Latency distribution summary in milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LatencyStats {
    /// Probes sent.
    pub sent: u64,
    /// Probes answered.
    pub received: u64,
    /// Unanswered probe percentage (`0.0..=100.0`).
    pub loss_pct: f64,
    /// Minimum RTT in milliseconds.
    pub min_ms: Option<f64>,
    /// Arithmetic mean RTT in milliseconds.
    pub mean_ms: Option<f64>,
    /// Median (`p50`) RTT in milliseconds.
    pub median_ms: Option<f64>,
    /// 25th percentile RTT in milliseconds.
    pub p25_ms: Option<f64>,
    /// 75th percentile RTT in milliseconds.
    pub p75_ms: Option<f64>,
    /// 95th percentile RTT in milliseconds.
    pub p95_ms: Option<f64>,
    /// 99th percentile RTT in milliseconds.
    pub p99_ms: Option<f64>,
    /// Maximum RTT in milliseconds.
    pub max_ms: Option<f64>,
    /// Sample standard deviation jitter in milliseconds.
    pub jitter_ms: Option<f64>,
}

/// Steady-state throughput summary for one direction.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ThroughputStats {
    /// Steady-state payload bytes transferred (or full-window bytes when shorter than ramp).
    pub bytes: u64,
    /// Steady-state window duration in milliseconds.
    pub duration_ms: u64,
    /// Mean throughput in bits per second.
    pub bps: u64,
    /// Mean throughput in megabits per second.
    pub mbps: f64,
    /// Mean sample throughput in Mbit/s.
    pub mean_mbps: f64,
    /// Median (`p50`) sample throughput in Mbit/s.
    pub median_mbps: f64,
    /// 25th percentile sample throughput in Mbit/s.
    pub p25_mbps: f64,
    /// 75th percentile sample throughput in Mbit/s.
    pub p75_mbps: f64,
    /// 95th percentile sample throughput in Mbit/s.
    pub p95_mbps: f64,
    /// 99th percentile sample throughput in Mbit/s.
    pub p99_mbps: f64,
}

/// RFC 5389 STUN UDP packet-loss, reordering, and voice-quality MOS summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UdpLossStats {
    /// Probed `stun:<host>:<port>` target.
    pub target: String,
    /// STUN Binding Requests sent.
    pub sent: u64,
    /// Unique STUN Binding Success responses received.
    pub received: u64,
    /// Packet loss percentage (`0.0..=100.0`).
    pub loss_pct: f64,
    /// Count of responses arriving out of sequence order.
    pub out_of_order: u64,
    /// Out-of-order percentage relative to received packets (`0.0..=100.0`).
    pub out_of_order_pct: f64,
    /// Median UDP RTT in milliseconds.
    pub median_ms: Option<f64>,
    /// UDP RTT jitter (sample standard deviation) in milliseconds.
    pub jitter_ms: Option<f64>,
    /// Simplified ITU-T G.107 E-model Mean Opinion Score (`1.0..=5.0`).
    pub mos: Option<f64>,
    /// Packet-loss quality classification (`Excellent`, `Good`, `Acceptable`, `Poor`, `Bad`).
    pub quality: String,
}

/// Derived Waveform bufferbloat and steady-state CV stability grades.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QualityStats {
    /// Waveform bufferbloat grade (`A+`, `A`, `B`, `C`, `D`, `F`, or `-`).
    pub bufferbloat_grade: String,
    /// Worst-direction loaded median latency increase over idle median in milliseconds.
    pub bufferbloat_ms: Option<f64>,
    /// Steady-state throughput coefficient-of-variation grade (`A`, `B`, `C`, `D`, `F`, or `-`).
    pub stability_grade: String,
    /// Worst-direction steady-state throughput CV percentage.
    pub stability_cv_pct: Option<f64>,
}

/// Complete Cloudflare speedtest measurement for one IP family (`IPv4` or `IPv6`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FamilyCloudflare {
    /// Client public IP reported by Cloudflare edge metadata (masked unless `--show-ip`).
    pub client_ip: Option<IpAddr>,
    /// Three-letter IATA edge colocation code (e.g. `LAX`, `NRT`, `FRA`).
    pub colo: Option<String>,
    /// Formatted edge server location (e.g. `LAX - Los Angeles, United States`).
    pub server: Option<String>,
    /// Client Autonomous System Number reported by `/meta`.
    pub asn: Option<u32>,
    /// Client AS organization name reported by `/meta`.
    pub as_org: Option<String>,
    /// Pre-transfer unloaded HTTP RTT distribution.
    pub idle_latency: LatencyStats,
    /// Multi-worker download throughput summary.
    pub download: ThroughputStats,
    /// Multi-worker upload throughput summary.
    pub upload: ThroughputStats,
    /// Loaded HTTP RTT distribution measured during download.
    pub loaded_download_latency: LatencyStats,
    /// Loaded HTTP RTT distribution measured during upload.
    pub loaded_upload_latency: LatencyStats,
    /// Optional STUN UDP packet-loss and MOS summary.
    pub udp: Option<UdpLossStats>,
    /// Non-fatal note when the STUN UDP probe was blocked or unreachable.
    pub udp_note: Option<String>,
    /// Derived bufferbloat and throughput stability grades.
    pub quality: Option<QualityStats>,
}

/// Complete output of `network.cloudflare`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Output {
    /// Effective run settings.
    pub settings: Settings,
    /// IPv4 measurement when reachable and completed.
    pub ipv4: Option<Box<FamilyCloudflare>>,
    /// IPv6 measurement when reachable and completed.
    pub ipv6: Option<Box<FamilyCloudflare>>,
    /// Per-family failure notes `(ipv6, reason)` when the other family succeeded.
    pub notes: Vec<(bool, String)>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `network.cloudflare` skipped or failed as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Invalid custom `base_url` or `stun_host`.
    InvalidEndpoint(String),
    /// Neither IPv4 nor IPv6 was reachable on this host.
    NoFamily,
    /// Every reachable IP family failed to complete a speed test.
    NoResult(String),
    /// Interrupted via `Ctrl-C` or timeout cancellation.
    Cancelled,
}
