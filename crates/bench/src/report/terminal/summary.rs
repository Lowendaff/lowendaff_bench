//! The 汇总 section, and the words for how each plugin ended.

use bench_api::Net;
use bench_api::typed::Stop;

use super::LABEL_WIDTH;
use super::Out;
use super::Page;
use super::cpu;
use super::disk;
use super::memory;
use super::pad;
use crate::orchestrator::Fault;
use crate::orchestrator::Run;
use crate::orchestrator::Status;
use crate::orchestrator::Unmet;
use crate::results::Measured;

/// Each plugin's title and how it ended, the items made for other platforms, and the count of
/// each status with the run's total time.
pub(super) fn section(out: &mut Out, page: &Page) {
    out.line("\n【汇总】");
    for run in page.runs {
        let label = pad(run.title.unwrap_or(run.id), LABEL_WIDTH);
        out.line(&format!("  {label}  {}", run_status(run)));
    }
    if !page.not_applicable.is_empty() {
        out.line(&format!("  本机不适用：{}", page.not_applicable.join("、")));
    }
    let count =
        |pick: fn(&Status) -> bool| page.runs.iter().filter(|run| pick(&run.status)).count();
    let completed = count(|status| matches!(status, Status::Completed));
    let skipped = count(|status| matches!(status, Status::Skipped(_)));
    let failed = count(|status| matches!(status, Status::Failed(_)));
    let timed_out = count(|status| matches!(status, Status::TimedOut));
    let cancelled = count(|status| matches!(status, Status::Cancelled));
    out.line(&format!(
        "\n共 {} 项：完成 {completed}，跳过 {skipped}，失败 {failed}，超时 {timed_out}，\
         中断 {cancelled}。总用时 {:.1} 秒",
        page.runs.len(),
        page.total.as_secs_f64()
    ));
}

/// The plugin's own reason, when it stopped short.
fn reason(run: &Run<Measured>) -> Option<String> {
    match &run.result {
        Some(Measured::Legacy(report)) => report.stop.as_ref().map(|stop| failure(stop).clone()),
        Some(Measured::CpuSysbench(report)) => report
            .stop
            .as_ref()
            .map(|stop| cpu::sysbench_failure(failure(stop))),
        Some(Measured::CpuGeekbench(report)) => report
            .stop
            .as_ref()
            .map(|stop| cpu::geekbench_failure(failure(stop))),
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
        None => None,
    }
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
    status_text(run, reason(run).as_deref())
}

/// How a plugin ended; `reason` is the plugin's own when it stopped short.
fn status_text<R>(run: &Run<R>, reason: Option<&str>) -> String {
    let reason = reason.unwrap_or_default();
    match &run.status {
        // A plugin that completed has run, so its time is known.
        Status::Completed => format!(
            "完成，用时 {:.1} 秒",
            run.elapsed.unwrap_or_default().as_secs_f64()
        ),
        Status::Skipped(Some(unmet)) => format!("跳过：{}", unmet_text(unmet)),
        Status::Skipped(None) => format!("跳过：{reason}"),
        Status::Failed(Some(fault)) => format!("失败：{}", fault_text(fault)),
        Status::Failed(None) => format!("失败：{reason}"),
        Status::TimedOut => "超时，已保留超时前的结果".to_owned(),
        Status::Cancelled => "已中断".to_owned(),
    }
}

/// A requirement that the host does not meet.
#[must_use]
pub fn unmet_text(unmet: &Unmet) -> String {
    match unmet {
        Unmet::Glibc {
            needed,
            found: Some(found),
        } => format!("需要 glibc {needed} 或更新，本机是 {found}"),
        Unmet::Glibc {
            needed,
            found: None,
        } => format!("需要 glibc {needed} 或更新，本机没有 glibc"),
        Unmet::Network(Net::Ipv4) => "需要 IPv4 网络连接".to_owned(),
        Unmet::Network(Net::Ipv6) => "需要 IPv6 网络连接".to_owned(),
        Unmet::Network(Net::Any | Net::None) => "需要网络连接".to_owned(),
        Unmet::Tool { name, why } if why.is_empty() => format!("工具 {name} 不可用"),
        Unmet::Tool { name, why } => format!("工具 {name} 不可用：{why}"),
    }
}

/// A fault around a plugin.
#[must_use]
pub fn fault_text(fault: &Fault) -> String {
    match fault {
        Fault::Panic(message) => format!("插件崩溃：{message}"),
        Fault::WorkDir(error) => format!("无法创建工作目录：{error}"),
        Fault::Aborted(error) => format!("任务异常结束：{error}"),
    }
}
