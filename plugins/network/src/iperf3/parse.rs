//! Reading what iperf3 printed with `-J`: the speed that the receiving side counted, the
//! smallest round-trip time and the retransmissions of an upload, iperf3's version, or the error
//! that iperf3 reported. Its JSON also names the host's own address, which nothing here keeps.

use bench_tool::job;
use serde_json::Value;

/// What one try of one direction came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attempt {
    /// iperf3 measured a speed.
    Measured {
        /// The bits per second that the receiving side counted.
        bits: u64,
        /// The smallest round-trip time of the sending streams, in microseconds, when the
        /// kernel reported one; the host's streams send in an upload only.
        rtt: Option<u64>,
        /// The segments that the sending streams sent again, in an upload.
        retransmits: Option<u64>,
    },
    /// The server was busy with another client's test.
    Busy,
    /// The server could not be reached: no answer, a closed port, no route.
    Unreachable,
    /// iperf3 reported this error.
    Error(String),
    /// iperf3 ended without a result that it measured.
    Nothing,
    /// The run went wrong as any tool's run can.
    Fault(job::Fault),
}

/// What iperf3 printed for an upload, or for a download when `download` is set. Its standard
/// output is its JSON; what it printed on its standard error follows, and is not read.
#[must_use]
pub fn attempt(output: &job::Output, download: bool) -> Attempt {
    let Some(json) = first_value(&output.text) else {
        // Its output names the host's own address, so no line of it is kept.
        return match output.code {
            Some(0) => Attempt::Nothing,
            Some(code) => Attempt::Fault(job::Fault::Exit { code, last: None }),
            None => Attempt::Fault(job::Fault::Signal { last: None }),
        };
    };
    if let Some(error) = json.get("error").and_then(Value::as_str) {
        return error_kind(error);
    }
    let bits = json
        .pointer("/end/sum_received/bits_per_second")
        .and_then(Value::as_f64)
        .filter(|bits| bits.is_finite() && *bits >= 0.5);
    let Some(bits) = bits else {
        return Attempt::Nothing;
    };
    let (rtt, retransmits) = if download {
        (None, None)
    } else {
        let rtt = json
            .pointer("/end/streams")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|stream| stream.pointer("/sender/min_rtt").and_then(Value::as_u64))
            .filter(|rtt| *rtt > 0)
            .min();
        let retransmits = json
            .pointer("/end/sum_sent/retransmits")
            .and_then(Value::as_u64);
        (rtt, retransmits)
    };
    // Rounded to the nearest bit per second, which no speed of a network needs finer.
    let bits = bits.round() as u64;
    Attempt::Measured {
        bits,
        rtt,
        retransmits,
    }
}

/// The version that iperf3 printed in its JSON, such as `3.22`.
#[must_use]
pub fn version(output: &job::Output) -> Option<String> {
    let json = first_value(&output.text)?;
    let version = json.pointer("/start/version").and_then(Value::as_str)?;
    version
        .strip_prefix("iperf ")
        .map(|version| version.trim().to_owned())
        .filter(|version| !version.is_empty())
}

/// What an error that iperf3 reported means. A message is kept without control characters, so
/// that it cannot move the terminal's cursor in the report.
fn error_kind(error: &str) -> Attempt {
    if error.starts_with("the server is busy") {
        Attempt::Busy
    } else if error.starts_with("unable to connect to server") {
        Attempt::Unreachable
    } else {
        Attempt::Error(error.chars().filter(|c| !c.is_control()).collect())
    }
}

/// The first JSON value in `text`, which may go on with what iperf3 printed on its standard
/// error.
fn first_value(text: &str) -> Option<Value> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .next()?
        .ok()
}
