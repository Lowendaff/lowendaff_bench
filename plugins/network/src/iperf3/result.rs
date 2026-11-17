//! What network speed measured: for each server and family, the speed each way and the latency,
//! or why a direction has none. Numbers and kinds of reasons only, with iperf3's own error
//! messages, which name no address of the host's; the reporter words them.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::IPERF3;
use bench_tool::job;
use serde::Serialize;

use super::servers::ListError;

/// Network speed's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// The version that iperf3 reported, such as `3.22`.
    pub version: Option<String>,
    /// How each test ran.
    pub settings: Settings,
    /// The countries or server ids that were asked for and that no server of the list has.
    pub unknown: Vec<String>,
    /// The servers tested over IPv4, in the list's order.
    pub ipv4: Vec<Tested>,
    /// The servers tested over IPv6, in the list's order.
    pub ipv6: Vec<Tested>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: IPERF3.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// How each test runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// The seconds each way.
    pub seconds: u16,
    /// The parallel streams.
    pub streams: u8,
    /// The most Mbit/s that a stream sends; `None` for no limit.
    pub rate: Option<u32>,
}

impl Settings {
    /// YABS's: 8 streams, 10 seconds each way, without a limit.
    pub const YABS: Self = Self {
        seconds: 10,
        streams: 8,
        rate: None,
    };
}

/// One server, over one family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tested {
    /// Its place, such as `Tokyo`.
    pub location: String,
    /// Its country's ISO 3166 code, such as `JP`.
    pub country: String,
    /// Who runs it.
    pub provider: String,
    /// The upload: the speed that the server received.
    pub upload: Way,
    /// The download: the speed that the host received.
    pub download: Way,
}

impl Tested {
    /// The latency to the server: the smallest round-trip time of the upload, in microseconds.
    #[must_use]
    pub fn latency(&self) -> Option<u64> {
        match &self.upload {
            Way::Measured(speed) => speed.rtt,
            _ => None,
        }
    }
}

/// What the tries of one direction came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Way {
    /// A speed, on the try that measured it.
    Measured(Speed),
    /// The server was busy on every try.
    Busy,
    /// The server could not be reached on any try.
    Unreachable,
    /// iperf3's own error message on the last try.
    Error(String),
    /// The last try went wrong as any tool's run can, such as past its limit.
    Fault(job::Fault),
    /// iperf3 reported nothing that it measured on the last try.
    Nothing,
    /// Not tried: the server could not be reached for the upload.
    Skipped,
}

/// A speed measured one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Speed {
    /// The bits per second that the receiving side counted.
    pub bits: u64,
    /// The try that measured it, from 1.
    pub tries: u32,
    /// For an upload, the smallest round-trip time of its streams, in microseconds, when the
    /// kernel reported one.
    pub rtt: Option<u64>,
    /// For an upload, the segments that its streams sent again.
    pub retransmits: Option<u64>,
}

/// Why network speed has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The server list could not be read, for the system's reason.
    Unreadable(String),
    /// The server list is not one that the item can use.
    List(ListError),
    /// No server of the list was chosen; the item skips. With the countries or ids asked for
    /// that no server has.
    NoServer(Vec<String>),
    /// None of the chosen servers answers over a family that the host has; the item skips.
    NoFamily,
    /// No server gave a speed either way; each one's reason is in the published result.
    NoResult,
    /// The plugin was cancelled.
    Cancelled,
}
