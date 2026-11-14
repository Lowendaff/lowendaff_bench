//! The disk group's chapters: disk health (smartctl) and disk performance (fio, in YABS's
//! formats), and why each stopped short, in the catalog's words.

use bench_tool::job;
use plugin_disk::fio;
use plugin_disk::fio::measure::BLOCK_SIZES;
use plugin_disk::fio::result::Outcome;
use plugin_disk::fio::result::Rates;
use plugin_disk::health;
use plugin_disk::health::location::Location;
use plugin_disk::health::result::BadBlocks;
use plugin_disk::health::result::Disk;
use plugin_disk::health::result::Health;

use super::Out;
use super::tool;
use crate::locale;
use crate::locale::FioRow;
use crate::locale::Unit;
use crate::locale::Verdict;
use crate::results::Report;

/// What a missing value shows.
const MISSING: &str = "—";

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// Disk health: the tool, then the disks in a table, a note on each disk that needs one, and a
/// note on each device node that smartctl created and that could not be removed.
pub(super) fn health(
    out: &mut Out,
    report: &Report<health::Output, health::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    if !output.disks.is_empty() {
        let version = output
            .version
            .as_deref()
            .map_or_else(String::new, |version| format!(" {version}"));
        out.line(&format!("{indent}smartctl{version}"));
        let headers = words.health_headers().map(str::to_owned);
        let with_host = several_cards(&output.disks);
        let rows: Vec<Vec<String>> = output
            .disks
            .iter()
            .map(|disk| cells(disk, with_host))
            .collect();
        out.table(indent, &headers, &rows);
        for disk in &output.disks {
            let name = location(&disk.location, with_host);
            if let Some(note) = note(&name, &disk.health) {
                out.field(indent, &words.note(), &note);
            }
        }
    }
    for left in &output.nodes_left {
        let note = words.node_left(&left.path, &left.error);
        out.field(indent, &words.note(), &note);
    }
}

/// Why disk health stopped short.
pub(super) fn health_failure(failure: &health::Failure) -> String {
    let words = locale::words();
    match failure {
        health::Failure::NoDisks => words.no_disks(),
        health::Failure::Scan(health::Fault::Tool(job::Fault::TimedOut(limit))) => {
            words.scan_timed_out(limit.as_secs())
        }
        health::Failure::Scan(fault) => words.scan_failed(&smartctl(fault)),
        health::Failure::AllFailed => words.all_failed(),
        health::Failure::Cancelled => words.interrupted(),
    }
}

/// The device cell: a kernel name, or a disk on a RAID card, which names its card when the disks
/// sit on more than one.
fn location(location: &Location, with_host: bool) -> String {
    match location {
        Location::Named(name) => name.clone(),
        Location::Card { host, numbers } => {
            let numbers: Vec<String> = numbers.iter().map(u32::to_string).collect();
            let host = with_host.then_some(*host);
            locale::words().card_disk(&numbers.join(":"), host)
        }
    }
}

/// Whether the RAID-card disks sit on more than one card.
fn several_cards(disks: &[Disk]) -> bool {
    let mut hosts = disks.iter().filter_map(|disk| match disk.location {
        Location::Card { host, .. } => Some(host),
        Location::Named(_) => None,
    });
    hosts
        .next()
        .is_some_and(|first| hosts.any(|host| host != first))
}

/// One disk's row.
fn cells(disk: &Disk, with_host: bool) -> Vec<String> {
    let words = locale::words();
    let values = &disk.values;
    vec![
        location(&disk.location, with_host),
        verdict(&disk.health).to_owned(),
        shown(values.hours, |hours| words.hours(hours)),
        shown(values.temperature, |degrees| format!("{degrees} °C")),
        shown(values.endurance_used, |percent| format!("{percent}%")),
        shown(values.written, written),
        bad_blocks(&values.bad_blocks),
        shown(values.interface_errors, |count| count.to_string()),
    ]
}

/// The health cell.
fn verdict(health: &Health) -> &'static str {
    let verdict = match health {
        Health::Passed => Verdict::Passed,
        Health::Failed => Verdict::Failed,
        Health::Unknown { .. } => Verdict::Unknown,
        Health::Unsupported => Verdict::Unsupported,
        Health::ReadFailed(_) => Verdict::ReadFailed,
    };
    locale::words().verdict(verdict)
}

/// The note on the disk named `disk`, for a disk that needs one.
fn note(disk: &str, health: &Health) -> Option<String> {
    let words = locale::words();
    match health {
        Health::ReadFailed(fault) => Some(words.disk_read_failed(disk, &read_failure(fault))),
        Health::Unsupported => Some(words.no_smart(disk)),
        Health::Unknown { smart_off: true } => Some(words.smart_off(disk)),
        Health::Passed | Health::Failed | Health::Unknown { smart_off: false } => None,
    }
}

/// Why a disk could not be read.
fn read_failure(fault: &health::Fault) -> String {
    match fault {
        health::Fault::Tool(job::Fault::TimedOut(limit)) => {
            locale::words().disk_read_timed_out(limit.as_secs())
        }
        fault => smartctl(fault),
    }
}

/// Why smartctl gave nothing to read.
fn smartctl(fault: &health::Fault) -> String {
    let words = locale::words();
    match fault {
        health::Fault::Tool(fault) => tool::fault("smartctl", fault, Unit::Seconds),
        health::Fault::Unparsable => words.unparsable(),
        health::Fault::Message(message) => message.clone(),
        health::Fault::Unopened => words.unopened(),
        health::Fault::Refused => words.arguments_refused(),
    }
}

/// The bad blocks cell: SATA's reallocated and pending sectors, or one count.
fn bad_blocks(bad_blocks: &BadBlocks) -> String {
    match bad_blocks {
        BadBlocks::Sectors {
            reallocated: None,
            pending: None,
        }
        | BadBlocks::Count(None) => MISSING.to_owned(),
        BadBlocks::Sectors {
            reallocated,
            pending,
        } => format!(
            "{} / {}",
            shown(*reallocated, |count| count.to_string()),
            shown(*pending, |count| count.to_string())
        ),
        BadBlocks::Count(Some(count)) => count.to_string(),
    }
}

/// Bytes in decimal units with one decimal: GB below 1000 GB, TB below 1000 TB, then PB.
/// Extra digits are cut, not rounded, so 999,999,999,999 bytes show as `999.9 GB`.
fn written(bytes: u64) -> String {
    let (unit, tenth) = if bytes < 1_000_000_000_000 {
        ("GB", 100_000_000)
    } else if bytes < 1_000_000_000_000_000 {
        ("TB", 100_000_000_000)
    } else {
        ("PB", 100_000_000_000_000)
    };
    let tenths = bytes / tenth;
    format!("{}.{} {unit}", tenths / 10, tenths % 10)
}

/// A value, or `—` when it is missing.
fn shown<T>(value: Option<T>, format: impl FnOnce(T) -> String) -> String {
    value.map_or_else(|| MISSING.to_owned(), format)
}

/// Disk performance: the tool, the method and the file's size, YABS's note on ZFS, then a table
/// for each two block sizes in YABS's layout, each followed by a note on each test without a
/// result.
pub(super) fn fio(out: &mut Out, report: &Report<fio::Output, fio::Failure>, indent: &str) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let version = output
        .version
        .as_deref()
        .map_or_else(String::new, |version| format!(" {version}"));
    out.line(&format!("{indent}fio{version}"));
    let (method_label, method) = words.fio_method(&file_size(output.size));
    out.field(indent, method_label, &method);
    if let Some(needed) = output.zfs_needed {
        out.field(indent, &words.note(), &words.zfs_note(needed));
    }
    for pair in BLOCK_SIZES {
        let outcomes = pair.map(|block_size| {
            output
                .tests
                .iter()
                .find(|test| test.block_size == block_size)
                .map(|test| &test.outcome)
        });
        if outcomes.iter().all(Option::is_none) {
            continue;
        }
        let [first, second] = pair;
        let title = words.block_sizes(&label(first), &label(second));
        out.line(&format!("{indent}{title}"));
        let headers = [
            words.block_size_header().to_owned(),
            label(first),
            label(second),
        ];
        let mut rows = Vec::new();
        for (row, pick) in [
            (FioRow::Read, read as fn(&Rates) -> (u64, u64)),
            (FioRow::Write, write),
            (FioRow::Total, total),
        ] {
            let [first, second] = outcomes.map(|outcome| cell(outcome, pick));
            rows.push(vec![words.fio_row(row).to_owned(), first, second]);
        }
        out.table(indent, &headers, &rows);
        for (block_size, outcome) in pair.into_iter().zip(outcomes) {
            let Some(Outcome::Failed(fault)) = outcome else {
                continue;
            };
            let note = match fault {
                fio::Fault::Tool(job::Fault::TimedOut(limit)) => {
                    words.block_timed_out(&label(block_size), limit.as_secs())
                }
                fault => words.block_failed(&label(block_size), &fio_fault(fault)),
            };
            out.field(indent, &words.note(), &note);
        }
    }
}

/// Why disk performance stopped short.
pub(super) fn fio_failure(failure: &fio::Failure) -> String {
    let words = locale::words();
    match failure {
        fio::Failure::NoSpaceInfo => words.no_space_info(),
        fio::Failure::InMemory => words.in_memory(),
        fio::Failure::Space { needed, available } => {
            words.no_space(&file_size(*needed), &free_space(*available))
        }
        fio::Failure::NoDirectIo => words.no_direct_io(),
        fio::Failure::Setup(fault) => words.setup_failed(&fio_fault(fault)),
        fio::Failure::NoResult => words.no_block_size(),
        fio::Failure::Cancelled => words.interrupted(),
    }
}

/// Why a run of fio gave no rates: fio's own error line when it ended badly.
fn fio_fault(fault: &fio::Fault) -> String {
    let words = locale::words();
    match fault {
        fio::Fault::Tool(job::Fault::Exit { last, .. } | job::Fault::Signal { last }) => {
            last.clone().unwrap_or_else(|| words.no_output("fio"))
        }
        fio::Fault::Tool(fault) => tool::fault("fio", fault, Unit::Seconds),
        fio::Fault::NoResult => words.no_result("fio"),
        fio::Fault::Error(error) => words.fio_reported(&error.to_string()),
        fio::Fault::Field(field) => words.fio_field(*field),
    }
}

/// The test file's size, such as `2 GiB` or `512 MiB`.
fn file_size(bytes: u64) -> String {
    if bytes.is_multiple_of(GIB) {
        format!("{} GiB", bytes / GIB)
    } else {
        format!("{} MiB", bytes / MIB)
    }
}

/// A speed as YABS's `format_speed` prints it, from KiB/s: decimal KB/s, MB/s or GB/s,
/// truncated to two decimals.
#[must_use]
pub fn speed(kib_per_second: u64) -> String {
    let bytes = kib_per_second as f64 * 1024.0;
    let (value, unit) = if kib_per_second >= 976_563 {
        (bytes / 1e9, "GB/s")
    } else if kib_per_second >= 977 {
        (bytes / 1e6, "MB/s")
    } else {
        (bytes / 1e3, "KB/s")
    };
    format!("{} {unit}", truncate(&awk_number(value), 2))
}

/// IOPS as YABS's `format_iops` prints them: from 1000 on, thousands truncated to one decimal,
/// followed by `k`.
#[must_use]
pub fn iops(iops: u64) -> String {
    if iops < 1000 {
        return iops.to_string();
    }
    format!("{}k", truncate(&awk_number(iops as f64 / 1000.0), 1))
}

fn read(rates: &Rates) -> (u64, u64) {
    (rates.read_kib, rates.read_iops)
}

fn write(rates: &Rates) -> (u64, u64) {
    (rates.write_kib, rates.write_iops)
}

/// Read plus write, as YABS adds the raw values before formatting them.
fn total(rates: &Rates) -> (u64, u64) {
    (
        rates.read_kib.saturating_add(rates.write_kib),
        rates.read_iops.saturating_add(rates.write_iops),
    )
}

/// A test's cell: the rate with its IOPS, or what kept it from one; `—` for a test that never
/// ended, as after an interrupt.
fn cell(outcome: Option<&Outcome>, pick: fn(&Rates) -> (u64, u64)) -> String {
    let words = locale::words();
    match outcome {
        Some(Outcome::Done(rates)) => {
            let (kib, operations) = pick(rates);
            words.rate(&speed(kib), &iops(operations))
        }
        Some(Outcome::Failed(fio::Fault::Tool(job::Fault::TimedOut(_)))) => {
            words.fio_timed_out().to_owned()
        }
        Some(Outcome::Failed(_)) => words.fio_error().to_owned(),
        None => MISSING.to_owned(),
    }
}

/// A block size as the tables show it, such as `4K`.
fn label(block_size: &str) -> String {
    block_size.to_uppercase()
}

/// A number as awk's `print` writes it with OFMT `%.6g`: six significant digits without
/// trailing zeros. That holds from 1 to below 10^6, and for 0; every real rate falls in that
/// range in the unit it is printed in. Beyond it (10^6 GB/s, or 10^9 IOPS) awk switches
/// notation, and the plain number printed here no longer matches YABS.
fn awk_number(value: f64) -> String {
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("scientific notation has an exponent");
    let exponent: i32 = exponent.parse().expect("the exponent is a decimal integer");
    let point = match usize::try_from(exponent) {
        Ok(point) if point < 6 => point,
        _ => return value.to_string(),
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let fixed = format!("{}.{}", &digits[..=point], &digits[point + 1..]);
    fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// What YABS's `awk -F. '{ printf "%0.Nf", $1"."substr($2,1,N) }'` makes of a number: the
/// fraction cut to `decimals` digits, then padded with zeros.
fn truncate(number: &str, decimals: usize) -> String {
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let kept: String = fraction.chars().take(decimals).collect();
    format!("{whole}.{kept:0<decimals$}")
}

/// Free space as the skip reason prints it, truncated so that it never looks like enough.
fn free_space(bytes: u64) -> String {
    let (unit, name) = if bytes >= GIB {
        (GIB, "GiB")
    } else {
        (MIB, "MiB")
    };
    let tenths = u128::from(bytes) * 10 / u128::from(unit);
    format!("{}.{} {name}", tenths / 10, tenths % 10)
}
