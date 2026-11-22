//! IP reputation (`ip.quality`): evaluates the host's public IPv4 and IPv6 addresses across nine
//! unauthenticated intelligence sources, 25 public DNSBL zones, and eight mail providers' inbound
//! MX servers on TCP port 25.

pub mod dnsbl;
pub mod mail;
pub mod measure;
pub mod parse;
pub mod providers;
pub mod result;

use std::net::SocketAddr;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use measure::Endpoints;
use serde::Deserialize;

pub use result::DnsblSummary;
pub use result::FactorSummary;
pub use result::Failure;
pub use result::FamilyReport;
pub use result::GeoVote;
pub use result::GreyNoiseClass;
pub use result::MailCheck;
pub use result::MailStatus;
pub use result::Origin;
pub use result::Output;
pub use result::RiskFactor;
pub use result::RiskLevel;
pub use result::ScoreVerdict;
pub use result::Settings;
pub use result::UsageType;

/// IP reputation plugin, configured from `[plugins."ip.quality"]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Quality {
    /// Whether to run DNSBL blacklist checks (default: `true`).
    pub dnsbl: bool,
    /// Whether to probe outbound TCP port 25 against mail providers (default: `true`).
    pub mail: bool,
    /// Optional base URL override (`https://...` or loopback `http://127.0.0.1` / `http://[::1]`)
    /// for local smoke tests.
    pub base_url: Option<String>,
    /// Optional loopback DNS server (`127.0.0.1:<port>` or `[::1]:<port>`) for local smoke tests.
    pub dns_server: Option<String>,
    /// Optional loopback SMTP server (`127.0.0.1:<port>` or `[::1]:<port>`) for local smoke tests.
    pub smtp_server: Option<String>,
}

impl Default for Quality {
    fn default() -> Self {
        Self {
            dnsbl: true,
            mail: true,
            base_url: None,
            dns_server: None,
            smtp_server: None,
        }
    }
}

impl Plugin for Quality {
    type Output = Output;
    type Failure = Failure;

    const DESCRIPTOR: Descriptor = Descriptor {
        id: "ip.quality",
        requires: Requirements {
            tools: &[],
            net: Net::Any,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(3 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let base_url = match self.base_url.as_deref() {
            Some(raw) => Some(validate_base_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let dns_server = match self.dns_server.as_deref() {
            Some(raw) => Some(validate_loopback_socket(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let smtp_server = match self.smtp_server.as_deref() {
            Some(raw) => Some(validate_loopback_socket(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let settings = Settings {
            dnsbl: self.dnsbl,
            mail: self.mail,
            custom_endpoints: base_url.is_some() || dns_server.is_some() || smtp_server.is_some(),
        };
        let endpoints = Endpoints {
            base_url,
            dns_server,
            smtp_server,
        };
        measure::measure(ctx, settings, &endpoints).await
    }
}

/// Validates `base_url`: accepts `https://...` or loopback `http://127.0.0.1` / `http://[::1]`.
#[must_use = "returns the validated base URL or the failure reason"]
pub fn validate_base_url(raw: &str) -> Result<String, Failure> {
    bench_api::validate_endpoint_url(raw, true)
        .ok_or_else(|| Failure::InvalidEndpoint(raw.to_owned()))
}

/// Validates a socket override, allowing loopback addresses (`127.0.0.0/8` or `::1`) with a
/// non-zero port only.
#[must_use = "returns the validated loopback socket address or the failure reason"]
pub fn validate_loopback_socket(raw: &str) -> Result<SocketAddr, Failure> {
    let Ok(addr) = raw.trim().parse::<SocketAddr>() else {
        return Err(Failure::InvalidEndpoint(raw.to_owned()));
    };
    if addr.ip().is_loopback() && addr.port() > 0 {
        Ok(addr)
    } else {
        Err(Failure::InvalidEndpoint(raw.to_owned()))
    }
}
