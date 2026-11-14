//! The memory group's words: the chapters of 内存性能, sysbench, STREAM, latency and NUMA, each
//! with its parameters, its tables and its notes; and why each stopped short.

use bench_api::typed::Stop;
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
use crate::results::Report;

/// What a missing value shows.
const MISSING: &str = "—";

/// The note when the kernel describes no caches.
const CACHE_UNKNOWN: &str = "没有末级缓存的信息，按 32 MiB 计算";

/// The note when STREAM's arrays are below four times the last-level caches.
const STREAM_RULE: &str = "未完全符合 STREAM 运行规则（每个数组至少为末级缓存总和的 4 倍）";

/// sysbench's chapter: its parameters, reading and writing with one thread and with all of them,
/// and its notes.
pub(super) fn sysbench(
    out: &mut Out,
    report: &Report<sysbench::Output, sysbench::Failure>,
    indent: &str,
) {
    let Some(output) = &report.output else {
        return;
    };
    let mut parameters = version(output.version.as_deref());
    parameters.push_str(&format!(
        "顺序读写，每项 {} 秒；单线程一块 {}",
        sysbench::measure::SECONDS,
        size(output.single)
    ));
    if let Some(per_thread) = output.per_thread {
        parameters.push_str(&format!("，多线程每个线程一块 {}", size(per_thread)));
    }
    out.field(indent, "sysbench", &parameters);
    let mut notes = cache_note(output.cache_unknown);
    let threads = columns(output.threads);
    let headers = headers(&threads);
    let rows: Vec<Vec<String>> = [(Operation::Read, "读取"), (Operation::Write, "写入")]
        .into_iter()
        .map(|(operation, word)| {
            let mut cells = vec![word.to_owned()];
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
    out.line(&format!("{indent}sysbench 结果"));
    out.table(indent, &headers, &rows);
    if let Some(four) = output.small {
        notes.push(format!(
            "可用内存不足，测试数据小于末级缓存总和的 4 倍（{}），\
                 一部分读写落在缓存里，结果偏高",
            size(four)
        ));
    }
    for run in &output.runs {
        let name = sysbench_run(run.operation, run.threads);
        notes.extend(retried(&name, &run.retries, sysbench_fault));
    }
    for note in notes {
        out.field(indent, "说明", &note);
    }
}

/// Why sysbench's chapter stopped short.
pub(super) fn sysbench_failure(failure: &sysbench::Failure) -> String {
    match failure {
        sysbench::Failure::NoMemoryInfo => NO_MEMORY_INFO.to_owned(),
        sysbench::Failure::Run {
            operation,
            threads,
            faults,
        } => failed(&sysbench_run(*operation, *threads), faults, sysbench_fault),
        sysbench::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// The name of a sysbench run, such as `单线程读取`.
fn sysbench_run(operation: Operation, threads: usize) -> String {
    let word = match operation {
        Operation::Read => "读取",
        Operation::Write => "写入",
    };
    format!("{}{word}", threads_word(threads))
}

/// Why an attempt of sysbench gave no rate.
fn sysbench_fault(fault: &sysbench::Fault) -> String {
    match fault {
        sysbench::Fault::Tool(fault) => tool::fault("sysbench", fault, tool::seconds),
        sysbench::Fault::NoRate => "sysbench 没有输出传输速度".to_owned(),
    }
}

/// STREAM's chapter: its parameters, its four kernels with one thread and with all of them, and
/// its notes.
pub(super) fn stream(
    out: &mut Out,
    report: &Report<stream::Output, stream::Failure>,
    indent: &str,
) {
    let Some(output) = &report.output else {
        return;
    };
    let array = plan::array_bytes(output.millions);
    let spread = if output.threads > 1 {
        "；多线程按节点均匀绑定"
    } else {
        ""
    };
    let mut parameters = version(output.version.as_deref());
    parameters.push_str(&format!(
        "每个数组 {} 万个元素（{} MiB，三个合计 {} MiB），每项取 10 遍中最快的一遍{spread}",
        output.millions * 100,
        mib(array),
        mib(3 * array)
    ));
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
    out.line(&format!("{indent}STREAM 结果"));
    out.table(indent, &headers, &rows);
    if output.below_rule {
        notes.push(STREAM_RULE.to_owned());
    }
    for run in &output.runs {
        let name = format!("{}测试", threads_word(run.threads));
        notes.extend(retried(&name, &run.retries, stream_fault));
    }
    for note in notes {
        out.field(indent, "说明", &note);
    }
}

/// Why STREAM's chapter stopped short.
pub(super) fn stream_failure(failure: &stream::Failure) -> String {
    match failure {
        stream::Failure::NoMemoryInfo => NO_MEMORY_INFO.to_owned(),
        stream::Failure::Short { budget } => format!(
            "可用内存不足：三个数组最少要 {} MiB，可用内存的一半只有 {}",
            mib(3 * plan::array_bytes(STREAM_SIZES[0])),
            size(*budget)
        ),
        stream::Failure::Run { threads, faults } => failed(
            &format!("{}测试", threads_word(*threads)),
            faults,
            stream_fault,
        ),
        stream::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// Why an attempt of STREAM gave no rates.
fn stream_fault(fault: &stream::Fault) -> String {
    match fault {
        stream::Fault::Tool(fault) => tool::fault("STREAM", fault, tool::seconds),
        stream::Fault::Invalid(line) => format!("STREAM 的结果没有通过校验：{line}"),
        stream::Fault::NoResult => "STREAM 没有输出结果".to_owned(),
    }
}

/// The latency chapter: what it runs, each level's buffer and latency, and its notes.
pub(super) fn latency(
    out: &mut Out,
    report: &Report<latency::Output, latency::Failure>,
    indent: &str,
) {
    let Some(output) = &report.output else {
        return;
    };
    out.field(indent, "延迟", "multichase：随机指针追逐，每档 2.5 秒");
    let mut notes = cache_note(output.cache_unknown);
    let headers = ["层级", "缓冲区", "延迟"].map(str::to_owned);
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
    out.line(&format!("{indent}延迟结果"));
    out.table(indent, &headers, &rows);
    if !output.missing.is_empty() {
        notes.push(missing(&output.missing));
    }
    if let Some(four) = output.small {
        notes.push(format!(
            "可用内存不足，内存一档的缓冲区小于末级缓存总和的 4 倍（{}），\
                 一部分访问落在缓存里，延迟偏低",
            size(four)
        ));
    }
    for row in &output.rows {
        let name = spaced(level(row.level), "测试");
        notes.extend(retried(&name, &row.retries, latency_fault));
    }
    for note in notes {
        out.field(indent, "说明", &note);
    }
}

/// Why the latency chapter stopped short.
pub(super) fn latency_failure(failure: &latency::Failure) -> String {
    match failure {
        latency::Failure::NoMemoryInfo => NO_MEMORY_INFO.to_owned(),
        latency::Failure::Run { level: run, faults } => {
            failed(&spaced(level(*run), "测试"), faults, latency_fault)
        }
        latency::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// Why an attempt of multichase gave no latency.
fn latency_fault(fault: &latency::Fault) -> String {
    match fault {
        latency::Fault::Tool(fault) => tool::fault("multichase", fault, tool::seconds),
        latency::Fault::NoLatency => "multichase 没有输出延迟".to_owned(),
    }
}

/// The NUMA chapter: what it runs, the bandwidth and the latency from each node's CPUs to each
/// node's memory, and its notes. A chapter that the kernel refused to bind shows only why.
pub(super) fn numa(out: &mut Out, report: &Report<numa::Output, numa::Failure>, indent: &str) {
    let Some(output) = &report.output else {
        return;
    };
    if matches!(report.stop, Some(Stop::Skipped(numa::Failure::Refused))) {
        return;
    }
    let parameters = format!(
        "{} 个节点；带宽用 multiload 的 stream-triad，每格 3 次采样取最快；延迟同上",
        output.nodes
    );
    out.field(indent, "NUMA", &parameters);
    let mut notes = cache_note(output.cache_unknown);
    let mut headers = vec![String::new()];
    headers.extend(output.columns.iter().map(|node| format!("节点 {node}")));
    let tables = [
        (
            Kind::Bandwidth,
            "NUMA 带宽（MiB/s；行是线程所在节点，列是内存所在节点）",
        ),
        (Kind::Latency, "NUMA 延迟（ns）"),
    ];
    for (kind, title) in tables {
        let rows: Vec<Vec<String>> = output
            .rows
            .iter()
            .zip(&output.cells)
            .map(|(row, cells)| {
                let mut line = vec![format!("节点 {row}")];
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
        out.line(&format!("{indent}{title}"));
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
                let name = numa_run(*row, *column, kind);
                notes.extend(retried(&name, retries, |fault| numa_fault(fault, kind)));
            }
        }
    }
    for note in notes {
        out.field(indent, "说明", &note);
    }
}

/// Why the NUMA chapter stopped short.
pub(super) fn numa_failure(failure: &numa::Failure) -> String {
    match failure {
        numa::Failure::NoMemoryInfo => NO_MEMORY_INFO.to_owned(),
        numa::Failure::OneNode => "需要多个 NUMA 节点".to_owned(),
        numa::Failure::Refused => "容器不允许把内存绑定到指定节点".to_owned(),
        numa::Failure::Run {
            row,
            column,
            kind,
            faults,
        } => failed(&numa_run(*row, *column, *kind), faults, |fault| {
            numa_fault(fault, *kind)
        }),
        numa::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// The name of a run of the NUMA matrix, such as `线程在节点 0、内存在节点 1 的带宽测试`.
fn numa_run(row: usize, column: usize, kind: Kind) -> String {
    let test = match kind {
        Kind::Bandwidth => "带宽测试",
        Kind::Latency => "延迟测试",
    };
    format!("线程在节点 {row}、内存在节点 {column} 的{test}")
}

/// Why an attempt of a NUMA run gave no value.
fn numa_fault(fault: &numa::Fault, kind: Kind) -> String {
    let tool = match kind {
        Kind::Bandwidth => "multiload",
        Kind::Latency => "multichase",
    };
    match fault {
        numa::Fault::Tool(fault) => tool::fault(tool, fault, tool::seconds),
        numa::Fault::NoBandwidth => "multiload 没有输出带宽".to_owned(),
        numa::Fault::NoLatency => "multichase 没有输出延迟".to_owned(),
        numa::Fault::Refused => "容器不允许把内存绑定到指定节点".to_owned(),
    }
}

/// Why a plugin could not size its runs.
const NO_MEMORY_INFO: &str = "读不到可用内存（/proc/meminfo）";

/// The version that a tool printed, followed by the colon that leads its parameters; nothing
/// when it printed none.
fn version(version: Option<&str>) -> String {
    version.map_or_else(String::new, |version| format!("{version}："))
}

/// The cache note, when the last-level caches were assumed.
fn cache_note(cache_unknown: bool) -> Vec<String> {
    if cache_unknown {
        vec![CACHE_UNKNOWN.to_owned()]
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

/// The header of a table of runs: 项目, then a column for each number of threads.
fn headers(threads: &[usize]) -> Vec<String> {
    let mut headers = vec!["项目".to_owned()];
    headers.extend(threads.iter().map(|&threads| column(threads)));
    headers
}

/// The header of the column of runs with `threads` threads.
fn column(threads: usize) -> String {
    if threads > 1 {
        format!("多线程（{threads} 线程）")
    } else {
        "单线程".to_owned()
    }
}

/// The name of the runs with `threads` threads, such as in `单线程读取`.
fn threads_word(threads: usize) -> &'static str {
    if threads > 1 {
        "多线程"
    } else {
        "单线程"
    }
}

/// A level's name in the latency table.
fn level(level: Level) -> &'static str {
    match level {
        Level::L1 => "L1",
        Level::L2 => "L2",
        Level::L3 => "L3",
        Level::Memory => "内存",
    }
}

/// `name` followed by `word`, with a space after a Latin name.
fn spaced(name: &str, word: &str) -> String {
    if name.ends_with(|c: char| c.is_ascii_alphanumeric()) {
        format!("{name} {word}")
    } else {
        format!("{name}{word}")
    }
}

/// A measured value with its unit, or `—` when it was not measured.
fn shown(value: Option<&Value>, unit: &str) -> String {
    value.map_or_else(
        || MISSING.to_owned(),
        |value| format!("{} {unit}", value.text),
    )
}

/// The note on the cache levels left out of the latency table.
fn missing(levels: &[Level]) -> String {
    let count = match levels.len() {
        1 => "一",
        2 => "两",
        _ => "三",
    };
    let names: Vec<&str> = levels.iter().map(|&each| level(each)).collect();
    format!("没有 {} 的信息，跳过这{count}档", names.join("、"))
}

/// A note on a run that worked after failed attempts, or `None` when its first attempt did.
/// `run` names the run, such as `单线程读取`.
fn retried<F>(run: &str, faults: &[F], words: impl Fn(&F) -> String) -> Option<String> {
    let last = faults.last()?;
    Some(format!(
        "{run}失败 {} 次后成功，最后一次失败的原因：{}",
        faults.len(),
        words(last)
    ))
}

/// Why a run failed: its name, the last attempt's reason, and the number of attempts.
fn failed<F>(run: &str, faults: &[F], words: impl Fn(&F) -> String) -> String {
    let last = faults.last().map_or_else(String::new, words);
    format!("{run}：{last}（共试了 {} 次）", faults.len())
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
        format!("{bytes} 字节")
    }
}

/// Mebibytes, rounded.
fn mib(bytes: u64) -> u64 {
    (bytes + MIB / 2) / MIB
}
