//! The CPU group's words: CPU 性能 (sysbench) and why it stopped short.

use std::time::Duration;

use bench_tool::job;
use plugin_cpu::sysbench;
use plugin_cpu::sysbench::measure::MAX_PRIME;
use plugin_cpu::sysbench::measure::SECONDS;

use super::Out;
use crate::results::Report;

/// CPU 性能: the tool and its parameters, the runs in a table, how many times the single-thread
/// rate the multi-thread one is, and a note on each run that worked only after retries.
pub(super) fn sysbench(
    out: &mut Out,
    report: &Report<sysbench::Output, sysbench::Failure>,
    indent: &str,
) {
    let Some(output) = &report.output else {
        return;
    };
    let version = output
        .version
        .as_deref()
        .map_or_else(String::new, |version| format!(" {version}"));
    out.line(&format!(
        "{indent}sysbench{version}：素数上限 {MAX_PRIME}，每项 {SECONDS} 秒"
    ));
    let headers = ["项目", "线程数", "每秒事件数"].map(str::to_owned);
    let row = |name: &str, run: &sysbench::result::Run| {
        vec![
            name.to_owned(),
            run.threads.to_string(),
            run.rate.text.clone(),
        ]
    };
    let mut rows = vec![row("单线程", &output.single)];
    let mut runs = vec![("单线程测试", &output.single)];
    if let Some(multi) = &output.multi {
        rows.push(row("多线程", multi));
        runs.push(("多线程测试", multi));
    }
    out.table(indent, &headers, &rows);
    if let Some(multi) = &output.multi {
        let ratio = multi.rate.value / output.single.rate.value;
        out.field(indent, "倍数", &format!("多线程是单线程的 {ratio:.2} 倍"));
    }
    for (name, run) in runs {
        let Some(last) = run.retries.last() else {
            continue;
        };
        let note = format!(
            "{name}失败 {} 次后成功，最后一次失败的原因：{}",
            run.retries.len(),
            sysbench_fault(last)
        );
        out.field(indent, "说明", &note);
    }
}

/// Why CPU 性能 stopped short: the run whose every attempt failed, its last reason and the
/// number of attempts.
pub(super) fn sysbench_failure(failure: &sysbench::Failure) -> String {
    match failure {
        sysbench::Failure::Single(faults) => attempts(faults),
        sysbench::Failure::Multi(faults) => format!("多线程测试失败：{}", attempts(faults)),
        sysbench::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// The last attempt's reason, and how many attempts there were.
fn attempts(faults: &[sysbench::Fault]) -> String {
    let last = faults.last().map(sysbench_fault).unwrap_or_default();
    format!("{last}（共试了 {} 次）", faults.len())
}

/// Why an attempt of sysbench gave no rate.
fn sysbench_fault(fault: &sysbench::Fault) -> String {
    match fault {
        sysbench::Fault::Tool(fault) => tool_fault("sysbench", fault, seconds),
        sysbench::Fault::NoRate => "sysbench 没有输出每秒事件数".to_owned(),
    }
}

/// How a run of `tool` went wrong; `limit` words its time limit.
fn tool_fault(tool: &str, fault: &job::Fault, limit: fn(Duration) -> String) -> String {
    let quoted = |last: &Option<String>| last.clone().unwrap_or_else(|| "没有输出".to_owned());
    match fault {
        job::Fault::Spawn(error) => format!("无法运行 {tool}：{error}"),
        job::Fault::Read(error) => format!("无法读取 {tool} 的输出：{error}"),
        job::Fault::TimedOut(duration) => format!("超过 {}仍未结束，已停止", limit(*duration)),
        job::Fault::Exit { code, last } => format!("{tool} 退出码 {code}：{}", quoted(last)),
        job::Fault::Signal { last } => format!("{tool} 被信号终止：{}", quoted(last)),
    }
}

/// A limit in seconds, such as `60 秒`.
fn seconds(limit: Duration) -> String {
    format!("{} 秒", limit.as_secs())
}
