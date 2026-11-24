//! What route tracing traced: for each family and target, every hop with the addresses that
//! answered, or why the target has no route. Numbers, the names that the lists and NextTrace's
//! service give, and kinds of reasons; the reporter words them.

use std::net::IpAddr;

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::NEXTTRACE;
use bench_tool::job;
use serde::Serialize;

use super::lists::ListError;
use super::lookup::LookupError;
use super::targets::Choice;
use super::targets::Group;

/// Route tracing's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// What the configuration chose.
    pub settings: Settings,
    /// The ids asked for that neither the list nor the cache servers have, as they were given.
    pub unknown: Vec<String>,
    /// How the list of Netflix's exchange ports served.
    pub exchanges: ExchangeList,
    /// The targets traced over IPv4, in order: the cache servers, then the list's.
    pub ipv4: Vec<Traced>,
    /// The targets traced over IPv6, in the same order.
    pub ipv6: Vec<Traced>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        let ran = self
            .ipv4
            .iter()
            .chain(&self.ipv6)
            .any(|traced| matches!(traced.route, Route::Traced(_)));
        if !ran {
            return Vec::new();
        }
        vec![ToolVersion {
            name: NEXTTRACE.name,
            version: NEXTTRACE.version.to_owned(),
        }]
    }
}

/// What the configuration chose; the defaults trace both cache servers and the list's defaults
/// from this repository's list, with each hop's network and place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// The groups chosen; none for the default.
    pub groups: Vec<Choice>,
    /// The ids chosen, which win over the groups; none for the default.
    pub targets: Vec<String>,
    /// Whether the targets came from another list than this repository's.
    pub list: bool,
    /// Whether NextTrace looked up each hop's network and place.
    pub geo: bool,
}

/// How the list of Netflix's exchange ports served, which only names exchanges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ExchangeList {
    /// It was read: the ports that it names.
    Read(usize),
    /// The program could not get it: its reason.
    Missing(String),
    /// It could not be read, for the system's reason.
    Unreadable(String),
    /// It is not a list that the item can use.
    Unusable(ListError),
}

/// One target, over one family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Traced {
    /// Which target.
    pub target: Target,
    /// The DNS name or address that NextTrace traced; none when a cache server's lookup failed.
    pub host: Option<String>,
    /// What came of it.
    pub route: Route,
}

/// A target that the item traces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Target {
    /// YouTube's nearest cache server.
    YouTube,
    /// Netflix's nearest cache server, with its place when the lookup named it.
    Netflix(Option<Place>),
    /// A target of the list.
    Listed {
        /// Its id.
        id: String,
        /// Its group.
        group: Group,
        /// Its name.
        name: String,
    },
}

impl Target {
    /// Its group.
    #[must_use]
    pub fn group(&self) -> Group {
        match self {
            Self::YouTube | Self::Netflix(_) => Group::Services,
            Self::Listed { group, .. } => *group,
        }
    }
}

/// A cache server's place, as fast.com's API names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    /// Its city.
    pub city: String,
    /// Its country.
    pub country: String,
}

/// What came of tracing one target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Route {
    /// The route, hop by hop.
    Traced(Trace),
    /// The cache server could not be looked up.
    Unavailable(Unavailable),
    /// NextTrace gave no route.
    Failed(TraceError),
}

/// Why a cache server could not be looked up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Unavailable {
    /// The program could not get the answer: its reason.
    Unfetched(String),
    /// The answer could not be read, for the system's reason.
    Unreadable(String),
    /// The answer names no server that the item can trace.
    Answer(LookupError),
}

/// Why NextTrace gave no route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TraceError {
    /// NextTrace's error, as its last line says it, up to its first character outside printable
    /// ASCII and without the time that it starts with, such as `DNS lookup failed: ...`.
    Error(String),
    /// NextTrace exited with 0 without a trace.
    Nothing,
    /// The run went wrong as any tool's run can, such as past its limit.
    Fault(job::Fault),
}

/// A route, hop by hop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trace {
    /// Whether the target answered.
    pub reached: bool,
    /// The hops from the first, one for each TTL that NextTrace probed.
    pub hops: Vec<Hop>,
}

/// One hop: the addresses that answered its probes, in the order in which they first answered;
/// none for a hop that no probe got an answer from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hop {
    /// The addresses that answered.
    pub answers: Vec<Answer>,
}

/// An address that answered at a hop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Answer {
    /// The address.
    pub address: IpAddr,
    /// Its Autonomous System number as NextTrace's service names it, such as `AS4809`; none when
    /// the service names none.
    pub asn: Option<String>,
    /// Its ISP or owner as NextTrace's service names it, such as `China Telecom CN2`; none when
    /// the service names none.
    pub network: Option<String>,
    /// The internet exchange where Netflix's network takes this address, from the list.
    pub exchange: Option<String>,
    /// Its place as NextTrace's service names it, country, province and city, such as `China
    /// Beijing Beijing`; none when the service names none.
    pub place: Option<String>,
    /// The smallest round-trip time of its answers, in microseconds.
    pub latency: u64,
}

/// Why route tracing has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The target list could not be read, for the system's reason.
    Unreadable(String),
    /// The target list is not one that the item can use.
    List(ListError),
    /// No target was chosen; the item skips. With the ids asked for that no target has.
    NoTarget(Vec<String>),
    /// No chosen target has a name of a family that the host reaches; the item skips.
    NoFamily,
    /// No target got a route; each one's reason is in the published result.
    NoRoute,
    /// The plugin was cancelled.
    Cancelled,
}
