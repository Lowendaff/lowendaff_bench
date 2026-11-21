//! Pure data types for streaming media unlock results and failure reasons.

use std::fmt;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Deserialize;
use serde::Serialize;

/// Geographic region group of a streaming service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    /// Multinational streaming platforms.
    Global,
    /// Asian streaming platforms (HK, MO, TW, JP, KR, SEA, IN).
    Asia,
    /// European streaming platforms.
    Europe,
    /// North and Latin American streaming platforms.
    Americas,
    /// Australian and New Zealand streaming platforms.
    Oceania,
}

impl Group {
    /// Every group in display and probe order.
    pub const ALL: [Self; 5] = [
        Self::Global,
        Self::Asia,
        Self::Europe,
        Self::Americas,
        Self::Oceania,
    ];

    /// Identifier used in configuration and progress steps.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Asia => "asia",
            Self::Europe => "europe",
            Self::Americas => "americas",
            Self::Oceania => "oceania",
        }
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Group selection in `[plugins."ip.media"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// Multinational platforms.
    Global,
    /// Asian platforms.
    Asia,
    /// European platforms.
    Europe,
    /// American platforms.
    Americas,
    /// Oceanian platforms.
    Oceania,
    /// Every group.
    All,
}

impl Choice {
    /// Identifier of this choice in configuration files.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Asia => "asia",
            Self::Europe => "europe",
            Self::Americas => "americas",
            Self::Oceania => "oceania",
            Self::All => "all",
        }
    }

    /// Whether `group` is included by this choice.
    #[must_use]
    pub const fn matches(self, group: Group) -> bool {
        matches!(
            (self, group),
            (Self::All, _)
                | (Self::Global, Group::Global)
                | (Self::Asia, Group::Asia)
                | (Self::Europe, Group::Europe)
                | (Self::Americas, Group::Americas)
                | (Self::Oceania, Group::Oceania)
        )
    }
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Additional detail when a streaming service refuses playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Restriction {
    /// Redirected to Mainland China (`CN`), where the service does not operate.
    ChinaMainland,
    /// Announced as coming soon in the detected region.
    ComingSoon,
    /// Refused explicitly due to proxy, VPN or unblocker detection.
    ProxyDetected,
}

/// Outcome of probing one streaming service over one IP family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Verdict {
    /// Full catalog unlocked, optionally naming the detected region or storefront.
    Yes {
        /// Two-letter ISO 3166 region code (or `HK/MO`), when reported by the service.
        region: Option<String>,
    },
    /// Only self-produced originals are available (such as on Netflix).
    OriginalsOnly {
        /// Two-letter ISO 3166 region code, when reported by the service.
        region: Option<String>,
    },
    /// Geographic restriction or unsupported country.
    No {
        /// Specific restriction detail, if reported.
        restriction: Option<Restriction>,
    },
    /// Blocked by an anti-bot challenge or WAF (such as Cloudflare Turnstile).
    BlockedWaf,
    /// Rate limited by the target (`HTTP 429`).
    RateLimited,
    /// The service's domain has no DNS record of this IP family.
    NoAddress,
    /// The probe timed out.
    TimedOut,
    /// Network failure or unrecognized answer, kept for a note under the table.
    Unexpected(String),
}

impl Verdict {
    /// Unlocked without a specific region code.
    #[must_use]
    pub const fn yes() -> Self {
        Self::Yes { region: None }
    }

    /// Unlocked in `region` (uppercased).
    #[must_use]
    pub fn yes_in(region: impl Into<String>) -> Self {
        Self::Yes {
            region: Some(region.into().to_ascii_uppercase()),
        }
    }

    /// Blocked in the current region without further qualifier.
    #[must_use]
    pub const fn no() -> Self {
        Self::No { restriction: None }
    }
}

/// Result of one streaming service across IPv4 and IPv6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceResult {
    /// Stable identifier, such as `netflix` or `bahamut`.
    pub id: &'static str,
    /// Display name of the streaming platform, such as `Netflix` or `BBC iPlayer`.
    pub name: &'static str,
    /// Geographic group the platform belongs to.
    pub group: Group,
    /// IPv4 verdict, or `None` when IPv4 was not reachable or not yet probed.
    pub ipv4: Option<Verdict>,
    /// IPv6 verdict, or `None` when IPv6 was not reachable or not yet probed.
    pub ipv6: Option<Verdict>,
}

/// Configuration snapshot recorded in [`Output`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Settings {
    /// Groups requested in configuration; empty means all groups.
    pub groups: Vec<Choice>,
    /// Specific service ids requested; empty means group selection applies.
    pub services: Vec<String>,
    /// Whether `base_url` overrode the built-in endpoints.
    pub custom_base: bool,
}

/// Complete or partial measurement of `ip.media`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Output {
    /// Effective settings of the run.
    pub settings: Settings,
    /// Unknown service ids named in `services`.
    pub unknown: Vec<String>,
    /// Whether the host had IPv4 reachability.
    pub ipv4_reachable: bool,
    /// Whether the host had IPv6 reachability.
    pub ipv6_reachable: bool,
    /// Probed services in catalog order.
    pub results: Vec<ServiceResult>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `ip.media` skipped or failed as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// `base_url` is neither an `https://` URL nor a loopback `http://` URL.
    InvalidBaseUrl(String),
    /// None of the requested `services` exists in the catalog.
    NoService(Vec<String>),
    /// Neither IPv4 nor IPv6 is reachable on this host.
    NoFamily,
    /// The run was interrupted before finishing.
    Cancelled,
}
