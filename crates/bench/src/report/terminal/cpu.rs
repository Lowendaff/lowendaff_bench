//! The CPU group's chapters: CPU performance (sysbench) and Geekbench 7, and why each stopped
//! short, in the catalog's words.

use bench_tool::job::Adjustment;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::memory::GIB;
use plugin_cpu::geekbench::result::Memory;
use plugin_cpu::geekbench::result::NoSwap;
use plugin_cpu::sysbench;
use plugin_cpu::sysbench::measure::MAX_PRIME;
use plugin_cpu::sysbench::measure::SECONDS;

use super::Out;
use super::tool;
use crate::locale;
use crate::locale::NoSwapWhy;
use crate::locale::Unit;
use crate::results::Report;

/// CPU performance: the tool and its parameters, the runs in a table, how many times the
/// single-thread rate the multi-thread one is, and a note on each run that worked only after
/// retries.
pub(super) fn sysbench(
    out: &mut Out,
    report: &Report<sysbench::Output, sysbench::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let parameters = words.sysbench_parameters(
        output.version.as_deref(),
        u64::from(MAX_PRIME),
        u64::from(SECONDS),
    );
    out.line(&format!("{indent}{parameters}"));
    let headers = words.cpu_headers().map(str::to_owned);
    let row = |name: &str, run: &sysbench::result::Run| {
        vec![
            name.to_owned(),
            run.threads.to_string(),
            run.rate.text.clone(),
        ]
    };
    let mut rows = vec![row(words.cpu_row(false), &output.single)];
    let mut runs = vec![(words.cpu_test(false), &output.single)];
    if let Some(multi) = &output.multi {
        rows.push(row(words.cpu_row(true), multi));
        runs.push((words.cpu_test(true), multi));
    }
    out.table(indent, &headers, &rows);
    if let Some(multi) = &output.multi {
        let ratio = multi.rate.value / output.single.rate.value;
        out.field(indent, words.ratio_label(), &words.ratio(ratio));
    }
    for (name, run) in runs {
        let Some(last) = run.retries.last() else {
            continue;
        };
        let note = words.retried(name, run.retries.len(), &sysbench_fault(last));
        out.field(indent, &words.note(), &note);
    }
}

/// Why CPU performance stopped short: the run whose every attempt failed, its last reason and
/// the number of attempts.
pub(super) fn sysbench_failure(failure: &sysbench::Failure) -> String {
    let words = locale::words();
    match failure {
        sysbench::Failure::Single(faults) => attempts(faults),
        sysbench::Failure::Multi(faults) => {
            let last = faults.last().map(sysbench_fault).unwrap_or_default();
            words.every_attempt_failed(words.cpu_test(true), &last, faults.len())
        }
        sysbench::Failure::Cancelled => words.interrupted(),
    }
}

/// The last attempt's reason, and how many attempts there were.
fn attempts(faults: &[sysbench::Fault]) -> String {
    let last = faults.last().map(sysbench_fault).unwrap_or_default();
    locale::words().attempts(&last, faults.len())
}

/// Why an attempt of sysbench gave no rate.
fn sysbench_fault(fault: &sysbench::Fault) -> String {
    match fault {
        sysbench::Fault::Tool(fault) => tool::fault("sysbench", fault, Unit::Seconds),
        sysbench::Fault::NoRate => locale::words().no_rate(),
    }
}

/// Geekbench 7: the version that ran, the scores and the result link, then a note on memory, on
/// the kernel's first choice, on runs again after failed uploads, and on swap left on.
pub(super) fn geekbench(
    out: &mut Out,
    report: &Report<geekbench::Output, geekbench::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    if let Some(scores) = &output.scores {
        if let Some(version) = &scores.version {
            out.line(&format!("{indent}Geekbench {version}"));
        }
        let [single, multi, link] = words.geekbench_labels();
        out.field(indent, single, &scores.single.to_string());
        out.field(indent, multi, &scores.multi.to_string());
        let url = scores.url.clone().unwrap_or_else(|| words.unavailable());
        out.field(indent, link, &url);
    }
    let note = words.note();
    if let Some(memory) = &output.memory {
        out.field(indent, &note, &memory_note(memory));
    }
    if let Some(first_to_go) = output.first_to_go.as_ref().and_then(first_to_go_note) {
        out.field(indent, &note, &first_to_go);
    }
    if output.scores.is_some()
        && let Some(last) = output.uploads.last()
    {
        let uploads = words.uploads_retried(output.uploads.len(), last);
        out.field(indent, &note, &uploads);
    }
    if let Some(left) = &output.swap_left {
        out.field(indent, &note, &words.swap_left(&left.error, &left.path));
    }
}

/// Why Geekbench 7 has no scores.
pub(super) fn geekbench_failure(failure: &geekbench::Failure) -> String {
    let words = locale::words();
    match failure {
        geekbench::Failure::Upload(reason) => words.upload_failed(reason),
        geekbench::Failure::Run(geekbench::Fault::Tool(fault)) => {
            tool::fault("Geekbench", fault, Unit::Minutes)
        }
        geekbench::Failure::Run(geekbench::Fault::NoScores) => words.no_scores(),
        geekbench::Failure::OutOfMemory => words.out_of_memory(),
        geekbench::Failure::Cancelled => words.interrupted(),
    }
}

/// What was done about memory, when it may be short.
fn memory_note(memory: &Memory) -> String {
    let words = locale::words();
    match memory {
        Memory::Swapped {
            need,
            available,
            gib,
        } => words.swapped(need / GIB, &tenths(*available), *gib),
        Memory::Short {
            need,
            available,
            why,
        } => {
            let free = match why {
                NoSwap::NoRoom(Some(free)) => Some(tenths(*free)),
                _ => None,
            };
            let why = match why {
                NoSwap::Unused => NoSwapWhy::Unused,
                NoSwap::NoRoom(_) => NoSwapWhy::NoRoom(free.as_deref()),
                NoSwap::Denied => NoSwapWhy::Denied,
                NoSwap::Unsupported => NoSwapWhy::Unsupported,
                NoSwap::Error(error) => NoSwapWhy::Error(error),
            };
            words.memory_short(need / GIB, &tenths(*available), why)
        }
    }
}

/// A note when Geekbench could not be made the kernel's first choice, or when the program kept
/// that choice for itself.
fn first_to_go_note(adjustment: &Adjustment) -> Option<String> {
    let words = locale::words();
    match adjustment {
        Adjustment::Done => None,
        Adjustment::NotSet(error) => Some(words.first_to_go_not_set(error)),
        Adjustment::NotRestored(error) => Some(words.oom_not_restored(error)),
    }
}

/// Bytes in GiB with one decimal, rounded down, so that nothing looks larger than it is.
fn tenths(bytes: u64) -> String {
    let tenths = bytes * 10 / GIB;
    format!("{}.{}", tenths / 10, tenths % 10)
}
