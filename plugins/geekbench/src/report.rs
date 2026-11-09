//! What Geekbench 7 shows (design cpu, §6.6): the scores and the result link, and notes on
//! memory, swap and retries.

use std::io;
use std::path::Path;

use bench_api::Record;

use crate::memory::GIB;
use crate::output::Scores;
use crate::process::Adjustment;

/// The reason when Geekbench was killed for lack of memory.
pub const OUT_OF_MEMORY: &str = "运行中因内存不足被系统终止";

/// The reason when a run went over its limit.
pub const TIMED_OUT: &str = "超过 60 分钟仍未结束，已停止";

/// The reason when the item was cancelled.
pub const CANCELLED: &str = "已中断";

/// Why no swap is added when the kernel would not use it (CP-18).
pub const SWAP_UNUSED: &str = "本机 vm.swappiness 为 0，内核不会把内存换出到 swap";

/// 单核, 多核 and 结果链接.
#[must_use]
pub fn scores(scores: &Scores) -> Vec<Record> {
    vec![
        Record::fact("single", "单核", scores.single.to_string()),
        Record::fact("multi", "多核", scores.multi.to_string()),
        Record::fact(
            "url",
            "结果链接",
            scores.url.clone().unwrap_or_else(|| "不可用".to_owned()),
        ),
    ]
}

/// The reason after the last failed upload.
#[must_use]
pub fn upload_failed(reason: &str) -> String {
    format!("上传失败（{reason}），免费版只有上传成功才给出分数")
}

/// A note on a run that succeeded after failed uploads, or `None` when the first one did.
#[must_use]
pub fn retried(failures: &[String]) -> Option<Record> {
    let last = failures.last()?;
    Some(Record::fact(
        "note-retries",
        "说明",
        format!("上传失败 {} 次后重跑成功，失败原因：{last}", failures.len()),
    ))
}

/// The note on temporary swap: `need` and `available` are bytes, `gib` the swap's size.
#[must_use]
pub fn swap_added(need: u64, available: u64, gib: u64) -> Record {
    Record::fact(
        "note-memory",
        "说明",
        format!(
            "可用内存不足（{}），临时加了 {gib} GiB swap，分数可能偏低",
            estimate(need, available)
        ),
    )
}

/// The note when memory is short and no swap could be added, for `reason`.
#[must_use]
pub fn swap_impossible(need: u64, available: u64, reason: &str) -> Record {
    Record::fact(
        "note-memory",
        "说明",
        format!(
            "可用内存可能不足（{}），无法临时加 swap：{reason}",
            estimate(need, available)
        ),
    )
}

/// Why a swap file could not be made or turned on.
#[must_use]
pub fn swap_error(err: &io::Error) -> String {
    match err.kind() {
        // EPERM: swapon needs CAP_SYS_ADMIN, which containers seldom have.
        io::ErrorKind::PermissionDenied => "没有权限（容器里通常不允许）".to_owned(),
        // EINVAL: ZFS, tmpfs, overlay and copy-on-write btrfs files cannot hold swap.
        io::ErrorKind::InvalidInput => "运行目录所在的文件系统不支持 swap 文件".to_owned(),
        _ => err.to_string(),
    }
}

/// Why there is no room for swap, from the free space of the work directory's filesystem.
#[must_use]
pub fn no_room(free_space: Option<u64>) -> String {
    match free_space {
        Some(free) => format!("运行目录所在的磁盘只剩 {} GiB", tenths(free)),
        None => "无法读取运行目录所在磁盘的剩余空间".to_owned(),
    }
}

/// The note when the swap could not be turned off, with what to do by hand.
#[must_use]
pub fn swap_left_on(path: &Path, err: &io::Error) -> Record {
    Record::fact(
        "note-swap",
        "说明",
        format!(
            "无法关闭临时 swap：{err}；请手动执行 swapoff {}，再删除这个文件",
            path.display()
        ),
    )
}

/// A note when Geekbench could not be made the kernel's first choice, or when the program
/// kept that choice for itself.
#[must_use]
pub fn oom(adjustment: &Adjustment) -> Option<Record> {
    let text = match adjustment {
        Adjustment::Done => return None,
        Adjustment::NotSet(err) => format!("无法让 Geekbench 在内存耗尽时优先被结束：{err}"),
        Adjustment::NotRestored(err) => format!("无法恢复本程序的 oom_score_adj：{err}"),
    };
    Some(Record::fact("note-oom", "说明", text))
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
