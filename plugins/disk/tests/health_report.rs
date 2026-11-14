//! The table and its 说明 lines.

mod common;

use bench_api::Record;
use bench_host::Root;
use plugin_disk::health::device::Device;
use plugin_disk::health::location::Location;
use plugin_disk::health::reading::Health;
use plugin_disk::health::reading::Reading;
use plugin_disk::health::reading::Values;
use plugin_disk::health::report;

fn reading(name: &str, kind: &str, fixture: &str) -> Reading {
    let device = Device {
        name: name.to_owned(),
        kind: kind.to_owned(),
    };
    Reading::new(device, Some(&common::fixture(fixture)))
}

/// A reading made up for one test.
fn made_up(name: &str, kind: &str, serial: Option<&str>, health: Health) -> Reading {
    Reading {
        device: Device {
            name: name.to_owned(),
            kind: kind.to_owned(),
        },
        serial: serial.map(str::to_owned),
        health,
        values: Values::default(),
    }
}

/// The owner's scan order: the four disks directly, then through the RAID card.
fn owners_readings() -> Vec<Reading> {
    vec![
        reading("/dev/sda", "sat", "owner/sda.json"),
        reading("/dev/sdb", "sat", "owner/sdb.json"),
        reading("/dev/sdc", "sat", "owner/sdc.json"),
        reading("/dev/sdd", "sat", "owner/sdd.json"),
        reading("/dev/bus/0", "sat+megaraid,18", "owner/megaraid-18.json"),
        reading("/dev/bus/0", "sat+megaraid,21", "owner/megaraid-21.json"),
        reading("/dev/bus/0", "sat+megaraid,23", "owner/megaraid-23.json"),
        reading("/dev/bus/0", "sat+megaraid,24", "owner/megaraid-24.json"),
    ]
}

fn table_rows(record: &Record) -> Vec<Vec<String>> {
    let Record::Table(table) = record else {
        panic!("the table record is a table: {record:?}");
    };
    table.rows().to_vec()
}

fn root() -> Root {
    Root::new("/nonexistent")
}

#[test]
fn the_owners_server_shows_four_disks_with_their_values() {
    let rows = report::rows(owners_readings(), &root());
    let record = report::table(&rows).expect("every row has all columns");
    let Record::Table(table) = &record else {
        panic!("a table: {record:?}");
    };
    assert_eq!(table.title(), "硬盘");
    assert_eq!(
        table.headers(),
        [
            "设备",
            "健康",
            "通电时间",
            "温度",
            "已用寿命",
            "累计写入",
            "坏块",
            "接口错误"
        ]
    );
    assert_eq!(
        table.rows(),
        [
            [
                "sda",
                "通过",
                "46167 小时",
                "25 °C",
                "23%",
                "48.2 TB",
                "0 / —",
                "3"
            ],
            [
                "sdb",
                "通过",
                "38112 小时",
                "28 °C",
                "49%",
                "169.0 TB",
                "380 / —",
                "0"
            ],
            [
                "sdc",
                "通过",
                "53544 小时",
                "27 °C",
                "79%",
                "277.2 TB",
                "0 / —",
                "0"
            ],
            [
                "sdd",
                "通过",
                "35124 小时",
                "27 °C",
                "30%",
                "155.6 TB",
                "13 / —",
                "0"
            ],
        ],
        "the RAID card's four paths merge into sda to sdd"
    );
    assert_eq!(report::notes(&rows), [], "nothing to explain");
    assert_eq!(record.id(), "disks");
}

#[test]
fn a_named_device_is_kept_over_a_raid_card_path_whichever_comes_first() {
    let readings = vec![
        made_up("/dev/bus/0", "sat+megaraid,18", Some("A"), Health::Failed),
        made_up("/dev/sda", "sat", Some("A"), Health::Passed),
    ];
    let rows = report::rows(readings, &root());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].location, Location::Named("sda".to_owned()));
    assert_eq!(rows[0].reading.health, Health::Passed);
}

#[test]
fn among_raid_card_paths_the_first_in_scan_order_is_kept() {
    let readings = vec![
        made_up("/dev/bus/0", "megaraid,8", Some("A"), Health::Passed),
        made_up("/dev/bus/1", "megaraid,9", Some("A"), Health::Failed),
        made_up("/dev/sdb", "sat", Some("B"), Health::Passed),
        made_up("/dev/sda", "sat", Some("B"), Health::Failed),
    ];
    let rows = report::rows(readings, &root());
    let kept: Vec<String> = rows.iter().map(|row| row.location.name(false)).collect();
    assert_eq!(
        kept,
        ["sdb", "RAID 卡 8 号盘"],
        "two named devices: the first stays"
    );
}

#[test]
fn readings_without_a_serial_are_never_merged() {
    let failed = || Health::ReadFailed("x".to_owned());
    let readings = vec![
        made_up("/dev/sda", "sat", None, failed()),
        made_up("/dev/bus/0", "sat+megaraid,18", None, failed()),
    ];
    assert_eq!(report::rows(readings, &root()).len(), 2);
}

#[test]
fn raid_card_disks_name_their_card_only_when_there_are_several() {
    let readings = vec![
        made_up("/dev/bus/0", "megaraid,8", Some("A"), Health::Passed),
        made_up("/dev/bus/0", "megaraid,9", Some("B"), Health::Passed),
    ];
    let rows = report::rows(readings, &root());
    let names = table_rows(&report::table(&rows).expect("a table"));
    assert_eq!(names[0][0], "RAID 卡 8 号盘");

    let readings = vec![
        made_up("/dev/bus/1", "megaraid,9", Some("A"), Health::Passed),
        made_up("/dev/bus/0", "megaraid,9", Some("B"), Health::Passed),
        made_up("/dev/sda", "sat", Some("C"), Health::Passed),
    ];
    let rows = report::rows(readings, &root());
    let names: Vec<String> = table_rows(&report::table(&rows).expect("a table"))
        .into_iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(
        names,
        ["sda", "RAID 卡 host0 的 9 号盘", "RAID 卡 host1 的 9 号盘"]
    );
}

#[test]
fn the_card_is_named_only_for_the_disks_left_after_merging() {
    // The second card's only disk is sdb, so one card remains.
    let readings = vec![
        made_up("/dev/sdb", "sat", Some("B"), Health::Passed),
        made_up("/dev/bus/0", "megaraid,8", Some("A"), Health::Passed),
        made_up("/dev/bus/1", "megaraid,9", Some("B"), Health::Passed),
    ];
    let rows = report::rows(readings, &root());
    let names: Vec<String> = rows.iter().map(|row| row.location.name(false)).collect();
    assert_eq!(names, ["sdb", "RAID 卡 8 号盘"]);
    let shown = table_rows(&report::table(&rows).expect("a table"));
    assert_eq!(shown[1][0], "RAID 卡 8 号盘");
}

#[test]
fn rows_that_need_it_get_a_note_after_the_table_in_row_order() {
    let readings = vec![
        reading("/dev/sde", "sat", "handwritten/smart-off.json"),
        made_up("/dev/sdd", "scsi", Some("D"), Health::Unsupported),
        reading("/dev/sdc", "sat", "handwritten/identify-failed.json"),
        reading("/dev/sda", "sat", "owner/sda.json"),
    ];
    let rows = report::rows(readings, &root());
    let cells = table_rows(&report::table(&rows).expect("a table"));
    assert_eq!(
        cells[1],
        ["sdc", "读取失败", "—", "—", "—", "—", "—", "—"],
        "a failed read shows nothing but its health"
    );
    assert_eq!(cells[2][1], "不支持");
    assert_eq!(cells[3][1], "未知");
    assert_eq!(
        report::notes(&rows),
        [
            Record::fact(
                "note-1",
                "说明",
                "sdc 读取失败：打不开设备，或读不到设备识别信息"
            ),
            Record::fact("note-2", "说明", "sdd 不支持 SMART"),
            Record::fact(
                "note-3",
                "说明",
                "sde 的 SMART 功能已关闭，本程序不会替它打开"
            ),
        ]
    );
}

#[test]
fn data_written_is_cut_to_one_decimal_in_decimal_units() {
    assert_eq!(report::written(0), "0.0 GB");
    assert_eq!(report::written(512_349_999_999), "512.3 GB");
    assert_eq!(
        report::written(999_999_999_999),
        "999.9 GB",
        "cut, never 1000.0 GB"
    );
    assert_eq!(report::written(1_000_000_000_000), "1.0 TB");
    assert_eq!(report::written(48_228_308_462_592), "48.2 TB");
    assert_eq!(report::written(21_699_999_744_000), "21.6 TB");
    assert_eq!(report::written(999_999_999_999_999), "999.9 TB");
    assert_eq!(report::written(1_000_000_000_000_000), "1.0 PB");
    assert_eq!(report::written(u64::MAX), "18446.7 PB");
}
