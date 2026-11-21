//! Apple CDN speed (`network.inetspeed`): pure-Rust implementation of `iNetSpeed-CLI` against
//! `mensura.cdn-apple.com`, featuring Dual DoH (`cloudflare-dns.com` + `dns.alidns.com`) endpoint
//! discovery with system DNS fallback, SNI-preserving IP-pinned RTT selection, unloaded latency,
//! and 1-thread / multi-thread download and streaming `PUT` upload rounds with concurrent loaded
//! latency.

pub mod measure;
pub mod parse;
pub mod result;

use std::net::IpAddr;
use std::num::NonZeroU8;
use std::num::NonZeroU16;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use serde::Deserialize;

pub use result::CandidateEndpoint;
pub use result::EndpointSource;
pub use result::Failure;
pub use result::LatencyStats;
pub use result::Output;
pub use result::RoundResult;
pub use result::SelectedEndpoint;
pub use result::Settings;
pub use result::TransferDirection;

/// Default Apple CDN download URL (`GET`).
pub const DEFAULT_DL_URL: &str = "https://mensura.cdn-apple.com/api/v1/gm/large";

/// Default Apple CDN upload URL (`PUT`).
pub const DEFAULT_UL_URL: &str = "https://mensura.cdn-apple.com/api/v1/gm/slurp";

/// Default Apple CDN latency probe URL (`GET`).
pub const DEFAULT_LATENCY_URL: &str = "https://mensura.cdn-apple.com/api/v1/gm/small";

/// Default Cloudflare DoH JSON endpoint base URL.
pub const DEFAULT_CF_DOH_URL: &str = "https://cloudflare-dns.com/dns-query";

/// Default AliDNS DoH JSON endpoint base URL.
pub const DEFAULT_ALI_DOH_URL: &str = "https://dns.alidns.com/resolve";

/// Apple CDN speed (`network.inetspeed`), as configured.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Inetspeed {
    /// Concurrent worker threads for multi-thread rounds (`1..=64`; `4` by default).
    pub threads: NonZeroU8,
    /// Per-thread transfer timeout in seconds (`1..=120`; `10` by default).
    pub timeout: NonZeroU16,
    /// Per-thread transfer cap string such as `2G`, `500M`, or `1GiB` (`2G` by default).
    pub max: String,
    /// Unloaded latency sample count (`1..=100`; `20` by default).
    pub latency_count: NonZeroU8,
    /// Optional fixed endpoint IP literal skipping DoH discovery.
    pub endpoint: Option<String>,
    /// Optional download URL override (`https://...` or loopback `http://127.0.0.1` / `[::1]`).
    pub dl_url: Option<String>,
    /// Optional upload URL override (`https://...` or loopback `http://127.0.0.1` / `[::1]`).
    pub ul_url: Option<String>,
    /// Optional latency URL override (`https://...` or loopback `http://127.0.0.1` / `[::1]`).
    pub latency_url: Option<String>,
    /// Optional Cloudflare DoH URL override used in tests.
    pub cf_doh_url: Option<String>,
    /// Optional AliDNS DoH URL override used in tests.
    pub ali_doh_url: Option<String>,
}

impl Default for Inetspeed {
    fn default() -> Self {
        Self {
            threads: NonZeroU8::new(Settings::DEFAULT_THREADS).expect("default threads > 0"),
            timeout: NonZeroU16::new(Settings::DEFAULT_TIMEOUT_SECS).expect("default timeout > 0"),
            max: Settings::DEFAULT_MAX.to_owned(),
            latency_count: NonZeroU8::new(Settings::DEFAULT_LATENCY_COUNT)
                .expect("default latency count > 0"),
            endpoint: None,
            dl_url: None,
            ul_url: None,
            latency_url: None,
            cf_doh_url: None,
            ali_doh_url: None,
        }
    }
}

impl Plugin for Inetspeed {
    type Output = Output;
    type Failure = Failure;

    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.inetspeed",
        requires: Requirements {
            tools: &[],
            net: Net::Any,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(15 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let max_trimmed = self.max.trim();
        let max_label = if max_trimmed.is_empty() {
            Settings::DEFAULT_MAX
        } else {
            max_trimmed
        };
        let max_bytes = parse::parse_size(max_label)
            .map_err(|_| Stop::Failed(Failure::InvalidMax(self.max.clone())))?;

        let forced_ip = match self
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(raw) => Some(
                raw.parse::<IpAddr>()
                    .map_err(|_| Stop::Failed(Failure::InvalidEndpoint(raw.to_owned())))?,
            ),
            None => None,
        };

        let dl_url = match self.dl_url.as_deref() {
            Some(raw) => Some(validate_endpoint_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let ul_url = match self.ul_url.as_deref() {
            Some(raw) => Some(validate_endpoint_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let latency_url = match self.latency_url.as_deref() {
            Some(raw) => Some(validate_endpoint_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let cf_doh_url = match self.cf_doh_url.as_deref() {
            Some(raw) => Some(validate_endpoint_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let ali_doh_url = match self.ali_doh_url.as_deref() {
            Some(raw) => Some(validate_endpoint_url(raw).map_err(Stop::Failed)?),
            None => None,
        };

        let settings = Settings {
            threads: self.threads.get().min(64),
            timeout_secs: self.timeout.get().min(120),
            max: max_label.to_owned(),
            max_bytes,
            latency_count: self.latency_count.get().min(100),
            forced_endpoint: forced_ip,
            custom_endpoints: dl_url.is_some()
                || ul_url.is_some()
                || latency_url.is_some()
                || cf_doh_url.is_some()
                || ali_doh_url.is_some(),
        };

        let endpoints = measure::Endpoints {
            dl_url: dl_url.as_deref().unwrap_or(DEFAULT_DL_URL),
            ul_url: ul_url.as_deref().unwrap_or(DEFAULT_UL_URL),
            latency_url: latency_url.as_deref().unwrap_or(DEFAULT_LATENCY_URL),
            cf_doh_url: cf_doh_url.as_deref().unwrap_or(DEFAULT_CF_DOH_URL),
            ali_doh_url: ali_doh_url.as_deref().unwrap_or(DEFAULT_ALI_DOH_URL),
        };

        measure::measure(ctx, settings, &endpoints).await
    }
}

/// Validates an endpoint URL (`https://...` or loopback `http://127.0.0.1` / `http://[::1]`).
#[must_use = "returns the validated URL or the failure reason"]
pub fn validate_endpoint_url(raw: &str) -> Result<String, Failure> {
    let trimmed = raw.trim();
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
