//! Summary helpers and the words for how each plugin ended.

use bench_api::Stop;

use super::Page;
use super::cpu;
use super::disk;
use super::ip;
use super::memory;
use super::network;
use crate::locale;
use crate::locale::Totals;
use crate::orchestrator::Fault;
use crate::orchestrator::Run;
use crate::orchestrator::Status;
use crate::orchestrator::Unmet;
use crate::results::Measured;

/// Counts of each status and total elapsed time for `page`.
pub(in crate::report) fn totals(page: &Page) -> Totals {
    let count =
        |pick: fn(&Status) -> bool| page.runs.iter().filter(|run| pick(&run.status)).count();
    Totals {
        items: page.runs.len(),
        completed: count(|status| matches!(status, Status::Completed)),
        skipped: count(|status| matches!(status, Status::Skipped(_))),
        failed: count(|status| matches!(status, Status::Failed(_))),
        timed_out: count(|status| matches!(status, Status::TimedOut)),
        interrupted: count(|status| matches!(status, Status::Cancelled)),
        seconds: page.total.as_secs_f64(),
    }
}

/// The plugin's own reason, when it stopped short.
pub(in crate::report) fn stop_reason(run: &Run<Measured>) -> Option<String> {
    let raw = match &run.result {
        Some(Measured::CpuBench(report)) => report
            .stop
            .as_ref()
            .map(|stop| cpu::bench_failure(report.output.as_ref(), failure(stop))),
        Some(Measured::MemorySysbench(report)) => report
            .stop
            .as_ref()
            .map(|stop| memory::sysbench_failure(failure(stop))),
        Some(Measured::MemoryStream(report)) => report
            .stop
            .as_ref()
            .map(|stop| memory::stream_failure(failure(stop))),
        Some(Measured::MemoryLatency(report)) => report
            .stop
            .as_ref()
            .map(|stop| memory::latency_failure(failure(stop))),
        Some(Measured::MemoryNuma(report)) => report
            .stop
            .as_ref()
            .map(|stop| memory::numa_failure(failure(stop))),
        Some(Measured::DiskHealth(report)) => report
            .stop
            .as_ref()
            .map(|stop| disk::health_failure(failure(stop))),
        Some(Measured::DiskFio(report)) => report
            .stop
            .as_ref()
            .map(|stop| disk::fio_failure(failure(stop))),
        Some(Measured::NetworkIperf3(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::iperf3_failure(failure(stop))),
        Some(Measured::NetworkTaier(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::taier_failure(failure(stop))),
        Some(Measured::NetworkCloudflare(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::cloudflare_failure(failure(stop))),
        Some(Measured::NetworkInetspeed(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::inetspeed_failure(failure(stop))),
        Some(Measured::NetworkTcping(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::tcping_failure(failure(stop))),
        Some(Measured::NetworkTrace(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::trace_failure(failure(stop))),
        Some(Measured::NetworkBgp(report)) => report
            .stop
            .as_ref()
            .map(|stop| network::bgp_failure(failure(stop))),
        Some(Measured::IpQuality(report)) => report
            .stop
            .as_ref()
            .map(|stop| ip::quality_failure(failure(stop))),
        Some(Measured::IpMedia(report)) => report
            .stop
            .as_ref()
            .map(|stop| ip::media_failure(failure(stop))),
        None => None,
    };
    raw.map(|text| super::strip_terminal_controls(&text, false))
}

/// The plugin's own reason in a stop, whether it skipped or failed.
fn failure<F>(stop: &Stop<F>) -> &F {
    match stop {
        Stop::Skipped(failure) | Stop::Failed(failure) => failure,
    }
}

/// How a plugin ended, with its own reason when it stopped short.
#[must_use]
pub fn run_status(run: &Run<Measured>) -> String {
    let text = status_text(run, stop_reason(run).as_deref());
    super::strip_terminal_controls(&text, false)
}

/// How a plugin ended; `reason` is the plugin's own when it stopped short.
fn status_text<R>(run: &Run<R>, reason: Option<&str>) -> String {
    let words = locale::words();
    let reason = reason.unwrap_or_default();
    match &run.status {
        // A plugin that completed has run, so its time is known.
        Status::Completed => words.completed(run.elapsed.unwrap_or_default().as_secs_f64()),
        Status::Skipped(Some(unmet)) => words.skipped(&unmet_text(unmet)),
        Status::Skipped(None) => words.skipped(reason),
        Status::Failed(Some(fault)) => words.failed(&fault_text(fault)),
        Status::Failed(None) => words.failed(reason),
        Status::TimedOut => words.timed_out(),
        Status::Cancelled => words.interrupted(),
    }
}

/// A requirement that the host does not meet.
#[must_use]
pub fn unmet_text(unmet: &Unmet) -> String {
    let text = locale::words().unmet(unmet);
    super::strip_terminal_controls(&text, false)
}

/// A fault around a plugin.
#[must_use]
pub fn fault_text(fault: &Fault) -> String {
    let text = locale::words().fault(fault);
    super::strip_terminal_controls(&text, false)
}
