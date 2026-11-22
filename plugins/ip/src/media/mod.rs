//! Streaming media unlock (`ip.media`): probes individual OTT platforms arranged by continent
//! over IPv4 and IPv6 directly from the host, without proxies or anti-bot bypasses.

pub mod measure;
pub mod parse;
pub mod platforms;
pub mod result;

use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use serde::Deserialize;

pub use result::Choice;
pub use result::Failure;
pub use result::Group;
pub use result::Output;
pub use result::Restriction;
pub use result::ServiceResult;
pub use result::Settings;
pub use result::Verdict;

/// Streaming media unlock plugin, configured from `[plugins."ip.media"]`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Media {
    /// Continent groups to probe (`global`, `asia`, `europe`, `americas`, `oceania`, `all`).
    pub groups: Vec<Choice>,
    /// Specific service ids to probe; wins over `groups` when non-empty.
    pub services: Vec<String>,
    /// Optional base URL override (`https://...` or loopback `http://127.0.0.1` / `http://[::1]`),
    /// used by local smoke tests so CI never depends on external streaming platforms.
    pub base_url: Option<String>,
}

impl Plugin for Media {
    type Output = Output;
    type Failure = Failure;

    const DESCRIPTOR: Descriptor = Descriptor {
        id: "ip.media",
        requires: Requirements {
            tools: &[],
            net: Net::Any,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(5 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let base_url = match self.base_url.as_deref() {
            Some(raw) => Some(validate_base_url(raw).map_err(Stop::Failed)?),
            None => None,
        };
        let chosen = measure::plan(&self.groups, &self.services);
        if chosen.scheduled.is_empty() {
            return Err(Stop::Skipped(Failure::NoService(chosen.unknown)));
        }
        let settings = Settings {
            groups: self.groups.clone(),
            services: self.services.clone(),
            custom_base: base_url.is_some(),
        };
        measure::measure(ctx, chosen, settings, base_url.as_deref()).await
    }
}

/// Ensures `base_url` is `https://...` or loopback `http://127.0.0.1` / `http://[::1]`
/// and trims any trailing slash.
#[must_use = "returns the validated base URL or the failure reason"]
pub fn validate_base_url(raw: &str) -> Result<String, Failure> {
    bench_api::validate_endpoint_url(raw, true)
        .ok_or_else(|| Failure::InvalidBaseUrl(raw.to_owned()))
}
