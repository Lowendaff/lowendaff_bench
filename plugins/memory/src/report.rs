//! What 内存性能 shows (design memory, §5.7, §5.8): each chapter's parameters, its tables and
//! its notes.

use bench_api::Record;
use bench_api::RecordError;
use bench_api::Table;

use crate::plan::GIB;
use crate::plan::KIB;
use crate::plan::MIB;
use crate::plan::STREAM_SIZES;

/// Seconds of each sysbench run (§5.3).
pub const SYSBENCH_SECONDS: u32 = 10;

/// What the latency chapter runs (§5.5).
pub const LATENCY: &str = "multichase：随机指针追逐，每档 2.5 秒";

/// The note when the kernel describes no caches (§5.2).
pub const CACHE_UNKNOWN: &str = "没有末级缓存的信息，按 32 MiB 计算";

/// The note when STREAM's arrays are below four times the last-level caches (§5.4).
pub const STREAM_RULE: &str = "未完全符合 STREAM 运行规则（每个数组至少为末级缓存总和的 4 倍）";

/// The NUMA chapter when the kernel refuses to bind memory to a node (§5.6).
pub const REFUSED: &str = "不可用：容器不允许把内存绑定到指定节点";

/// A size such as `256 MiB`, `16 KiB`, `1 GiB` or `30.5 MiB`.
#[must_use]
pub fn size(bytes: u64) -> String {
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

/// The name of the runs with `threads` threads, such as in `单线程读取`.
#[must_use]
pub fn threads_word(threads: usize) -> &'static str {
    if threads > 1 {
        "多线程"
    } else {
        "单线程"
    }
}

/// The header of the column of runs with `threads` threads.
#[must_use]
pub fn column(threads: usize) -> String {
    if threads > 1 {
        format!("多线程（{threads} 线程）")
    } else {
        "单线程".to_owned()
    }
}

/// `name` followed by `word`, with a space after a Latin name.
#[must_use]
pub fn spaced(name: &str, word: &str) -> String {
    if name.ends_with(|c: char| c.is_ascii_alphanumeric()) {
        format!("{name} {word}")
    } else {
        format!("{name}{word}")
    }
}

/// The sysbench chapter's parameters (§5.7).
#[must_use]
pub fn sysbench(version: &str, single: u64, per_thread: Option<u64>) -> String {
    let mut text = format!(
        "{version}：顺序读写，每项 {SYSBENCH_SECONDS} 秒；单线程一块 {}",
        size(single)
    );
    if let Some(per_thread) = per_thread {
        text.push_str(&format!("，多线程每个线程一块 {}", size(per_thread)));
    }
    text
}

/// The STREAM chapter's parameters, for arrays of `millions` million elements (§5.7).
#[must_use]
pub fn stream(version: &str, millions: u64, multi: bool) -> String {
    let array = crate::plan::array_bytes(millions);
    let spread = if multi {
        "；多线程按节点均匀绑定"
    } else {
        ""
    };
    format!(
        "{version}：每个数组 {} 万个元素（{} MiB，三个合计 {} MiB），\
         每项取 10 遍中最快的一遍{spread}",
        millions * 100,
        mib(array),
        mib(3 * array)
    )
}

/// The STREAM chapter when not even its smallest size fits in `budget` (§5.4).
#[must_use]
pub fn stream_short(budget: u64) -> String {
    let smallest = 3 * crate::plan::array_bytes(STREAM_SIZES[0]);
    format!(
        "可用内存不足：三个数组最少要 {} MiB，可用内存的一半只有 {}",
        mib(smallest),
        size(budget)
    )
}

/// The NUMA chapter's parameters, for `nodes` nodes (§5.7).
#[must_use]
pub fn numa(nodes: usize) -> String {
    format!("{nodes} 个节点；带宽用 multiload 的 stream-triad，每格 3 次采样取最快；延迟同上")
}

/// The note on sysbench's buffers when they are below four times the last-level caches.
#[must_use]
pub fn sysbench_small(cache: u64) -> String {
    format!(
        "可用内存不足，测试数据小于末级缓存总和的 4 倍（{}），一部分读写落在缓存里，结果偏高",
        size(cache.saturating_mul(4))
    )
}

/// The note on memory's latency buffer when it is below four times the last-level caches.
#[must_use]
pub fn latency_small(cache: u64) -> String {
    format!(
        "可用内存不足，内存一档的缓冲区小于末级缓存总和的 4 倍（{}），\
         一部分访问落在缓存里，延迟偏低",
        size(cache.saturating_mul(4))
    )
}

/// The note on the cache levels left out of the latency table (§5.5).
#[must_use]
pub fn missing(levels: &[&str]) -> String {
    let count = match levels.len() {
        1 => "一",
        2 => "两",
        _ => "三",
    };
    format!("没有 {} 的信息，跳过这{count}档", levels.join("、"))
}

/// A note on a run that succeeded after failed attempts, or `None` when the first attempt
/// succeeded. `run` names the run, such as `单线程读取`.
#[must_use]
pub fn retried(run: &str, failures: &[String]) -> Option<String> {
    let last = failures.last()?;
    Some(format!(
        "{run}失败 {} 次后成功，最后一次失败的原因：{last}",
        failures.len()
    ))
}

/// Why a chapter failed: the run whose every attempt failed, its last reason, and the number of
/// attempts.
#[must_use]
pub fn failure(run: &str, failures: &[String]) -> String {
    let last = failures.last().map_or("", String::as_str);
    format!("{run}：{last}（共试了 {} 次）", failures.len())
}

/// Why the item failed: the chapters that did (§5.8).
#[must_use]
pub fn failed(chapters: &[&str]) -> String {
    spaced(&chapters.join("、"), "出错")
}

/// A table whose missing cells show `—`; the first cell of each row is its name.
pub fn table(
    id: &str,
    title: &str,
    headers: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
) -> Result<Record, RecordError> {
    let rows = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| cell.unwrap_or_else(|| "—".to_owned()))
                .collect()
        })
        .collect();
    Ok(Record::Table(Table::try_new(id, title, headers, rows)?))
}

/// Mebibytes, rounded.
fn mib(bytes: u64) -> u64 {
    (bytes + MIB / 2) / MIB
}
