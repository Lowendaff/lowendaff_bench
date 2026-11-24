//! Reading what NextTrace printed with `-j`: its JSON, one document on its standard output after
//! any warning that it prints first, such as the one about running without root; or, when it
//! traced nothing, the error on its last line.

use std::net::IpAddr;

use serde::Deserialize;

use super::result::Answer;
use super::result::Hop;
use super::result::Trace;

/// NextTrace's report, of which only the hops and the reason that it stopped are read.
#[derive(Deserialize)]
struct Report {
    #[serde(rename = "Hops")]
    hops: Vec<Vec<Probe>>,
    #[serde(rename = "StopReason", default)]
    stop: Option<StopReason>,
}

/// One probe of a hop.
#[derive(Deserialize)]
struct Probe {
    #[serde(rename = "Success")]
    success: bool,
    #[serde(rename = "Address", default)]
    address: Option<Address>,
    /// The round-trip time, in nanoseconds.
    #[serde(rename = "RTT", default)]
    rtt: u64,
    #[serde(rename = "Geo", default)]
    geo: Option<Geo>,
}

/// The address that answered a probe.
#[derive(Deserialize)]
struct Address {
    #[serde(rename = "IP")]
    ip: String,
}

/// What NextTrace's service knows of an address, in English where it has it.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Geo {
    asnumber: String,
    isp: String,
    owner: String,
    country_en: String,
    prov_en: String,
    city_en: String,
}

/// Why NextTrace stopped.
#[derive(Deserialize)]
struct StopReason {
    reason: String,
}

/// The trace in what NextTrace printed: the JSON document that starts at the first line that
/// opens one. For each hop, the probes that got an answer give its addresses, in the order in
/// which they first answered; an address's network and place come from its first answer, and its
/// latency is the smallest round-trip time of its answers. `None` when there is no trace.
#[must_use]
pub fn trace(text: &str) -> Option<Trace> {
    let start = json_start(text)?;
    // What follows the document, its standard error, is not read.
    let report = serde_json::Deserializer::from_str(&text[start..])
        .into_iter::<Report>()
        .next()?
        .ok()?;
    let hops = report
        .hops
        .into_iter()
        .map(|probes| {
            let mut answers: Vec<Answer> = Vec::new();
            let answered = probes.into_iter().filter(|probe| probe.success);
            for probe in answered {
                let Some(address) = probe
                    .address
                    .and_then(|address| address.ip.parse::<IpAddr>().ok())
                else {
                    continue;
                };
                // In microseconds, to the nearest.
                let latency = probe.rtt.saturating_add(500) / 1000;
                match answers.iter_mut().find(|known| known.address == address) {
                    Some(known) => known.latency = known.latency.min(latency),
                    None => {
                        let geo = probe.geo.unwrap_or_default();
                        answers.push(Answer {
                            address,
                            asn: asn(&geo),
                            network: network(&geo),
                            exchange: None,
                            place: place(&geo),
                            latency,
                        });
                    }
                }
            }
            Hop { answers }
        })
        .collect();
    let reached = report
        .stop
        .is_some_and(|stop| stop.reason == "destination_reached");
    Some(Trace { reached, hops })
}

/// Where the first line that opens a JSON document starts.
fn json_start(text: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with('{') {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// An address's Autonomous System number, when NextTrace's service names a valid numeric ASN.
fn asn(geo: &Geo) -> Option<String> {
    let digits = clean(&geo.asnumber);
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| format!("AS{digits}"))
}

/// An address's network: its ISP, else its owner.
fn network(geo: &Geo) -> Option<String> {
    [&geo.isp, &geo.owner]
        .into_iter()
        .map(|name| clean(name))
        .find(|name| !name.is_empty())
}

/// An address's place: its country, province and city, those that the service names.
fn place(geo: &Geo) -> Option<String> {
    let parts: Vec<String> = [&geo.country_en, &geo.prov_en, &geo.city_en]
        .into_iter()
        .map(|part| clean(part))
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// A name of the service's, as the report can show it: without control characters, which could
/// move the terminal's cursor, and with each run of spaces as one.
fn clean(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// NextTrace's error when it traced nothing: its last line with text, without the time that its
/// log starts a line with, up to its first character outside printable ASCII, after which its
/// errors go on in Chinese; `None` when nothing of it is left, as of its warning, whose lines
/// start in Chinese, or when the line is of a JSON document, which names the host's own address.
#[must_use]
pub fn error(text: &str) -> Option<String> {
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?
        .trim();
    if line.starts_with('{') {
        return None;
    }
    let line = without_time(line);
    let end = line
        .find(|c: char| !(' '..='~').contains(&c))
        .unwrap_or(line.len());
    let error = line[..end].trim_end();
    (!error.is_empty()).then(|| error.to_owned())
}

/// A line without the date and time that Go's log starts it with, `2026/11/18 09:49:38 `.
fn without_time(line: &str) -> &str {
    let pattern = b"dddd/dd/dd dd:dd:dd ";
    let bytes = line.as_bytes();
    let timed = bytes.len() > pattern.len()
        && pattern.iter().zip(bytes).all(|(want, byte)| match want {
            b'd' => byte.is_ascii_digit(),
            _ => want == byte,
        });
    if timed { &line[pattern.len()..] } else { line }
}
