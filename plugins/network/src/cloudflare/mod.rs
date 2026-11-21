//! Cloudflare speed (`network.cloudflare`): pure-Rust multi-worker HTTPS download and upload
//! throughput, idle and loaded latency, Waveform bufferbloat and CV stability grading, and RFC 5389
//! STUN UDP packet-loss and ITU-T G.107 E-model MOS probing against `speed.cloudflare.com`.

pub mod measure;
pub mod parse;
pub mod result;

use std::collections::hash_map::RandomState;
use std::num::NonZeroU8;
use std::num::NonZeroU16;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use serde::Deserialize;

pub use result::Failure;
pub use result::FamilyCloudflare;
pub use result::LatencyStats;
pub use result::Output;
pub use result::QualityStats;
pub use result::Settings;
pub use result::ThroughputStats;
pub use result::UdpLossStats;

/// Default Cloudflare speedtest base URL.
pub const DEFAULT_BASE_URL: &str = "https://speed.cloudflare.com";

/// Default Cloudflare STUN server hostname for UDP packet-loss probing.
pub const DEFAULT_STUN_HOST: &str = "stun.cloudflare.com";

/// Default RFC 5389 STUN UDP port.
pub const DEFAULT_STUN_PORT: u16 = 3478;

/// Cloudflare speed (`network.cloudflare`), as configured.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cloudflare {
    /// Duration per throughput direction in seconds (`1..=60`; `10` by default).
    pub seconds: NonZeroU16,
    /// Concurrent HTTP workers per throughput direction (`1..=32`; `6` by default).
    pub concurrency: NonZeroU8,
    /// Whether to run the RFC 5389 STUN UDP packet-loss probe (`true` by default).
    pub udp: bool,
    /// Number of UDP STUN Binding Request packets when `udp` is enabled (`1..=500`; `50` default).
    pub udp_packets: u16,
    /// Idle latency sampling duration in milliseconds (`200..=10000`; `2000` by default).
    pub idle_ms: u16,
    /// Optional base URL override (`https://...` or loopback `http://127.0.0.1` / `http://[::1]`).
    pub base_url: Option<String>,
    /// Optional STUN server hostname or IP override used by local smoke responders.
    pub stun_host: Option<String>,
    /// STUN UDP port (`3478` by default).
    pub stun_port: u16,
}

impl Default for Cloudflare {
    fn default() -> Self {
        Self {
            seconds: NonZeroU16::new(Settings::DEFAULT_SECONDS).expect("default seconds > 0"),
            concurrency: NonZeroU8::new(Settings::DEFAULT_CONCURRENCY)
                .expect("default concurrency > 0"),
            udp: true,
            udp_packets: Settings::DEFAULT_UDP_PACKETS,
            idle_ms: Settings::DEFAULT_IDLE_MS,
            base_url: None,
            stun_host: None,
            stun_port: DEFAULT_STUN_PORT,
        }
    }
}

impl Plugin for Cloudflare {
    type Output = Output;
    type Failure = Failure;

    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.cloudflare",
        requires: Requirements {
            tools: &[],
            net: Net::Any,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(10 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let base_url = match self.base_url.as_deref() {
            Some(raw) => Some(validate_base_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let stun_host = match self.stun_host.as_deref() {
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains(char::is_control)
                {
                    return Err(Stop::Failed(Failure::InvalidEndpoint(raw.to_owned())));
                }
                Some(trimmed.to_owned())
            }
            None => None,
        };
        if self.stun_port == 0 {
            return Err(Stop::Failed(Failure::InvalidEndpoint(
                "stun_port=0".to_owned(),
            )));
        }
        let settings = Settings {
            seconds: self.seconds.get().min(60),
            concurrency: self.concurrency.get().min(32),
            udp: self.udp,
            udp_packets: self.udp_packets.clamp(1, 500),
            idle_ms: self.idle_ms.clamp(200, 10_000),
            custom_endpoints: base_url.is_some()
                || stun_host.is_some()
                || self.stun_port != DEFAULT_STUN_PORT,
        };
        let meas_id = random_meas_id();
        measure::measure(
            ctx,
            settings,
            base_url.as_deref().unwrap_or(DEFAULT_BASE_URL),
            stun_host.as_deref().unwrap_or(DEFAULT_STUN_HOST),
            self.stun_port,
            &meas_id,
        )
        .await
    }
}

/// Validates `base_url`: accepts `https://...` or loopback `http://127.0.0.1` / `http://[::1]`.
#[must_use = "returns the validated base URL or the failure reason"]
pub fn validate_base_url(raw: &str) -> Result<String, Failure> {
    let trimmed = raw.trim().trim_end_matches('/');
    let valid = trimmed.starts_with("https://")
        || is_loopback_http(trimmed, "http://127.0.0.1")
        || is_loopback_http(trimmed, "http://[::1]");
    if valid && trimmed.len() > "https://".len() {
        Ok(trimmed.to_owned())
    } else {
        Err(Failure::InvalidEndpoint(raw.to_owned()))
    }
}

fn is_loopback_http(url: &str, prefix: &str) -> bool {
    let Some(rest) = url.strip_prefix(prefix) else {
        return false;
    };
    rest.is_empty() || rest.starts_with(':') || rest.starts_with('/')
}

fn random_meas_id() -> String {
    use std::hash::BuildHasher;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    RandomState::new().hash_one(nanos).to_string()
}
