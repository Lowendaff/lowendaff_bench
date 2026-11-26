//! Structured output, configuration, and failure types for `network.tcping`.

use std::fmt;
use std::num::NonZeroU8;
use std::str::FromStr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Deserialize;
use serde::Serialize;
use serde::de;

/// Minimum probes allowed per target (`--tcping-count`).
pub const MIN_COUNT: u8 = 5;

/// Maximum probes allowed per target (`--tcping-count`).
pub const MAX_COUNT: u8 = 50;

/// Default probes sent per target (`--tcping-count`).
pub const DEFAULT_COUNT: u8 = 10;

/// Validated probe count per target (`5..=50`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProbeCount(NonZeroU8);

impl ProbeCount {
    /// Default handshake probe count (`10`).
    pub const DEFAULT: Self = match NonZeroU8::new(DEFAULT_COUNT) {
        Some(v) => Self(v),
        None => unreachable!(),
    };

    /// Validates that `raw` lies within `5..=50`.
    #[must_use]
    pub fn new(raw: u8) -> Option<Self> {
        if (MIN_COUNT..=MAX_COUNT).contains(&raw) {
            NonZeroU8::new(raw).map(Self)
        } else {
            None
        }
    }

    /// Underlying `u8` probe count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl Default for ProbeCount {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'de> Deserialize<'de> for ProbeCount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = u8::deserialize(deserializer)?;
        Self::new(raw).ok_or_else(|| {
            de::Error::custom(format!(
                "probe count {raw} must be {MIN_COUNT}..={MAX_COUNT}"
            ))
        })
    }
}

/// Target group in `servers/tcping.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Mainland China provincial standard carrier routes (`AS4134`, `AS4837`, `AS9808`).
    China,
    /// Mainland China premium international backbones (`AS4809`, `AS9929`, `AS58807`).
    ChinaPremium,
    /// Single-region global Internet hubs across Asia-Pacific, Americas, and Europe.
    Global,
}

impl Group {
    /// Stable lowercase snake_case group code.
    #[must_use]
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::China => "china",
            Self::ChinaPremium => "china_premium",
            Self::Global => "global",
        }
    }
}

/// User-facing group selector (`--tcping-groups`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum GroupChoice {
    /// Mainland China provincial carrier targets.
    #[serde(rename = "china")]
    China,
    /// Mainland China premium backbone targets (`CN2`, `9929`, `CMIN2`).
    #[serde(rename = "china-premium", alias = "china_premium")]
    ChinaPremium,
    /// Global single-region hub targets.
    #[serde(rename = "global")]
    Global,
    /// Every group.
    #[serde(rename = "all")]
    All,
}

impl GroupChoice {
    /// Canonical CLI flag token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::China => "china",
            Self::ChinaPremium => "china-premium",
            Self::Global => "global",
            Self::All => "all",
        }
    }
}

impl fmt::Display for GroupChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GroupChoice {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "china" => Ok(Self::China),
            "china-premium" | "china_premium" => Ok(Self::ChinaPremium),
            "global" => Ok(Self::Global),
            "all" => Ok(Self::All),
            other => Err(other.to_owned()),
        }
    }
}

/// Carrier classification of a TCP latency target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// China Telecom (`AS4134` / `AS4809`).
    Telecom,
    /// China Unicom (`AS4837` / `AS9929`).
    Unicom,
    /// China Mobile (`AS9808` / `AS58807`).
    Mobile,
    /// China Education and Research Network (`AS4538`).
    Cernet,
    /// China Broadnet.
    Cbn,
    /// International or cloud/IX provider.
    Other,
}

impl Operator {
    /// Stable lowercase operator code.
    #[must_use]
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::Telecom => "telecom",
            Self::Unicom => "unicom",
            Self::Mobile => "mobile",
            Self::Cernet => "cernet",
            Self::Cbn => "cbn",
            Self::Other => "other",
        }
    }
}

/// Effective configuration recorded in [`Output`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Requested group filters (empty means all groups with default curation).
    pub groups: Vec<GroupChoice>,
    /// Whether every catalog target in the selected groups was requested (`--tcping-all`).
    pub all_targets: bool,
    /// Handshake probes sent per active endpoint (`5..=50`).
    pub count: u8,
}

/// Latency distribution and packet-loss metrics for one probed `(target, family)` pair.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LatencyMeasured {
    /// Hostname or IP literal of the endpoint actually measured.
    pub endpoint: String,
    /// TCP port of the endpoint actually measured.
    pub port: u16,
    /// Resolved remote IP address connected to.
    pub resolved_ip: String,
    /// Whether probing failed over from `candidates[0]` (`primary`) to `candidates[1]` (`backup`).
    pub used_backup: bool,
    /// Total TCP handshake probes sent against the active endpoint.
    pub sent: u8,
    /// Successful TCP handshakes (`SYN-ACK` or remote `RST`) received.
    pub received: u8,
    /// Packet-loss percentage (`0.0..=100.0`, rounded to one decimal place).
    pub loss_pct: f64,
    /// Minimum RTT in milliseconds (`None` when `received == 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ms: Option<f64>,
    /// Arithmetic mean RTT in milliseconds (`None` when `received == 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_ms: Option<f64>,
    /// Median (`P50`) RTT in milliseconds (`None` when `received == 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_ms: Option<f64>,
    /// 95th-percentile (`P95`) RTT in milliseconds (`None` when `received == 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_ms: Option<f64>,
    /// Maximum RTT in milliseconds (`None` when `received == 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
    /// Mean consecutive `|RTT_i - RTT_{i-1}|` jitter in milliseconds (`None` when `received == 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jitter_ms: Option<f64>,
}

/// Outcome of probing one `(target, family)` pair.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TargetOutcome {
    /// Received at least one TCP handshake response (`received > 0`).
    Measured(LatencyMeasured),
    /// Sent probes to a resolved endpoint with `100%` packet loss (`received == 0`).
    Loss(LatencyMeasured),
    /// DNS resolution or immediate local socket routing failed for every candidate.
    Failed {
        /// Diagnostic failure summary.
        reason: String,
    },
}

/// One probed `(target, family)` entry in [`Output`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestedTarget {
    /// Stable target identifier (e.g. `cn-sh-ct`, `cn-sh-cn2-4809`, `global-jp-tyo`).
    pub id: String,
    /// Target group (`china`, `china_premium`, or `global`).
    pub group: Group,
    /// Region or province name (e.g. `Shanghai`, `Asia-Pacific`).
    pub region: String,
    /// City name (e.g. `Shanghai`, `Tokyo`).
    pub city: String,
    /// Display name (e.g. `Shanghai Telecom`, `Shanghai Telecom CN2`, `Tokyo, JP`).
    pub name: String,
    /// Carrier classification.
    pub operator: Operator,
    /// Expected autonomous system number.
    pub asn: u32,
    /// `true` when probed over IPv6, `false` when probed over IPv4.
    pub ipv6: bool,
    /// Measurement outcome.
    #[serde(flatten)]
    pub outcome: TargetOutcome,
}

/// Complete or partial `network.tcping` measurement result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Effective run settings.
    pub settings: Settings,
    /// Probed `(target, family)` entries in canonical `(IPv4, IPv6) x group` order.
    pub targets: Vec<TestedTarget>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `network.tcping` skipped or failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Failure {
    /// Orchestrator did not provide the target list file.
    MissingList,
    /// Target list file could not be read.
    Unreadable(String),
    /// Target list exceeded 128 KiB.
    TooLarge,
    /// Target list was not valid JSON of the expected top-level shape.
    InvalidJson(String),
    /// Target list used a newer or unknown `version`.
    UnsupportedVersion(u64),
    /// Target list held `0` or more than `300` targets.
    TargetCount(usize),
    /// Entry `entry` (1-based) violated `field`.
    InvalidEntry {
        /// 1-based target index.
        entry: usize,
        /// Broken field rule.
        field: &'static str,
    },
    /// Two targets shared the same `id`.
    DuplicateId(String),
    /// No targets matched the selected groups and reachable IP families.
    NoTargets,
    /// Every probed target failed without receiving a single TCP handshake response.
    AllUnreachable,
    /// Interrupted via cancellation token.
    Interrupted,
}
