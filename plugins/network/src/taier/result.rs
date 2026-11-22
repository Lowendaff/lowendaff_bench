//! Typed results and failure reasons for `network.taier` (China carrier speedtest).

use std::fmt;
use std::net::Ipv4Addr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Deserialize;
use serde::Serialize;

use super::nodes::ListError;

/// Carrier network of a CAICT GlobalSpeed node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operator {
    /// China Telecom (`AS4134` / `AS4809`).
    Telecom,
    /// China Unicom (`AS4837` / `AS9929`).
    Unicom,
    /// China Mobile (`AS9808` / `AS58807`).
    Mobile,
    /// China Education and Research Network (`CERNET`).
    Cernet,
    /// China Broadnet (`CBN`).
    Cbn,
    /// Overseas or unlisted carrier.
    Other,
}

impl Operator {
    /// Canonical lowercase code used in configuration, CLI flags, and progress steps.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Telecom => "telecom",
            Self::Unicom => "unicom",
            Self::Mobile => "mobile",
            Self::Cernet => "cernet",
            Self::Cbn => "cbn",
            Self::Other => "other",
        }
    }

    /// Compact carrier alias used in `<place>:<op>[+<op>...]` selectors and TUI badges.
    #[must_use]
    pub const fn short_code(self) -> &'static str {
        match self {
            Self::Telecom => "ct",
            Self::Unicom => "cu",
            Self::Mobile => "cm",
            Self::Cernet => "edu",
            Self::Cbn => "cbn",
            Self::Other => "other",
        }
    }

    /// Canonical execution order across carriers.
    pub const ALL: [Self; 6] = [
        Self::Telecom,
        Self::Unicom,
        Self::Mobile,
        Self::Cernet,
        Self::Cbn,
        Self::Other,
    ];

    /// Default three major mainland Chinese carriers.
    pub const DEFAULTS: [Self; 3] = [Self::Telecom, Self::Unicom, Self::Mobile];
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Configuration selector for `operators` in `network.taier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperatorChoice {
    /// China Telecom.
    Telecom,
    /// China Unicom.
    Unicom,
    /// China Mobile.
    Mobile,
    /// CERNET.
    Cernet,
    /// China Broadnet.
    Cbn,
    /// Other / overseas.
    Other,
    /// Every carrier present in the selected locations.
    All,
}

impl OperatorChoice {
    /// Parses a canonical carrier name or short alias (`ct`, `cu`, `cm`, `edu`, `cbn`, `other`,
    /// `all`), ignoring ASCII case.
    #[must_use]
    pub fn parse_token(raw: &str) -> Option<Self> {
        let lower = raw.trim().to_ascii_lowercase();
        match lower.as_str() {
            "telecom" | "ct" => Some(Self::Telecom),
            "unicom" | "cu" => Some(Self::Unicom),
            "mobile" | "cm" => Some(Self::Mobile),
            "cernet" | "edu" => Some(Self::Cernet),
            "cbn" => Some(Self::Cbn),
            "other" => Some(Self::Other),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// Compact code written in `<place>:<op>` selector strings.
    #[must_use]
    pub const fn short_code(self) -> &'static str {
        match self {
            Self::Telecom => "ct",
            Self::Unicom => "cu",
            Self::Mobile => "cm",
            Self::Cernet => "edu",
            Self::Cbn => "cbn",
            Self::Other => "other",
            Self::All => "all",
        }
    }
}

/// Effective measurement parameters recorded in [`Output`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Transfer window in seconds per direction (`4` by default).
    pub seconds: u16,
    /// Parallel TCP streams per direction (`8` by default).
    pub streams: u8,
    /// Explicit `<place>[:<op>[+<op>...]]` selectors when `--taier` or the TUI node picker was
    /// used.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub selections: Vec<String>,
    /// Requested city/province filters (empty when defaults were used).
    pub cities: Vec<String>,
    /// Resolved carrier filter in execution order.
    pub operators: Vec<Operator>,
    /// Whether a custom `list` path or URL overrode the default Taier server list.
    pub custom_list: bool,
}

impl Settings {
    /// Standard default parameters (`4` s each way, `8` parallel streams).
    pub const DEFAULT_SECONDS: u16 = 4;
    /// Standard default stream count.
    pub const DEFAULT_STREAMS: u8 = 8;
}

/// Successful throughput readings for one `(location, operator)` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NodeMeasured {
    /// Sustained download throughput in bits per second.
    pub download_bps: u64,
    /// Sustained upload throughput in bits per second.
    pub upload_bps: u64,
}

/// Outcome of testing one `(location, operator)` slot across its candidate nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "data", rename_all = "snake_case")]
pub enum NodeOutcome {
    /// Download and upload completed.
    Measured(NodeMeasured),
    /// Every candidate node in this slot returned `dovalid: -1` (queue full / busy).
    Busy,
    /// No candidate node in this slot accepted TCP connections.
    Unreachable,
    /// Node rejected `dovalid` authentication (`0` or `2`).
    AuthRejected,
    /// Handshake or stream timed out on the last candidate node.
    TimedOut,
    /// Unexpected protocol or stream failure on the last candidate node.
    Failed(String),
}

/// Result row for one `(location, operator)` carrier target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestedNode {
    /// Location identifier (e.g. `beijing` or `jiangsu-nanjing`).
    pub id: String,
    /// ASCII province or region name (e.g. `Beijing` or `Jiangsu`).
    pub province: String,
    /// ASCII city name (e.g. `Beijing` or `Nanjing`).
    pub city: String,
    /// Carrier network.
    pub operator: Operator,
    /// IPv4 address of the node that answered (or the last candidate tried).
    pub ip: Ipv4Addr,
    /// TCP port of the node that answered (or the last candidate tried).
    pub port: u16,
    /// Number of candidate nodes tried in this `(location, operator)` slot (`>= 1`).
    pub tried_nodes: u8,
    /// Measurement outcome.
    pub outcome: NodeOutcome,
}

/// Complete output of `network.taier`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// Effective run settings.
    pub settings: Settings,
    /// City/province tokens requested by the user that matched no location in the list.
    pub unknown: Vec<String>,
    /// Tested `(location, operator)` slots, in execution order.
    pub results: Vec<TestedNode>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// Why `network.taier` skipped or failed as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The downloaded or local node list file could not be read.
    Unreadable(String),
    /// The node list failed validation.
    List(ListError),
    /// No `(location, operator)` node matched the configuration.
    NoNode(Vec<String>),
    /// Every attempted `(location, operator)` slot failed to complete a speed test.
    NoResult,
    /// Interrupted via `Ctrl-C` or timeout cancellation.
    Cancelled,
}
