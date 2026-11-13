//! What 硬盘性能 prints: YABS's number formats, its two tables, and the
//! reasons to skip.

use bench_api::Record;
use bench_api::RecordError;
use bench_api::Table;
use bench_host::storage::FilesystemKind;

use crate::fio::Outcome;
use crate::fio::Rates;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// The test file's size, which YABS chooses by architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSize {
    /// Size in bytes; the test needs this much free space.
    pub bytes: u64,
    /// fio's `--size` value.
    pub fio: &'static str,
    /// The size as printed.
    pub text: &'static str,
}

impl FileSize {
    /// 512 MiB on ARM and RISC-V, 2 GiB elsewhere, as YABS's `disk_test` chooses.
    #[must_use]
    pub fn for_arch(arch: &str) -> Self {
        if matches!(arch, "aarch64" | "arm" | "riscv64") {
            Self {
                bytes: 512 * MIB,
                fio: "512M",
                text: "512 MiB",
            }
        } else {
            Self {
                bytes: 2 * GIB,
                fio: "2G",
                text: "2 GiB",
            }
        }
    }
}

/// Why the test cannot measure the disk here, if it cannot: the test directory is on a RAM
/// disk, or has less free space than the test file needs. Exactly enough passes, as in
/// YABS.
#[must_use]
pub fn skip_reason(kind: Option<FilesystemKind>, available: u64, size: FileSize) -> Option<String> {
    if kind == Some(FilesystemKind::Memory) {
        return Some(
            "测试目录在内存盘上，测到的会是内存速度；请在硬盘上的目录运行，或用 --work-dir 指定"
                .to_owned(),
        );
    }
    (available < size.bytes).then(|| {
        format!(
            "测试目录可用空间不足 {}（只有 {}）",
            size.text,
            size_text(available)
        )
    })
}

/// YABS's warning for ZFS: results may be inaccurate with less free space than twice
/// `spa_asize_inflation`, in GiB.
#[must_use]
pub fn zfs_note(
    kind: Option<FilesystemKind>,
    inflation: Option<u64>,
    available: u64,
) -> Option<Record> {
    if kind != Some(FilesystemKind::Zfs) {
        return None;
    }
    let needed = inflation?.checked_mul(2)?;
    (available < needed.saturating_mul(GIB)).then(|| {
        Record::fact(
            "zfs",
            "说明",
            format!(
                "测试目录在 ZFS 上，可用空间少于 {needed} GiB，结果可能不准（与 YABS 的提示相同）"
            ),
        )
    })
}

/// The first line of the section: the method and the file size.
#[must_use]
pub fn method(size: FileSize) -> Record {
    Record::fact(
        "method",
        "测试方式",
        format!("与 YABS 相同：读写各半的随机测试，测试文件 {}", size.text),
    )
}

/// The table for two block sizes, in YABS's layout: rows 读取, 写入 and 合计, one column per
/// block size, each cell a speed with its IOPS.
///
/// # Errors
///
/// Never in practice: every row has as many cells as the header.
pub fn table(pair: [(&str, &Outcome); 2]) -> Result<Record, RecordError> {
    let [(first, first_outcome), (second, second_outcome)] = pair;
    let headers = vec!["块大小".to_owned(), label(first), label(second)];
    let mut rows = Vec::new();
    for (name, pick) in [
        ("读取", read as fn(&Rates) -> (u64, u64)),
        ("写入", write),
        ("合计", total),
    ] {
        rows.push(vec![
            name.to_owned(),
            cell(first_outcome, pick),
            cell(second_outcome, pick),
        ]);
    }
    let table = Table::try_new(
        &format!("results-{first}"),
        &format!("{} 与 {}", label(first), label(second)),
        headers,
        rows,
    )?;
    Ok(Record::Table(table))
}

/// 说明 lines for the tests of a table that have no result.
#[must_use]
pub fn notes(pair: [(&str, &Outcome); 2]) -> Vec<Record> {
    let mut notes = Vec::new();
    for (block_size, outcome) in pair {
        let text = match outcome {
            Outcome::Done(_) => continue,
            Outcome::TimedOut => format!("{} 超过 35 秒仍未结束，已停止", label(block_size)),
            Outcome::Failed(reason) => format!("{} 出错：{reason}", label(block_size)),
        };
        notes.push(Record::fact(&format!("note-{block_size}"), "说明", text));
    }
    notes
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

fn cell(outcome: &Outcome, pick: fn(&Rates) -> (u64, u64)) -> String {
    match outcome {
        Outcome::Done(rates) => {
            let (kib, operations) = pick(rates);
            format!("{}（{}）", speed(kib), iops(operations))
        }
        Outcome::TimedOut => "超时".to_owned(),
        Outcome::Failed(_) => "出错".to_owned(),
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
fn size_text(bytes: u64) -> String {
    let (unit, name) = if bytes >= GIB {
        (GIB, "GiB")
    } else {
        (MIB, "MiB")
    };
    let tenths = u128::from(bytes) * 10 / u128::from(unit);
    format!("{}.{} {name}", tenths / 10, tenths % 10)
}
