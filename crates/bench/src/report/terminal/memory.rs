//! The memory group's chapters, sysbench, STREAM, latency and NUMA, each with its parameters, its
//! tables and its notes, and why each stopped short, in the catalog's words.

use bench_api::Stop;
use plugin_memory::Value;
use plugin_memory::latency;
use plugin_memory::numa;
use plugin_memory::numa::result::Kind;
use plugin_memory::numa::result::Measured;
use plugin_memory::plan;
use plugin_memory::plan::GIB;
use plugin_memory::plan::KIB;
use plugin_memory::plan::Level;
use plugin_memory::plan::MIB;
use plugin_memory::plan::STREAM_SIZES;
use plugin_memory::stream;
use plugin_memory::stream::result::KERNELS;
use plugin_memory::sysbench;
use plugin_memory::sysbench::result::Operation;

use super::Out;
use super::tool;
use crate::locale;
use crate::locale::Probe;
use crate::locale::Unit;
use crate::results::Report;

/// What a missing value shows.
const MISSING: &str = "—";

/// sysbench's chapter: its parameters, reading and writing with one thread and with all of them,
/// and its notes.
pub(super) fn sysbench(
    out: &mut Out,
    report: &Report<sysbench::Output, sysbench::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let parameters = words.memory_sysbench(
        output.version.as_deref(),
        u64::from(sysbench::measure::SECONDS),
        &size(output.single),
        output.per_thread.map(size).as_deref(),
    );
    out.field(indent, "sysbench", &parameters);
    let mut notes = cache_note(output.cache_unknown);
    let threads = columns(output.threads);
    let headers = headers(&threads);
    let rows: Vec<Vec<String>> = [Operation::Read, Operation::Write]
        .into_iter()
        .map(|operation| {
            let mut cells = vec![words.operation(operation == Operation::Write).to_owned()];
            cells.extend(threads.iter().map(|&threads| {
                let run = output
                    .runs
                    .iter()
                    .find(|run| run.operation == operation && run.threads == threads);
                run.map_or_else(
                    || MISSING.to_owned(),
                    |run| format!("{} MiB/s", run.rate.text),
                )
            }));
            cells
        })
        .collect();
    out.line(&format!("{indent}{}", words.results("sysbench")));
    out.table(indent, &headers, &rows);
    if let Some(four) = output.small {
        notes.push(words.sysbench_small(&size(four)));
    }
    for run in &output.runs {
        let name = sysbench_run(run.operation, run.threads);
        notes.extend(retried(&name, &run.retries, sysbench_fault));
    }
    for note in notes {
        out.field(indent, &words.note(), &note);
    }
}

/// Why sysbench's chapter stopped short.
pub(super) fn sysbench_failure(failure: &sysbench::Failure) -> String {
    let words = locale::words();
    match failure {
        sysbench::Failure::NoMemoryInfo => words.no_memory_info(),
        sysbench::Failure::Run {
            operation,
            threads,
            faults,
        } => failed(&sysbench_run(*operation, *threads), faults, sysbench_fault),
        sysbench::Failure::Cancelled => words.interrupted(),
    }
}

/// The name of a sysbench run, such as `single-thread read`.
fn sysbench_run(operation: Operation, threads: usize) -> String {
    locale::words().sysbench_run(operation == Operation::Write, threads > 1)
}

/// Why an attempt of sysbench gave no rate.
fn sysbench_fault(fault: &sysbench::Fault) -> String {
    match fault {
        sysbench::Fault::Tool(fault) => tool::fault("sysbench", fault, Unit::Seconds),
        sysbench::Fault::NoRate => locale::words().no_transfer_rate(),
    }
}

/// STREAM's chapter: its parameters, its four kernels with one thread and with all of them, and
/// its notes.
pub(super) fn stream(
    out: &mut Out,
    report: &Report<stream::Output, stream::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let array = plan::array_bytes(output.millions);
    let parameters = words.memory_stream(
        output.version.as_deref(),
        output.millions,
        mib(array),
        mib(3 * array),
        output.threads > 1,
    );
    out.field(indent, "STREAM", &parameters);
    let mut notes = cache_note(output.cache_unknown);
    let threads = columns(output.threads);
    let headers = headers(&threads);
    let rows: Vec<Vec<String>> = KERNELS
        .iter()
        .enumerate()
        .map(|(kernel, name)| {
            let mut cells = vec![(*name).to_owned()];
            cells.extend(threads.iter().map(|&threads| {
                let run = output.runs.iter().find(|run| run.threads == threads);
                run.map_or_else(
                    || MISSING.to_owned(),
                    |run| format!("{} MB/s", run.rates[kernel].text),
                )
            }));
            cells
        })
        .collect();
    out.line(&format!("{indent}{}", words.results("STREAM")));
    out.table(indent, &headers, &rows);
    if output.below_rule {
        notes.push(words.stream_rule());
    }
    for run in &output.runs {
        let name = words.stream_run(run.threads > 1);
        notes.extend(retried(&name, &run.retries, stream_fault));
    }
    for note in notes {
        out.field(indent, &words.note(), &note);
    }
}

/// Why STREAM's chapter stopped short.
pub(super) fn stream_failure(failure: &stream::Failure) -> String {
    let words = locale::words();
    match failure {
        stream::Failure::NoMemoryInfo => words.no_memory_info(),
        stream::Failure::Short { budget } => {
            words.stream_short(mib(3 * plan::array_bytes(STREAM_SIZES[0])), &size(*budget))
        }
        stream::Failure::Run { threads, faults } => {
            failed(&words.stream_run(*threads > 1), faults, stream_fault)
        }
        stream::Failure::Cancelled => words.interrupted(),
    }
}

/// Why an attempt of STREAM gave no rates.
fn stream_fault(fault: &stream::Fault) -> String {
    let words = locale::words();
    match fault {
        stream::Fault::Tool(fault) => tool::fault("STREAM", fault, Unit::Seconds),
        stream::Fault::Invalid(line) => words.stream_invalid(line),
        stream::Fault::NoResult => words.no_result("STREAM"),
    }
}

/// The latency chapter: what it runs, each level's buffer and latency, and its notes.
pub(super) fn latency(
    out: &mut Out,
    report: &Report<latency::Output, latency::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    out.field(indent, words.latency_label(), &words.latency_method());
    let mut notes = cache_note(output.cache_unknown);
    let headers = words.latency_headers().map(str::to_owned);
    let rows: Vec<Vec<String>> = output
        .rows
        .iter()
        .map(|row| {
            vec![
                level(row.level).to_owned(),
                size(row.buffer),
                shown(row.latency.as_ref(), "ns"),
            ]
        })
        .collect();
    out.line(&format!("{indent}{}", words.results(words.latency_label())));
    out.table(indent, &headers, &rows);
    if !output.missing.is_empty() {
        let names: Vec<&str> = output.missing.iter().map(|&each| level(each)).collect();
        notes.push(words.levels_missing(&names));
    }
    if let Some(four) = output.small {
        notes.push(words.latency_small(&size(four)));
    }
    for row in &output.rows {
        let name = words.latency_run(level(row.level));
        notes.extend(retried(&name, &row.retries, latency_fault));
    }
    for note in notes {
        out.field(indent, &words.note(), &note);
    }
}

/// Why the latency chapter stopped short.
pub(super) fn latency_failure(failure: &latency::Failure) -> String {
    let words = locale::words();
    match failure {
        latency::Failure::NoMemoryInfo => words.no_memory_info(),
        latency::Failure::Run { level: run, faults } => {
            failed(&words.latency_run(level(*run)), faults, latency_fault)
        }
        latency::Failure::Cancelled => words.interrupted(),
    }
}

/// Why an attempt of multichase gave no latency.
fn latency_fault(fault: &latency::Fault) -> String {
    match fault {
        latency::Fault::Tool(fault) => tool::fault("multichase", fault, Unit::Seconds),
        latency::Fault::NoLatency => locale::words().no_latency(),
    }
}

/// The NUMA chapter: what it runs, the bandwidth and the latency from each node's CPUs to each
/// node's memory, and its notes. A chapter that the kernel refused to bind shows only why.
pub(super) fn numa(out: &mut Out, report: &Report<numa::Output, numa::Failure>, indent: &str) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    if matches!(report.stop, Some(Stop::Skipped(numa::Failure::Refused))) {
        return;
    }
    out.field(indent, "NUMA", &words.numa_method(output.nodes));
    let mut notes = cache_note(output.cache_unknown);
    let mut headers = vec![String::new()];
    headers.extend(output.columns.iter().map(|node| words.node(*node)));
    for kind in [Kind::Bandwidth, Kind::Latency] {
        let rows: Vec<Vec<String>> = output
            .rows
            .iter()
            .zip(&output.cells)
            .map(|(row, cells)| {
                let mut line = vec![words.node(*row)];
                line.extend(cells.iter().map(|cell| {
                    let measured = match kind {
                        Kind::Bandwidth => cell.bandwidth.as_ref(),
                        Kind::Latency => cell.latency.as_ref(),
                    };
                    measured.map_or_else(
                        || MISSING.to_owned(),
                        |measured| measured.value.text.clone(),
                    )
                }));
                line
            })
            .collect();
        out.line(&format!("{indent}{}", words.numa_title(probe(kind))));
        out.table(indent, &headers, &rows);
    }
    for (row, cells) in output.rows.iter().zip(&output.cells) {
        for (column, cell) in output.columns.iter().zip(cells) {
            let runs = [
                (Kind::Bandwidth, cell.bandwidth.as_ref()),
                (Kind::Latency, cell.latency.as_ref()),
            ];
            for (kind, measured) in runs {
                let Some(Measured { retries, .. }) = measured else {
                    continue;
                };
                let name = words.numa_run(*row, *column, probe(kind));
                notes.extend(retried(&name, retries, |fault| numa_fault(fault, kind)));
            }
        }
    }
    for note in notes {
        out.field(indent, &words.note(), &note);
    }
}

/// Why the NUMA chapter stopped short.
pub(super) fn numa_failure(failure: &numa::Failure) -> String {
    let words = locale::words();
    match failure {
        numa::Failure::NoMemoryInfo => words.no_memory_info(),
        numa::Failure::OneNode => words.one_node(),
        numa::Failure::Refused => words.bind_refused(),
        numa::Failure::Run {
            row,
            column,
            kind,
            faults,
        } => failed(
            &words.numa_run(*row, *column, probe(*kind)),
            faults,
            |fault| numa_fault(fault, *kind),
        ),
        numa::Failure::Cancelled => words.interrupted(),
    }
}

/// What a NUMA run measures, as the catalog names it.
fn probe(kind: Kind) -> Probe {
    match kind {
        Kind::Bandwidth => Probe::Bandwidth,
        Kind::Latency => Probe::Latency,
    }
}

/// Why an attempt of a NUMA run gave no value.
fn numa_fault(fault: &numa::Fault, kind: Kind) -> String {
    let words = locale::words();
    let tool = match kind {
        Kind::Bandwidth => "multiload",
        Kind::Latency => "multichase",
    };
    match fault {
        numa::Fault::Tool(fault) => tool::fault(tool, fault, Unit::Seconds),
        numa::Fault::NoBandwidth => words.no_bandwidth(),
        numa::Fault::NoLatency => words.no_latency(),
        numa::Fault::Refused => words.bind_refused(),
    }
}

/// The cache note, when the last-level caches were assumed.
fn cache_note(cache_unknown: bool) -> Vec<String> {
    if cache_unknown {
        vec![locale::words().cache_unknown()]
    } else {
        Vec::new()
    }
}

/// The threads of each column of results: one, and all the usable threads when there are more.
fn columns(threads: usize) -> Vec<usize> {
    if threads > 1 {
        vec![1, threads]
    } else {
        vec![1]
    }
}

/// The header of a table of runs: the test, then a column for each number of threads.
fn headers(threads: &[usize]) -> Vec<String> {
    let words = locale::words();
    let mut headers = vec![words.test_header().to_owned()];
    headers.extend(threads.iter().map(|&threads| words.threads_column(threads)));
    headers
}

/// A level's name in the latency table.
fn level(level: Level) -> &'static str {
    match level {
        Level::L1 => "L1",
        Level::L2 => "L2",
        Level::L3 => "L3",
        Level::Memory => locale::words().memory_level(),
    }
}

/// A measured value with its unit, or `—` when it was not measured.
fn shown(value: Option<&Value>, unit: &str) -> String {
    value.map_or_else(
        || MISSING.to_owned(),
        |value| format!("{} {unit}", value.text),
    )
}

/// A note on a run that worked after failed attempts, or `None` when its first attempt did.
/// `run` names the run, such as `single-thread read`.
fn retried<F>(run: &str, faults: &[F], words: impl Fn(&F) -> String) -> Option<String> {
    let last = faults.last()?;
    Some(locale::words().retried(run, faults.len(), &words(last)))
}

/// Why a run failed: its name, the last attempt's reason, and the number of attempts.
fn failed<F>(run: &str, faults: &[F], words: impl Fn(&F) -> String) -> String {
    let last = faults.last().map_or_else(String::new, words);
    locale::words().every_attempt_failed(run, &last, faults.len())
}

/// A size such as `256 MiB`, `16 KiB`, `1 GiB` or `30.5 MiB`.
fn size(bytes: u64) -> String {
    if bytes >= GIB && bytes.is_multiple_of(GIB) {
        format!("{} GiB", bytes / GIB)
    } else if bytes >= MIB && bytes.is_multiple_of(MIB) {
        format!("{} MiB", bytes / MIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes.is_multiple_of(KIB) {
        format!("{} KiB", bytes / KIB)
    } else {
        locale::words().bytes(bytes)
    }
}

/// Mebibytes, rounded.
fn mib(bytes: u64) -> u64 {
    (bytes + MIB / 2) / MIB
}
