//! Pure data types for `ip.quality`: network identity, native/broadcast origin, usage type,
//! multi-source risk scores and risk factors, DNSBL blacklist hits, and port-25 mail checks.

use std::net::IpAddr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Serialize;

/// Effective settings recorded in [`Output`] for the report's method line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Whether DNSBL blacklist checks ran.
    pub dnsbl: bool,
    /// Whether outbound TCP port 25 and mail provider checks ran.
    pub mail: bool,
    /// Whether loopback/custom endpoint overrides were configured.
    pub custom_endpoints: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dnsbl: true,
            mail: true,
            custom_endpoints: false,
        }
    }
}

/// Network usage classification of an IP range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum UsageType {
    /// Consumer or home broadband ISP (`isp`).
    Residential,
    /// Cloud, VPS or colocation facility (`hosting` / `datacenter`).
    Hosting,
    /// Corporate or enterprise network (`business`).
    Business,
    /// Cellular or wireless carrier (`mobile`).
    Mobile,
    /// University or research network (`education`).
    Education,
    /// Public-sector or government network (`government`).
    Government,
    /// Anycast or edge content delivery network (`cdn`).
    ContentDelivery,
}

/// Whether the IP operates in the country where its RIR block is registered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Origin {
    /// RIR registration country matches the operational GeoIP consensus country.
    Native {
        /// Two-letter ISO 3166-1 alpha-2 country code.
        country: String,
    },
    /// RIR registration country differs from the operational GeoIP consensus country.
    Broadcast {
        /// Country recorded in RIR WHOIS registration.
        registered: String,
        /// Operational GeoIP consensus country.
        located: String,
    },
    /// Registration country or GeoIP country could not be determined.
    Unknown,
}

/// Normalized qualitative risk tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RiskLevel {
    /// Clean / low risk.
    Low,
    /// Elevated / medium risk.
    Medium,
    /// High risk.
    High,
    /// Severe / very high risk.
    VeryHigh,
}

/// GreyNoise Community classification for an IP address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GreyNoiseClass {
    /// Not observed scanning the Internet in the last 90 days.
    NotObserved,
    /// Observed, classified benign (or known RIOT service).
    Benign,
    /// Observed conducting opportunistic malicious scanning.
    Malicious,
    /// Observed scanning with unknown intent.
    Unknown,
}

/// Outcome of one risk-scoring or reconnaissance source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ScoreVerdict {
    /// Integer fraud/risk score on a fixed scale (`score/max`), such as `12/100`.
    Numeric {
        /// Reported score.
        score: u8,
        /// Scale maximum (typically `100`).
        max: u8,
        /// Qualitative tier.
        level: RiskLevel,
    },
    /// Decimal abuse ratio reported by `ipapi.is`, such as `"0.0039"`.
    Ratio {
        /// Formatted ratio text as reported by the provider.
        text: String,
        /// Qualitative tier.
        level: RiskLevel,
    },
    /// GreyNoise Community Internet-scanner telemetry.
    GreyNoise {
        /// Known benign business/CDN service (`riot: true`).
        riot: bool,
        /// Observed mass-scanning the Internet (`noise: true`).
        noise: bool,
        /// Intent classification.
        classification: GreyNoiseClass,
    },
    /// Shodan InternetDB reconnaissance summary.
    Shodan {
        /// Number of open ports observed by Shodan.
        ports: usize,
        /// Number of CVEs associated with observed banners.
        vulns: usize,
        /// Whether threat tags (`compromised`, `malware`, `scanner`, `vpn`, `proxy`, `tor`) hit.
        flagged: bool,
    },
    /// Source presented an anti-bot or WAF challenge (never bypassed).
    BlockedWaf,
    /// Source returned `HTTP 429` or quota exhaustion.
    RateLimited,
    /// Source does not cover or resolve for this IP family.
    NoAddress,
    /// Request timed out.
    TimedOut,
    /// Unexpected status or payload, surfaced as a note.
    Unexpected(String),
}

/// Categorical risk factor aggregated across intelligence sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum RiskFactor {
    /// Open, web or residential proxy.
    Proxy,
    /// Commercial or private VPN endpoint.
    Vpn,
    /// Tor exit or relay node.
    Tor,
    /// Hosting, cloud or server infrastructure.
    Datacenter,
    /// Observed abuse, spam, brute-force scanning or compromise.
    Abuser,
    /// Automated crawler or search bot.
    Robot,
    /// Reserved or unrouted bogon space.
    Bogon,
}

impl RiskFactor {
    /// Display order in the report's risk-factor table.
    pub const ALL: [Self; 7] = [
        Self::Proxy,
        Self::Vpn,
        Self::Tor,
        Self::Datacenter,
        Self::Abuser,
        Self::Robot,
        Self::Bogon,
    ];
}

/// Aggregated votes for one [`RiskFactor`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FactorSummary {
    /// The risk factor.
    pub factor: RiskFactor,
    /// Number of sources that reported an explicit boolean for this factor.
    pub checked: usize,
    /// Display names of sources that flagged this factor `true`.
    pub flagged_by: Vec<&'static str>,
}

/// Country vote count across GeoIP sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeoVote {
    /// Two-letter ISO country code.
    pub country: String,
    /// Number of GeoIP sources reporting `country`.
    pub count: usize,
}

/// Aggregated DNSBL blacklist scan result for one IP address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DnsblSummary {
    /// Zones that returned a definitive answer (`clean` or `listed`).
    pub checked: usize,
    /// Zones skipped due to resolver refusal (`127.255.255.0/24`) or lookup timeout.
    pub refused: usize,
    /// Display names of DNSBL zones where the IP address is listed.
    pub listed: Vec<&'static str>,
}

/// Outcome of a non-intrusive TCP port-25 SMTP banner check against one mail provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MailStatus {
    /// Connected and received a `2xx` greeting code (typically `220`).
    Deliverable(u16),
    /// Connected, but the MX server rejected the session at banner time (`4xx` or `5xx`).
    Rejected(u16),
    /// Connection timed out, was refused, or had no route (port 25 blocked).
    Blocked,
    /// Mail provider's MX hostname has no DNS record for this IP family.
    NoAddress,
    /// Unexpected reply format.
    Unexpected(String),
}

/// Result of checking one mail provider's inbound MX server on port 25.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MailCheck {
    /// Stable lowercase identifier, such as `gmail`.
    pub id: &'static str,
    /// Display name, such as `Gmail`.
    pub name: &'static str,
    /// Banner check result.
    pub status: MailStatus,
}

/// Full reputation and connectivity report for one public IP address (`IPv4` or `IPv6`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyReport {
    /// The host's public IP address of this family (masked by the reporter unless `--show-ip`).
    pub address: IpAddr,
    /// Autonomous System Number, such as `13335` for `AS13335`.
    pub asn: Option<u32>,
    /// Organization or ISP name.
    pub org: Option<String>,
    /// Reverse DNS or observed hostname (shown only with `--show-ip`).
    pub hostname: Option<String>,
    /// Regional Internet Registry, such as `APNIC` or `ARIN`.
    pub rir: Option<String>,
    /// Country recorded in official RIR registration.
    pub registered_country: Option<String>,
    /// Majority-consensus operational GeoIP country.
    pub geo_country: Option<String>,
    /// Vote breakdown across GeoIP sources, highest count first.
    pub geo_votes: Vec<GeoVote>,
    /// Total GeoIP sources that reported a country code.
    pub geo_total: usize,
    /// Native vs broadcast classification.
    pub origin: Origin,
    /// Primary network usage classification.
    pub usage: Option<UsageType>,
    /// Per-source risk scores and telemetry verdicts.
    pub scores: Vec<(&'static str, ScoreVerdict)>,
    /// Aggregated risk factors in [`RiskFactor::ALL`] order.
    pub factors: Vec<FactorSummary>,
    /// DNSBL scan summary when `dnsbl` was enabled.
    pub dnsbl: Option<DnsblSummary>,
    /// Mail provider port-25 checks when `mail` was enabled.
    pub mail: Vec<MailCheck>,
    /// Unexpected provider failures surfaced as report notes.
    pub notes: Vec<(&'static str, String)>,
}

/// Complete output of `ip.quality`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// Effective settings of the run.
    pub settings: Settings,
    /// Whether IPv4 reached the Internet on this host.
    pub ipv4_reachable: bool,
    /// Whether IPv6 reached the Internet on this host.
    pub ipv6_reachable: bool,
    /// IPv4 reputation report when a public IPv4 address was known.
    pub ipv4: Option<Box<FamilyReport>>,
    /// IPv6 reputation report when a public IPv6 address was known.
    pub ipv6: Option<Box<FamilyReport>>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `ip.quality` skipped or failed as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Neither IPv4 nor IPv6 is reachable on this host.
    NoFamily,
    /// Reachable network families had no discovered public IP address.
    NoPublicAddress,
    /// Configured `base_url`, `dns_server` or `smtp_server` failed validation.
    InvalidEndpoint(String),
    /// Run was cancelled before finishing.
    Cancelled,
}
