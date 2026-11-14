//! The disk group's words: 硬盘健康 (smartctl) and why it stopped short.

use bench_tool::job;
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
        Health::ReadFailed(fault) => Some(format!("读取失败：{}", read(fault))),
        Health::Unsupported => Some("不支持 SMART".to_owned()),
        Health::Unknown { smart_off: true } => {
            Some("的 SMART 功能已关闭，本程序不会替它打开".to_owned())
        }
        Health::Passed | Health::Failed | Health::Unknown { smart_off: false } => None,
    }
}

/// Why a disk could not be read.
fn read(fault: &health::Fault) -> String {
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
