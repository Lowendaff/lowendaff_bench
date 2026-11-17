//! The network group's chapter: network speed (iperf3), a table for each family, with a note on
//! each direction that took several tries or failed, and why the item stopped short, in the
//! catalog's words.

use bench_tool::job;
use plugin_network::iperf3;
use plugin_network::iperf3::result::Tested;
use plugin_network::iperf3::result::Way;
use plugin_network::iperf3::servers;
use plugin_network::iperf3::servers::ListError;
use plugin_network::iperf3::servers::Rule;

use super::Out;
use super::tool;
use crate::locale;
use crate::locale::ListRule;
use crate::locale::Unit;
use crate::locale::WayCell;
use crate::results::Report;

/// What a missing value shows.
const MISSING: &str = "—";

/// Network speed: the method, a note on what was asked for and is not in the list, then a table
/// for each family with its notes.
pub(super) fn iperf3(
    out: &mut Out,
    report: &Report<iperf3::Output, iperf3::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let settings = output.settings;
    out.line(&format!(
        "{indent}{}",
        words.iperf3_method(
            output.version.as_deref(),
            settings.streams,
            settings.seconds,
            settings.rate
        )
    ));
    if !output.unknown.is_empty() {
        out.field(
            indent,
            &words.note(),
            &words.unknown_servers(&output.unknown),
        );
    }
    for (ipv6, tested) in [(false, &output.ipv4), (true, &output.ipv6)] {
        if tested.is_empty() {
            continue;
        }
        out.line(&format!("{indent}{}", words.family_title(ipv6)));
        let headers = words.network_headers().map(str::to_owned);
        let rows: Vec<Vec<String>> = tested.iter().map(row).collect();
        out.table(indent, &headers, &rows);
        for server in tested {
            for note in notes(server) {
                out.field(indent, &words.note(), &note);
            }
        }
    }
}

/// A server's row: its place, its provider, its speed each way, and its latency.
fn row(tested: &Tested) -> Vec<String> {
    vec![
        locale::words().place(&tested.location, &tested.country),
        tested.provider.clone(),
        cell(&tested.upload),
        cell(&tested.download),
        tested.latency().map_or_else(|| MISSING.to_owned(), latency),
    ]
}

/// A direction's cell: its speed, or what kept it from one.
fn cell(way: &Way) -> String {
    let words = locale::words();
    let kind = match way {
        Way::Measured(speed) => return bits(speed.bits),
        Way::Busy => WayCell::Busy,
        Way::Unreachable => WayCell::Unreachable,
        Way::Fault(job::Fault::TimedOut(_)) => WayCell::TimedOut,
        Way::Error(_) | Way::Fault(_) | Way::Nothing => WayCell::Error,
        Way::Skipped => WayCell::NotTried,
    };
    words.way_cell(kind).to_owned()
}

/// The notes on a server: each direction that took several tries, or that failed for a reason
/// that its cell does not tell.
fn notes(tested: &Tested) -> Vec<String> {
    let words = locale::words();
    let place = words.place(&tested.location, &tested.country);
    let mut notes = Vec::new();
    for (download, way) in [(false, &tested.upload), (true, &tested.download)] {
        let reason = match way {
            Way::Measured(speed) if speed.tries > 1 => {
                notes.push(words.tries(&place, download, speed.tries));
                continue;
            }
            Way::Error(error) => error.clone(),
            Way::Fault(job::Fault::TimedOut(_)) => continue,
            Way::Fault(fault) => tool::fault("iperf3", fault, Unit::Seconds),
            Way::Nothing => words.nothing_measured(),
            Way::Measured(_) | Way::Busy | Way::Unreachable | Way::Skipped => continue,
        };
        notes.push(words.way_failed(&place, download, &reason));
    }
    notes
}

/// Bits per second as iperf3 prints them: from 1 Gbit/s on, in Gbit/s, else in Mbit/s, with
/// three digits or more.
#[must_use]
pub fn bits(bits: u64) -> String {
    let (value, unit) = if bits >= 1_000_000_000 {
        (bits as f64 / 1e9, "Gbit/s")
    } else {
        (bits as f64 / 1e6, "Mbit/s")
    };
    format!("{} {unit}", digits(value))
}

/// A round-trip time in microseconds, in milliseconds with three digits or more.
#[must_use]
pub fn latency(micros: u64) -> String {
    format!("{} ms", digits(micros as f64 / 1000.0))
}

/// A value with three digits or more: none after the point from 100, one from 10, two below.
fn digits(value: f64) -> String {
    if value >= 99.95 {
        format!("{value:.0}")
    } else if value >= 9.995 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

/// Why network speed stopped short.
pub(super) fn iperf3_failure(failure: &iperf3::Failure) -> String {
    let words = locale::words();
    match failure {
        iperf3::Failure::Unreadable(reason) => words.list_unreadable(reason),
        iperf3::Failure::List(error) => list_error(error),
        iperf3::Failure::NoServer(unknown) => words.no_server(unknown),
        iperf3::Failure::NoFamily => words.no_family(),
        iperf3::Failure::NoResult => words.no_speed(),
        iperf3::Failure::Cancelled => words.interrupted(),
    }
}

/// Why a server list cannot be used.
fn list_error(error: &ListError) -> String {
    let words = locale::words();
    match error {
        ListError::TooLarge => words.list_too_large(servers::LIMIT),
        ListError::Format(reason) => words.list_format(reason),
        ListError::Newer(version) => words.list_newer(*version),
        ListError::Version(version) => words.list_version(*version),
        ListError::TooMany(count) => words.list_too_many(*count, servers::MOST_SERVERS),
        ListError::Server(position, rule) => {
            let rule = match rule {
                Rule::Id => ListRule::Id,
                Rule::Country => ListRule::Country,
                Rule::Host => ListRule::Host,
                Rule::Ports => ListRule::Ports,
                Rule::Families => ListRule::Families,
                Rule::Text => ListRule::Text,
            };
            words.list_rule(*position, rule)
        }
        ListError::Duplicate(id) => words.list_duplicate(id),
    }
}
