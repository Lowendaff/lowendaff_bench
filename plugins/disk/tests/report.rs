//! What 硬盘性能 prints: YABS's number formats, the tables in YABS's layout, and the reasons to
//! skip.

use std::fs;
use std::path::Path;

use bench_api::Record;
use bench_api::Table;
use bench_host::storage::FilesystemKind;
use plugin_disk::fio::Outcome;
use plugin_disk::fio::Rates;
use plugin_disk::report;
use plugin_disk::report::FileSize;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// Rates whose cells YABS's own functions printed as below (`tests/fixtures/README.md`).
const FOUR_K: Rates = Rates {
    read_kib: 90_645,
    read_iops: 22_661,
    write_kib: 90_889,
    write_iops: 22_722,
};
const SIXTY_FOUR_K: Rates = Rates {
    read_kib: 1_065_063,
    read_iops: 16_641,
    write_kib: 1_066_789,
    write_iops: 16_668,
};

fn row(cells: [&str; 3]) -> Vec<String> {
    let mut row = Vec::new();
    for cell in cells {
        row.push(cell.to_owned());
    }
    row
}

#[test]
fn speeds_and_iops_print_exactly_as_yabs_prints_them() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yabs-format.txt");
    let cases = fs::read_to_string(path).expect("fixture is readable");
    let mut checked = 0;
    for line in cases.lines() {
        let mut fields = line.split('\t');
        let (Some(kind), Some(input), Some(expected), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            panic!("malformed fixture line: {line:?}");
        };
        let input: u64 = input.parse().expect("the input is an integer");
        let actual = match kind {
            "speed" => report::speed(input),
            "iops" => report::iops(input),
            other => panic!("unknown kind {other}"),
        };
        assert_eq!(actual, expected, "{kind} of {input}");
        checked += 1;
    }
    assert_eq!(checked, 655, "every case of the fixture was checked");
}

#[test]
fn the_file_size_follows_the_architecture_as_in_yabs() {
    let two = FileSize {
        bytes: 2 * GIB,
        fio: "2G",
        text: "2 GiB",
    };
    let half = FileSize {
        bytes: 512 * MIB,
        fio: "512M",
        text: "512 MiB",
    };
    assert_eq!(FileSize::for_arch("x86_64"), two);
    assert_eq!(FileSize::for_arch("aarch64"), half);
    assert_eq!(FileSize::for_arch("arm"), half);
    assert_eq!(FileSize::for_arch("riscv64"), half);
    assert_eq!(FileSize::for_arch("powerpc64"), two);
}

#[test]
fn a_ram_disk_is_skipped_whatever_its_free_space() {
    let reason =
        "测试目录在内存盘上，测到的会是内存速度；请在硬盘上的目录运行，或用 --work-dir 指定";
    let size = FileSize::for_arch("x86_64");
    assert_eq!(
        report::skip_reason(Some(FilesystemKind::Memory), 100 * GIB, size),
        Some(reason.to_owned())
    );
}

#[test]
fn exactly_enough_free_space_passes_as_in_yabs() {
    for (arch, needed, text) in [
        ("x86_64", 2 * GIB, "2 GiB"),
        ("aarch64", 512 * MIB, "512 MiB"),
    ] {
        let size = FileSize::for_arch(arch);
        for kind in [None, Some(FilesystemKind::Other), Some(FilesystemKind::Zfs)] {
            assert_eq!(report::skip_reason(kind, needed, size), None, "{arch}");
            let short = report::skip_reason(kind, needed - 1, size).expect("too little space");
            assert!(
                short.starts_with(&format!("测试目录可用空间不足 {text}（只有 ")),
                "{short}"
            );
        }
    }
}

#[test]
fn the_free_space_shown_is_cut_not_rounded() {
    let size = FileSize::for_arch("x86_64");
    for (available, shown) in [
        (2 * GIB - 1, "1.9 GiB"),
        (GIB + GIB / 4, "1.2 GiB"),
        (GIB, "1.0 GiB"),
        (GIB - 1, "1023.9 MiB"),
        (812 * MIB + MIB / 2, "812.5 MiB"),
        (0, "0.0 MiB"),
    ] {
        assert_eq!(
            report::skip_reason(None, available, size),
            Some(format!("测试目录可用空间不足 2 GiB（只有 {shown}）"))
        );
    }
}

#[test]
fn zfs_with_little_free_space_gets_yabss_note() {
    let note = Record::fact(
        "zfs",
        "说明",
        "测试目录在 ZFS 上，可用空间少于 48 GiB，结果可能不准（与 YABS 的提示相同）",
    );
    let zfs = Some(FilesystemKind::Zfs);
    assert_eq!(report::zfs_note(zfs, Some(24), 48 * GIB - 1), Some(note));
    assert_eq!(report::zfs_note(zfs, Some(24), 48 * GIB), None);
    assert_eq!(
        report::zfs_note(zfs, None, 0),
        None,
        "no zfs module, no note"
    );
    assert_eq!(
        report::zfs_note(Some(FilesystemKind::Other), Some(24), 0),
        None
    );
    assert_eq!(report::zfs_note(None, Some(24), 0), None);
}

#[test]
fn the_method_comes_first() {
    assert_eq!(
        report::method(FileSize::for_arch("aarch64")),
        Record::fact(
            "method",
            "测试方式",
            "与 YABS 相同：读写各半的随机测试，测试文件 512 MiB"
        )
    );
}

#[test]
fn a_table_has_yabss_rows_and_one_column_per_block_size() {
    let first = Outcome::Done(FOUR_K);
    let second = Outcome::Done(SIXTY_FOUR_K);
    let expected = Table::try_new(
        "results-4k",
        "4K 与 64K",
        row(["块大小", "4K", "64K"]),
        vec![
            row(["读取", "92.82 MB/s（22.6k）", "1.09 GB/s（16.6k）"]),
            row(["写入", "93.07 MB/s（22.7k）", "1.09 GB/s（16.6k）"]),
            row(["合计", "185.89 MB/s（45.3k）", "2.18 GB/s（33.3k）"]),
        ],
    )
    .expect("rows as wide as the header");
    let pair = [("4k", &first), ("64k", &second)];
    assert_eq!(report::table(pair), Ok(Record::Table(expected)));
    assert_eq!(report::notes(pair), Vec::<Record>::new());
}

#[test]
fn tests_without_a_result_show_why_after_the_table() {
    let first = Outcome::TimedOut;
    let second = Outcome::Failed("fio: error=No space left on device".to_owned());
    let expected = Table::try_new(
        "results-512k",
        "512K 与 1M",
        row(["块大小", "512K", "1M"]),
        vec![
            row(["读取", "超时", "出错"]),
            row(["写入", "超时", "出错"]),
            row(["合计", "超时", "出错"]),
        ],
    )
    .expect("rows as wide as the header");
    let pair = [("512k", &first), ("1m", &second)];
    assert_eq!(report::table(pair), Ok(Record::Table(expected)));
    assert_eq!(
        report::notes(pair),
        vec![
            Record::fact("note-512k", "说明", "512K 超过 35 秒仍未结束，已停止"),
            Record::fact(
                "note-1m",
                "说明",
                "1M 出错：fio: error=No space left on device"
            ),
        ]
    );
}

#[test]
fn impossible_rates_still_print() {
    // Far beyond any disk, where awk changes notation; the cell must still print.
    let speed = report::speed(u64::MAX);
    assert!(speed.ends_with(" GB/s"), "{speed}");
    let iops = report::iops(u64::MAX);
    assert!(iops.ends_with('k'), "{iops}");
}
