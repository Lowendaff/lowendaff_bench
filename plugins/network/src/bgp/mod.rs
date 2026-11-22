//! BGP paths (`network.bgp`): queries globally observed BGP `AS_PATH` routes, direct upstreams,
//! origin ASNs, and RPKI ROA validity for the host's public IPv4 and IPv6 prefixes via RIPEstat.

pub mod asns;
pub mod measure;
pub mod parse;
pub mod result;

use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use serde::Deserialize;

pub use result::BgpPath;
pub use result::Failure;
pub use result::FamilyBgp;
pub use result::OriginSummary;
pub use result::Output;
pub use result::RpkiStatus;
pub use result::Settings;
pub use result::UpstreamShare;

/// BGP path topology plugin (`network.bgp`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bgp {
    /// Whether to upload rendered topology SVGs to `imageupload.app` in `text` mode.
    pub upload: bool,
    /// Optional base URL override (`https://...` or loopback `http://127.0.0.1` / `http://[::1]`)
    /// used by local smoke responders.
    pub base_url: Option<String>,
    /// Optional topology upload endpoint override (`https://...` or loopback `http://127.0.0.1` /
    /// `http://[::1]`) used by local smoke responders.
    pub upload_url: Option<String>,
}

impl Default for Bgp {
    fn default() -> Self {
        Self {
            upload: true,
            base_url: None,
            upload_url: None,
        }
    }
}

impl Plugin for Bgp {
    type Output = Output;
    type Failure = Failure;

    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.bgp",
        requires: Requirements {
            tools: &[],
            net: Net::Any,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(2 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let base_url = match self.base_url.as_deref() {
            Some(raw) => Some(validate_base_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let upload_url = match self.upload_url.as_deref() {
            Some(raw) => Some(validate_base_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let settings = Settings {
            custom_endpoints: base_url.is_some() || upload_url.is_some(),
            upload: self.upload,
            upload_url,
        };
        measure::measure(ctx, settings, base_url.as_deref()).await
    }
}

/// Validates `base_url`: accepts `https://...` or loopback `http://127.0.0.1` / `http://[::1]`.
#[must_use = "returns the validated base URL or the failure reason"]
pub fn validate_base_url(raw: &str) -> Result<String, Failure> {
    bench_api::validate_endpoint_url(raw, true)
        .ok_or_else(|| Failure::InvalidEndpoint(raw.to_owned()))
}
