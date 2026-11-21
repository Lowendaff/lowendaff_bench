//! Typed results and failure reasons for `network.inetspeed` (Apple CDN speedtest).

use std::fmt;
use std::net::IpAddr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Serialize;

/// Effective measurement parameters recorded in [`Output`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Concurrent threads for multi-thread rounds (`4` by default).
    pub threads: u8,
    /// Per-thread transfer timeout in seconds (`10` by default).
    pub timeout_secs: u16,
    /// Human-readable per-thread byte cap string (`2G` by default).
    pub max: String,
    /// Parsed per-thread byte cap (`2_000_000_000` by default).
    pub max_bytes: u64,
    /// Idle latency sample count (`20` by default).
    pub latency_count: u8,
    /// User-forced endpoint IP when `--inetspeed-endpoint` was specified.
    pub forced_endpoint: Option<IpAddr>,
    /// Whether custom URLs overrode the default `mensura.cdn-apple.com` endpoints.
    pub custom_endpoints: bool,
}

impl Settings {
    /// Default multi-thread worker count.
    pub const DEFAULT_THREADS: u8 = 4;
    /// Default per-thread timeout in seconds.
    pub const DEFAULT_TIMEOUT_SECS: u16 = 10;
    /// Default human-readable per-thread transfer cap.
    pub const DEFAULT_MAX: &str = "2G";
    /// Default per-thread transfer cap in bytes (`2 GB`).
    pub const DEFAULT_MAX_BYTES: u64 = 2_000_000_000;
    /// Default idle latency sample count.
    pub const DEFAULT_LATENCY_COUNT: u8 = 20;
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            threads: Self::DEFAULT_THREADS,
            timeout_secs: Self::DEFAULT_TIMEOUT_SECS,
            max: Self::DEFAULT_MAX.to_owned(),
            max_bytes: Self::DEFAULT_MAX_BYTES,
            latency_count: Self::DEFAULT_LATENCY_COUNT,
            forced_endpoint: None,
            custom_endpoints: false,
        }
    }
}

/// How an Apple CDN candidate or selected endpoint IP was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSource {
    /// Explicitly pinned via `--inetspeed-endpoint` or literal URL host.
    User,
    /// Discovered via Dual DoH (`cloudflare-dns.com` + `dns.alidns.com`).
    Doh,
    /// Resolved via host system DNS after DoH timeout/empty answer.
    SystemDns,
    /// Unpinned default DNS resolution.
    DefaultDns,
}

impl EndpointSource {
    /// Canonical code string for report formatting.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Doh => "doh",
            Self::SystemDns => "system_dns",
            Self::DefaultDns => "default_dns",
        }
    }
}

impl fmt::Display for EndpointSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Probed Apple CDN candidate IP during endpoint discovery.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateEndpoint {
    /// Candidate IPv4 or IPv6 address.
    pub ip: IpAddr,
    /// Discovery source.
    pub source: EndpointSource,
    /// Measured HTTP RTT in milliseconds when healthy.
    pub rtt_ms: Option<f64>,
    /// Whether the RTT probe succeeded.
    pub ok: bool,
    /// Error summary when the RTT probe failed.
    pub error: Option<String>,
}

/// Endpoint chosen for the speedtest run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectedEndpoint {
    /// Target hostname from the download URL (e.g. `mensura.cdn-apple.com`).
    pub host: String,
    /// Pinned IP address when endpoint pinning is active.
    pub ip: Option<IpAddr>,
    /// Discovery source.
    pub source: EndpointSource,
    /// Initial RTT probe in milliseconds when available.
    pub rtt_ms: Option<f64>,
}

/// Latency summary in milliseconds (`min`, `avg`, `median`, `max`, `jitter`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LatencyStats {
    /// Number of successful RTT samples.
    pub samples: usize,
    /// Minimum RTT in milliseconds.
    pub min_ms: f64,
    /// Arithmetic mean RTT in milliseconds.
    pub avg_ms: f64,
    /// Median RTT in milliseconds.
    pub median_ms: f64,
    /// Maximum RTT in milliseconds.
    pub max_ms: f64,
    /// Jitter in milliseconds.
    pub jitter_ms: f64,
}

/// Transfer direction of a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    /// HTTP `GET` download round.
    Download,
    /// Streaming HTTP `PUT` upload round.
    Upload,
}

/// Outcome of one single-thread or multi-thread transfer round.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundResult {
    /// Transfer direction.
    pub direction: TransferDirection,
    /// Concurrent worker thread count (`1` or `settings.threads`).
    pub threads: u8,
    /// Total bytes transferred across all threads.
    pub bytes: u64,
    /// Round wall duration in milliseconds.
    pub duration_ms: u64,
    /// Aggregate throughput in bits per second.
    pub bps: u64,
    /// Aggregate throughput in megabits per second.
    pub mbps: f64,
    /// Number of worker threads that encountered an error.
    pub fault_count: u8,
    /// Loaded latency measured concurrently during this round.
    pub loaded_latency: Option<LatencyStats>,
    /// Non-empty note when the round degraded or transferred zero bytes.
    pub error: Option<String>,
}

/// Complete output of `network.inetspeed`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Effective run settings.
    pub settings: Settings,
    /// Selected Apple CDN endpoint.
    pub selected: SelectedEndpoint,
    /// Ordered candidate endpoints probed during discovery.
    pub candidates: Vec<CandidateEndpoint>,
    /// Unloaded latency measured before transfers.
    pub idle_latency: Option<LatencyStats>,
    /// Executed transfer rounds (`1T DL`, `NT DL`, `1T UL`, `NT UL`).
    pub rounds: Vec<RoundResult>,
    /// Cumulative payload bytes transferred across all rounds.
    pub total_bytes: u64,
    /// Discovery or round degradation notes.
    pub warnings: Vec<String>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `network.inetspeed` skipped or failed as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Invalid custom URL or `--inetspeed-endpoint` IP address.
    InvalidEndpoint(String),
    /// Invalid `max` byte-size string.
    InvalidMax(String),
    /// Neither IPv4 nor IPv6 was reachable on this host.
    NoFamily,
    /// Every transfer round failed with zero bytes transferred.
    NoResult,
    /// Interrupted via `Ctrl-C` or timeout cancellation.
    Cancelled,
}
