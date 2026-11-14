//! The disk group's words: 硬盘健康 (smartctl) and 硬盘性能 (fio, in YABS's formats), and why each
//! stopped short.

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
use crate::results::Report;

/// What a missing value shows.
const MISSING: &str = "—";

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// 硬盘健康: the tool, then the disks in a table, a note on each disk that needs one, and a note
/// on each device node that smartctl created and that could not be removed.
pub(super) fn health(
    out: &mut Out,
    report: &Report<health::Output, health::Failure>,
    indent: &str,
) {
    let Some(output) = &report.output else {
        return;
    };
    if !output.disks.is_empty() {
        let version = output
            .version
            .as_deref()
            .map_or_else(String::new, |version| format!(" {version}"));
        out.line(&format!("{indent}smartctl{version}"));
        let headers = [
            "设备",
            "健康",
            "通电时间",
            "温度",
            "已用寿命",
            "累计写入",
            "坏块",
            "接口错误",
        ]
        .map(str::to_owned);
        let with_host = several_cards(&output.disks);
        let rows: Vec<Vec<String>> = output
            .disks
            .iter()
            .map(|disk| cells(disk, with_host))
            .collect();
        out.table(indent, &headers, &rows);
        for disk in &output.disks {
            if let Some(note) = note(&disk.health) {
                let name = location(&disk.location, with_host);
                out.field(indent, "说明", &format!("{name} {note}"));
            }
        }
    }
    for left in &output.nodes_left {
        let note = format!("无法删除 {}：{}", left.path.display(), left.error);
        out.field(indent, "说明", &note);
    }
}

/// Why 硬盘健康 stopped short.
pub(super) fn health_failure(failure: &health::Failure) -> String {
    match failure {
        health::Failure::NoDisks => "smartctl 没有找到硬盘".to_owned(),
        health::Failure::Scan(health::Fault::Tool(job::Fault::TimedOut(limit))) => {
            format!("smartctl 扫描硬盘失败：超过 {} 秒仍未完成", limit.as_secs())
        }
        health::Failure::Scan(fault) => format!("smartctl 扫描硬盘失败：{}", smartctl(fault)),
        health::Failure::AllFailed => "所有硬盘都读取失败".to_owned(),
        health::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// The 设备 cell: a kernel name, or a disk on a RAID card, which names its card when the disks
/// sit on more than one.
fn location(location: &Location, with_host: bool) -> String {
    match location {
        Location::Named(name) => name.clone(),
        Location::Card { host, numbers } => {
            let numbers: Vec<String> = numbers.iter().map(u32::to_string).collect();
            let numbers = numbers.join(":");
            if with_host {
                format!("RAID 卡 host{host} 的 {numbers} 号盘")
            } else {
                format!("RAID 卡 {numbers} 号盘")
            }
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
    let values = &disk.values;
    vec![
        location(&disk.location, with_host),
        verdict(&disk.health).to_owned(),
        shown(values.hours, |hours| format!("{hours} 小时")),
        shown(values.temperature, |degrees| format!("{degrees} °C")),
        shown(values.endurance_used, |percent| format!("{percent}%")),
        shown(values.written, written),
        bad_blocks(&values.bad_blocks),
        shown(values.interface_errors, |count| count.to_string()),
    ]
}

/// The 健康 cell.
fn verdict(health: &Health) -> &'static str {
    match health {
        Health::Passed => "通过",
        Health::Failed => "未通过",
        Health::Unknown { .. } => "未知",
        Health::Unsupported => "不支持",
        Health::ReadFailed(_) => "读取失败",
    }
}

/// The 说明 line's text after the disk's name, for a disk that needs one.
fn note(health: &Health) -> Option<String> {
    match health {
        Health::ReadFailed(fault) => Some(format!("读取失败：{}", read_failure(fault))),
        Health::Unsupported => Some("不支持 SMART".to_owned()),
        Health::Unknown { smart_off: true } => {
            Some("的 SMART 功能已关闭，本程序不会替它打开".to_owned())
        }
        Health::Passed | Health::Failed | Health::Unknown { smart_off: false } => None,
    }
}

/// Why a disk could not be read.
fn read_failure(fault: &health::Fault) -> String {
    match fault {
        health::Fault::Tool(job::Fault::TimedOut(limit)) => {
            format!("超过 {} 秒仍未读完，已停止", limit.as_secs())
        }
        fault => smartctl(fault),
    }
}

/// Why smartctl gave nothing to read.
fn smartctl(fault: &health::Fault) -> String {
    match fault {
        health::Fault::Tool(fault) => tool::fault("smartctl", fault, tool::seconds),
        health::Fault::Unparsable => "smartctl 的输出无法解析".to_owned(),
        health::Fault::Message(message) => message.clone(),
        health::Fault::Unopened => "打不开设备，或读不到设备识别信息".to_owned(),
        health::Fault::Refused => "smartctl 不接受命令参数".to_owned(),
    }
}

/// The 坏块 cell: SATA's reallocated and pending sectors, or one count.
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

/// 硬盘性能: the tool, the method and the file's size, YABS's note on ZFS, then a table for each
/// two block sizes in YABS's layout, each followed by a note on each test without a result.
pub(super) fn fio(out: &mut Out, report: &Report<fio::Output, fio::Failure>, indent: &str) {
    let Some(output) = &report.output else {
        return;
    };
    let version = output
        .version
        .as_deref()
        .map_or_else(String::new, |version| format!(" {version}"));
    out.line(&format!("{indent}fio{version}"));
    let method = format!(
        "与 YABS 相同：读写各半的随机测试，测试文件 {}",
        file_size(output.size)
    );
    out.field(indent, "测试方式", &method);
    if let Some(needed) = output.zfs_needed {
        let note = format!(
            "测试目录在 ZFS 上，可用空间少于 {needed} GiB，结果可能不准（与 YABS 的提示相同）"
        );
        out.field(indent, "说明", &note);
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
        out.line(&format!("{indent}{} 与 {}", label(first), label(second)));
        let headers = ["块大小".to_owned(), label(first), label(second)];
        let mut rows = Vec::new();
        for (name, pick) in [
            ("读取", read as fn(&Rates) -> (u64, u64)),
            ("写入", write),
            ("合计", total),
        ] {
            let [first, second] = outcomes.map(|outcome| cell(outcome, pick));
            rows.push(vec![name.to_owned(), first, second]);
        }
        out.table(indent, &headers, &rows);
        for (block_size, outcome) in pair.into_iter().zip(outcomes) {
            let Some(Outcome::Failed(fault)) = outcome else {
                continue;
            };
            let note = match fault {
                fio::Fault::Tool(job::Fault::TimedOut(limit)) => format!(
                    "{} 超过 {} 秒仍未结束，已停止",
                    label(block_size),
                    limit.as_secs()
                ),
                fault => format!("{} 出错：{}", label(block_size), fio_fault(fault)),
            };
            out.field(indent, "说明", &note);
        }
    }
}

/// Why 硬盘性能 stopped short.
pub(super) fn fio_failure(failure: &fio::Failure) -> String {
    match failure {
        fio::Failure::NoSpaceInfo => "无法读取测试目录的可用空间".to_owned(),
        fio::Failure::InMemory => {
            "测试目录在内存盘上，测到的会是内存速度；请在硬盘上的目录运行，或用 --work-dir 指定"
                .to_owned()
        }
        fio::Failure::Space { needed, available } => format!(
            "测试目录可用空间不足 {}（只有 {}）",
            file_size(*needed),
            free_space(*available)
        ),
        fio::Failure::NoDirectIo => "测试目录所在的文件系统不支持直接 I/O".to_owned(),
        fio::Failure::Setup(fault) => format!("生成测试文件失败：{}", fio_fault(fault)),
        fio::Failure::NoResult => "四种块大小都没有结果".to_owned(),
        fio::Failure::Cancelled => "已中断".to_owned(),
    }
}

/// Why a run of fio gave no rates: fio's own error line when it ended badly.
fn fio_fault(fault: &fio::Fault) -> String {
    match fault {
        fio::Fault::Tool(job::Fault::Exit { last, .. } | job::Fault::Signal { last }) => {
            last.clone().unwrap_or_else(|| "fio 没有输出".to_owned())
        }
        fio::Fault::Tool(fault) => tool::fault("fio", fault, tool::seconds),
        fio::Fault::NoResult => "fio 没有输出结果".to_owned(),
        fio::Fault::Error(error) => format!("fio 报告错误 {error}"),
        fio::Fault::Field(field) => format!("无法解析 fio 输出的第 {field} 个字段"),
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
    match outcome {
        Some(Outcome::Done(rates)) => {
            let (kib, operations) = pick(rates);
            format!("{}（{}）", speed(kib), iops(operations))
        }
        Some(Outcome::Failed(fio::Fault::Tool(job::Fault::TimedOut(_)))) => "超时".to_owned(),
        Some(Outcome::Failed(_)) => "出错".to_owned(),
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
