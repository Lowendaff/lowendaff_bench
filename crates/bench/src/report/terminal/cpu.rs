//! The CPU group's words: CPU 性能 (sysbench) and Geekbench 7, and why each stopped short.

use std::time::Duration;

use bench_tool::job;
use bench_tool::job::Adjustment;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::memory::GIB;
use plugin_cpu::geekbench::result::Memory;
use plugin_cpu::geekbench::result::NoSwap;
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

/// A limit in minutes, such as `60 分钟`.
fn minutes(limit: Duration) -> String {
    format!("{} 分钟", limit.as_secs() / 60)
}

/// Geekbench 7: the version that ran, the scores and the result link, then a note on memory, on
/// the kernel's first choice, on runs again after failed uploads, and on swap left on.
pub(super) fn geekbench(
    out: &mut Out,
    report: &Report<geekbench::Output, geekbench::Failure>,
    indent: &str,
) {
    let Some(output) = &report.output else {
        return;
    };
    if let Some(scores) = &output.scores {
        if let Some(version) = &scores.version {
            out.line(&format!("{indent}Geekbench {version}"));
        }
        out.field(indent, "单核", &scores.single.to_string());
        out.field(indent, "多核", &scores.multi.to_string());
        out.field(
            indent,
            "结果链接",
            scores.url.as_deref().unwrap_or("不可用"),
        );
    }
    if let Some(memory) = &output.memory {
        out.field(indent, "说明", &memory_note(memory));
    }
    if let Some(note) = output.first_to_go.as_ref().and_then(first_to_go_note) {
        out.field(indent, "说明", &note);
    }
    if output.scores.is_some()
        && let Some(last) = output.uploads.last()
    {
        let count = output.uploads.len();
        let note = format!("上传失败 {count} 次后重跑成功，失败原因：{last}");
        out.field(indent, "说明", &note);
    }
    if let Some(left) = &output.swap_left {
        let note = format!(
            "无法关闭临时 swap：{}；请手动执行 swapoff {}，再删除这个文件",
            left.error,
            left.path.display()
        );
        out.field(indent, "说明", &note);
    }
}

/// Why Geekbench 7 has no scores.
pub(super) fn geekbench_failure(failure: &geekbench::Failure) -> String {
    match failure {
        geekbench::Failure::Upload(reason) => {
            format!("上传失败（{reason}），免费版只有上传成功才给出分数")
        }
        geekbench::Failure::Run(geekbench::Fault::Tool(fault)) => {
            tool_fault("Geekbench", fault, minutes)
        }
        geekbench::Failure::Run(geekbench::Fault::NoScores) => "Geekbench 没有输出分数".to_owned(),
        geekbench::Failure::OutOfMemory => "运行中因内存不足被系统终止".to_owned(),
        geekbench::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// What was done about memory, when it may be short.
fn memory_note(memory: &Memory) -> String {
    match memory {
        Memory::Swapped {
            need,
            available,
            gib,
        } => format!(
            "可用内存不足（{}），临时加了 {gib} GiB swap，分数可能偏低",
            estimate(*need, *available)
        ),
        Memory::Short {
            need,
            available,
            why,
        } => format!(
            "可用内存可能不足（{}），无法临时加 swap：{}",
            estimate(*need, *available),
            no_swap(why)
        ),
    }
}

/// Why no temporary swap was added.
fn no_swap(why: &NoSwap) -> String {
    match why {
        NoSwap::Unused => "本机 vm.swappiness 为 0，内核不会把内存换出到 swap".to_owned(),
        NoSwap::NoRoom(Some(free)) => format!("运行目录所在的磁盘只剩 {} GiB", tenths(*free)),
        NoSwap::NoRoom(None) => "无法读取运行目录所在磁盘的剩余空间".to_owned(),
        NoSwap::Denied => "没有权限（容器里通常不允许）".to_owned(),
        NoSwap::Unsupported => "运行目录所在的文件系统不支持 swap 文件".to_owned(),
        NoSwap::Error(error) => error.clone(),
    }
}

/// A note when Geekbench could not be made the kernel's first choice, or when the program kept
/// that choice for itself.
fn first_to_go_note(adjustment: &Adjustment) -> Option<String> {
    match adjustment {
        Adjustment::Done => None,
        Adjustment::NotSet(error) => {
            Some(format!("无法让 Geekbench 在内存耗尽时优先被结束：{error}"))
        }
        Adjustment::NotRestored(error) => Some(format!("无法恢复本程序的 oom_score_adj：{error}")),
    }
}

/// `估计需要 5 GiB，可用 1.6 GiB`.
fn estimate(need: u64, available: u64) -> String {
    format!(
        "估计需要 {} GiB，可用 {} GiB",
        need / GIB,
        tenths(available)
    )
}

/// Bytes in GiB with one decimal, rounded down, so that nothing looks larger than it is.
fn tenths(bytes: u64) -> String {
    let tenths = bytes * 10 / GIB;
    format!("{}.{}", tenths / 10, tenths % 10)
}
