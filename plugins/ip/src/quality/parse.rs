//! Shared HTTP probe helpers, WAF/rate-limit gate, and pure parsing utilities for intelligence
//! provider modules.

use std::net::IpAddr;

use bench_api::Context;
use bench_api::HttpError;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use serde_json::Value;

use super::result::Output;
use super::result::RiskFactor;
use super::result::RiskLevel;
use super::result::ScoreVerdict;
use super::result::UsageType;

/// Normalized facts extracted from one intelligence provider's response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderData {
    /// Autonomous System Number (`AS13335` -> `13335`).
    pub asn: Option<u32>,
    /// Organization or ISP name.
    pub org: Option<String>,
    /// Reverse DNS or observed hostname.
    pub hostname: Option<String>,
    /// Regional Internet Registry (`ARIN`, `RIPE`, `APNIC`, `LACNIC`, `AFRINIC`).
    pub rir: Option<String>,
    /// Country recorded in RIR/ASN registration.
    pub registered_country: Option<String>,
    /// Operational GeoIP country code.
    pub geo_country: Option<String>,
    /// Network usage classification.
    pub usage: Option<UsageType>,
    /// Provider's risk score or telemetry verdict, when the provider supplies one.
    pub score: Option<ScoreVerdict>,
    /// Explicit boolean risk-factor observations reported by this provider.
    pub flags: Vec<(RiskFactor, bool)>,
}

/// Why a provider probe did not yield [`ProviderData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFailure {
    /// Anti-bot or WAF challenge (such as Cloudflare Turnstile).
    BlockedWaf,
    /// Rate limited (`HTTP 429` or quota exceeded).
    RateLimited,
    /// Endpoint has no record or coverage for this IP family.
    NoAddress,
    /// Request timed out.
    TimedOut,
    /// Unexpected HTTP status, transport error or payload shape.
    Unexpected(String),
}

impl ProviderFailure {
    /// Converts a provider failure into a [`ScoreVerdict`] for providers in the score table.
    #[must_use]
    pub fn into_score(self) -> ScoreVerdict {
        match self {
            Self::BlockedWaf => ScoreVerdict::BlockedWaf,
            Self::RateLimited => ScoreVerdict::RateLimited,
            Self::NoAddress => ScoreVerdict::NoAddress,
            Self::TimedOut => ScoreVerdict::TimedOut,
            Self::Unexpected(why) => ScoreVerdict::Unexpected(why),
        }
    }
}

/// Resolves the target URL: `{base_url}/{id}/{ip}` when `base_url` is set, else `real_url`.
#[must_use]
pub fn endpoint(base_url: Option<&str>, id: &str, ip: IpAddr, real_url: &str) -> String {
    match base_url {
        Some(base) => format!("{base}/{id}/{ip}"),
        None => real_url.to_owned(),
    }
}

/// Sends a request built for `primary`; if the provider's own hostname has no DNS record of
/// `primary` (`HttpError::NoAddress`) and `fallback` is available, retries over `fallback` since
/// the target `ip` is embedded in the request URL.
pub async fn send_for_ip(
    ctx: &Context<Output>,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    build: impl Fn(IpFamily) -> HttpRequest,
) -> Result<HttpResponse, ProviderFailure> {
    let first = ctx.probe(&build(primary)).await;
    match first {
        Ok(res) => Ok(res),
        Err(HttpError::NoAddress) if fallback.is_some() => {
            let alt = fallback.unwrap_or(primary);
            ctx.probe(&build(alt)).await.map_err(|err| map_error(&err))
        }
        Err(err) => Err(map_error(&err)),
    }
}

/// Maps an [`HttpError`] to a [`ProviderFailure`].
#[must_use]
pub fn map_error(error: &HttpError) -> ProviderFailure {
    match error {
        HttpError::NoAddress => ProviderFailure::NoAddress,
        HttpError::TimedOut => ProviderFailure::TimedOut,
        HttpError::TooManyRedirects => ProviderFailure::Unexpected("too many redirects".to_owned()),
        HttpError::InvalidRedirect(why) => {
            ProviderFailure::Unexpected(format!("invalid redirect: {why}"))
        }
        HttpError::NotHttps(_) => ProviderFailure::Unexpected("non-HTTPS endpoint".to_owned()),
        HttpError::Transport(why) => ProviderFailure::Unexpected(why.clone()),
        HttpError::Cancelled => ProviderFailure::Unexpected("interrupted".to_owned()),
    }
}

/// Universal rate-limit (`429`), anti-bot/WAF challenge, and `5xx` check run before provider
/// parsers. Never attempts to solve or bypass a challenge.
#[must_use]
pub fn gate(response: &HttpResponse) -> Option<ProviderFailure> {
    if response.status == 429 {
        return Some(ProviderFailure::RateLimited);
    }
    if is_waf_challenge(response) {
        return Some(ProviderFailure::BlockedWaf);
    }
    if response.status >= 500 {
        return Some(ProviderFailure::Unexpected(format!(
            "HTTP {}",
            response.status
        )));
    }
    None
}

/// Detects Cloudflare Turnstile, Cloudflare JS challenge, DataDome, PerimeterX, Imperva, and
/// Akamai anti-bot gates.
#[must_use]
pub fn is_waf_challenge(response: &HttpResponse) -> bool {
    crate::waf::is_challenge(response)
}

/// Parses the response body as JSON.
#[must_use]
pub fn json(response: &HttpResponse) -> Option<Value> {
    serde_json::from_slice(&response.body).ok()
}

/// Normalizes a two-letter ISO country code (`"sg"` -> `"SG"`), rejecting placeholders (`XX`,
/// `ZZ`, `EU`, `AP`).
#[must_use]
pub fn country_code(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() == 2 && trimmed.bytes().all(|b| b.is_ascii_alphabetic()) {
        let upper = trimmed.to_ascii_uppercase();
        if !matches!(upper.as_str(), "XX" | "ZZ" | "EU" | "AP") {
            return Some(upper);
        }
    }
    None
}

/// Normalizes an RIR authority name (`"ripe"` -> `"RIPE"`, `"arin"` -> `"ARIN"`).
#[must_use]
pub fn rir_name(raw: &str) -> Option<String> {
    let upper = raw.trim().to_ascii_uppercase();
    if upper.contains("ARIN") {
        Some("ARIN".to_owned())
    } else if upper.contains("RIPE") {
        Some("RIPE".to_owned())
    } else if upper.contains("APNIC") {
        Some("APNIC".to_owned())
    } else if upper.contains("LACNIC") {
        Some("LACNIC".to_owned())
    } else if upper.contains("AFRINIC") {
        Some("AFRINIC".to_owned())
    } else {
        None
    }
}

/// Parses an ASN string such as `"AS13335"` or `"13335"`.
#[must_use]
pub fn parse_asn(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    let digits = trimmed
        .strip_prefix("AS")
        .or_else(|| trimmed.strip_prefix("as"))
        .unwrap_or(trimmed)
        .split_whitespace()
        .next()?;
    let num: u32 = digits.parse().ok()?;
    (num > 0).then_some(num)
}

/// Maps a usage-type label from providers onto [`UsageType`].
#[must_use]
pub fn usage_from_str(raw: &str) -> Option<UsageType> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "isp" | "residential" | "fixed line isp" | "comcast" => Some(UsageType::Residential),
        "hosting" | "datacenter" | "data center" | "dch" | "cloud" | "server" => {
            Some(UsageType::Hosting)
        }
        "business" | "corporate" | "commercial" | "com" | "banking" => Some(UsageType::Business),
        "mobile" | "cellular" | "wireless" | "mob" => Some(UsageType::Mobile),
        "education" | "edu" | "university" | "college" => Some(UsageType::Education),
        "government" | "gov" | "military" | "mil" => Some(UsageType::Government),
        "cdn" | "content delivery network" => Some(UsageType::ContentDelivery),
        _ => None,
    }
}

/// Maps a `0..=100` risk score to [`RiskLevel`].
#[must_use]
pub const fn level_from_score(score: u8) -> RiskLevel {
    if score < 25 {
        RiskLevel::Low
    } else if score < 50 {
        RiskLevel::Medium
    } else if score < 75 {
        RiskLevel::High
    } else {
        RiskLevel::VeryHigh
    }
}

/// Extracts the substring between `prefix` and `suffix`.
#[must_use]
pub fn extract_between<'a>(haystack: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = haystack.find(prefix)? + prefix.len();
    let rest = &haystack[start..];
    let end = rest.find(suffix)?;
    Some(&rest[..end])
}

/// Cleans a trimmed non-empty string field.
#[must_use]
pub fn clean_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
