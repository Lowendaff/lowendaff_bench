//! The disk section of the report, from constructed runs: for 硬盘健康 the tool, the disks in a
//! table, the notes and each way the item ends. Compared with snapshots, and with the words that
//! the item's own tests asserted before the reporter wrote them.

use std::path::PathBuf;
use std::time::Duration;

use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::typed::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use plugin_disk::health;
use plugin_disk::health::location::Location;
use plugin_disk::health::result::BadBlocks;
use plugin_disk::health::result::Disk;
use plugin_disk::health::result::Health;
use plugin_disk::health::result::NodeLeft;
use plugin_disk::health::result::Values;

fn health_run(
    status: Status,
    output: Option<health::Output>,
    stop: Option<Stop<health::Failure>>,
) -> Run<Measured> {
    Run {
        id: "disk.health",
        group: "disk",
        title: Some("硬盘健康"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_millis(400)),
        status,
        result: Some(Measured::DiskHealth(Report { output, stop })),
    }
}

/// The report of `runs` from its disk section on, with the summary.
fn section(runs: &[Run<Measured>]) -> String {
    let host = HostProfile::default();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs,
        total: Duration::from_secs(5),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("【硬盘】").expect("a disk section");
    text[start..].to_owned()
}

fn named(name: &str) -> Location {
    Location::Named(name.to_owned())
}

fn ssd(name: &str, numbers: [u64; 4], written: u64, reallocated: u64, interface: u64) -> Disk {
    let [hours, temperature, endurance, _] = numbers;
    Disk {
        location: named(name),
        health: Health::Passed,
        values: Values {
            hours: Some(hours),
            temperature: Some(i64::try_from(temperature).expect("a temperature")),
            endurance_used: Some(endurance),
            written: Some(written),
            bad_blocks: BadBlocks::Sectors {
                reallocated: Some(reallocated),
                pending: None,
            },
            interface_errors: Some(interface),
        },
    }
}

/// The owner's four SATA SSDs, with the values that their fixtures hold.
fn owners_disks() -> Vec<Disk> {
    vec![
        ssd("sda", [46_167, 25, 23, 0], 48_228_308_462_592, 0, 3),
        ssd("sdb", [38_112, 28, 49, 0], 169_012_345_678_912, 380, 0),
        ssd("sdc", [53_544, 27, 79, 0], 277_298_765_432_100, 0, 0),
        ssd("sdd", [35_124, 27, 30, 0], 155_678_901_234_567, 13, 0),
    ]
}

fn output(disks: Vec<Disk>) -> health::Output {
    health::Output {
        version: Some("7.5".to_owned()),
        disks,
        nodes_left: Vec::new(),
    }
}

fn shows(text: &str, line: &str) -> bool {
    text.lines().any(|shown| shown == line)
}

#[test]
fn the_owners_server_shows_four_disks_with_their_values() {
    let run = health_run(Status::Completed, Some(output(owners_disks())), None);
    let text = section(&[run]);
    insta::assert_snapshot!(text);
    for line in [
        "  smartctl 7.5",
        "  设备  健康  通电时间    温度   已用寿命  累计写入  坏块     接口错误",
        "  sda   通过  46167 小时  25 °C  23%       48.2 TB   0 / —           3",
        "  sdb   通过  38112 小时  28 °C  49%       169.0 TB  380 / —         0",
        "  sdc   通过  53544 小时  27 °C  79%       277.2 TB  0 / —           0",
        "  sdd   通过  35124 小时  27 °C  30%       155.6 TB  13 / —          0",
        "  完成，用时 0.4 秒",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert!(!text.contains("说明"), "nothing to explain: {text}");
}

#[test]
fn disks_that_need_it_get_a_note_after_the_table_in_row_order() {
    let blank = |name: &str, health| Disk {
        location: named(name),
        health,
        values: Values::default(),
    };
    let disks = vec![
        ssd("sda", [46_167, 25, 23, 0], 48_228_308_462_592, 0, 3),
        blank("sdc", Health::ReadFailed(health::Fault::Unopened)),
        blank("sdd", Health::Unsupported),
        blank("sde", Health::Unknown { smart_off: true }),
        blank("sdf", Health::Unknown { smart_off: false }),
        blank("sdg", Health::Failed),
    ];
    let mut output = output(disks);
    output.nodes_left = vec![NodeLeft {
        path: PathBuf::from("/dev/megaraid_sas_ioctl_node"),
        error: "Is a directory (os error 21)".to_owned(),
    }];
    let text = section(&[health_run(Status::Completed, Some(output), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  sdc   读取失败  —           —      —         —         —      —",
        "  说明              sdc 读取失败：打不开设备，或读不到设备识别信息",
        "  说明              sdd 不支持 SMART",
        "  说明              sde 的 SMART 功能已关闭，本程序不会替它打开",
        "  说明              无法删除 /dev/megaraid_sas_ioctl_node：Is a directory (os error 21)",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    let cells: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("  sd"))
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert_eq!(
        cells,
        ["通过", "读取失败", "不支持", "未知", "未知", "未通过"],
        "the 健康 column"
    );
    assert_eq!(
        text.matches("说明").count(),
        4,
        "no note without a known reason: {text}"
    );
}

#[test]
fn each_reason_a_disk_cannot_be_read_has_its_words() {
    let note = |fault: health::Fault| {
        let disk = Disk {
            location: named("sdb"),
            health: Health::ReadFailed(fault),
            values: Values::default(),
        };
        let run = health_run(Status::Completed, Some(output(vec![disk])), None);
        section(&[run])
            .lines()
            .find_map(|line| line.strip_prefix("  说明              sdb 读取失败："))
            .expect("a note")
            .to_owned()
    };
    let cases = [
        (
            health::Fault::Tool(job::Fault::TimedOut(Duration::from_secs(60))),
            "超过 60 秒仍未读完，已停止",
        ),
        (
            health::Fault::Tool(job::Fault::Spawn("Text file busy (os error 26)".to_owned())),
            "无法运行 smartctl：Text file busy (os error 26)",
        ),
        (
            health::Fault::Tool(job::Fault::Read("Broken pipe (os error 32)".to_owned())),
            "无法读取 smartctl 的输出：Broken pipe (os error 32)",
        ),
        (health::Fault::Unparsable, "smartctl 的输出无法解析"),
        (
            health::Fault::Message(
                "Smartctl open device: /dev/sdz [SAT] failed: No such device".to_owned(),
            ),
            "Smartctl open device: /dev/sdz [SAT] failed: No such device",
        ),
        (health::Fault::Unopened, "打不开设备，或读不到设备识别信息"),
        (health::Fault::Refused, "smartctl 不接受命令参数"),
    ];
    for (fault, words) in cases {
        assert_eq!(note(fault), words);
    }
}

#[test]
fn raid_card_disks_name_their_card_only_when_there_are_several() {
    let disk = |location| Disk {
        location,
        health: Health::Passed,
        values: Values::default(),
    };
    let card = |host, numbers: &[u32]| Location::Card {
        host,
        numbers: numbers.to_vec(),
    };
    let names = |disks: Vec<Disk>| -> Vec<String> {
        let run = health_run(Status::Completed, Some(output(disks)), None);
        section(&[run])
            .lines()
            .filter(|line| line.ends_with("—") && line.contains("通过"))
            .map(|line| {
                line.trim()
                    .split("  ")
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    };
    assert_eq!(
        names(vec![disk(card(0, &[8])), disk(card(0, &[9]))]),
        ["RAID 卡 8 号盘", "RAID 卡 9 号盘"]
    );
    assert_eq!(
        names(vec![
            disk(named("sda")),
            disk(card(0, &[9])),
            disk(card(1, &[9]))
        ]),
        ["sda", "RAID 卡 host0 的 9 号盘", "RAID 卡 host1 的 9 号盘"]
    );
    assert_eq!(names(vec![disk(card(3, &[0, 5]))]), ["RAID 卡 0:5 号盘"]);
}

#[test]
fn the_values_print_as_the_item_printed_them() {
    let cell = |values: Values| {
        let disk = Disk {
            location: named("sdx"),
            health: Health::Passed,
            values,
        };
        let run = health_run(Status::Completed, Some(output(vec![disk])), None);
        let text = section(&[run]);
        let row = text
            .lines()
            .find(|line| line.starts_with("  sdx "))
            .expect("the disk's row")
            .to_owned();
        row.split("  ")
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
            .map(str::to_owned)
            .collect::<Vec<String>>()
    };
    let written = |bytes| {
        cell(Values {
            written: Some(bytes),
            ..Values::default()
        })[5]
            .clone()
    };
    for (bytes, shown) in [
        (0, "0.0 GB"),
        (512_349_999_999, "512.3 GB"),
        (999_999_999_999, "999.9 GB"),
        (1_000_000_000_000, "1.0 TB"),
        (48_228_308_462_592, "48.2 TB"),
        (21_699_999_744_000, "21.6 TB"),
        (999_999_999_999_999, "999.9 TB"),
        (1_000_000_000_000_000, "1.0 PB"),
        (u64::MAX, "18446.7 PB"),
    ] {
        assert_eq!(written(bytes), shown, "cut, never rounded: {bytes}");
    }
    let bad = |bad_blocks| {
        cell(Values {
            bad_blocks,
            ..Values::default()
        })[6]
            .clone()
    };
    let sectors = |reallocated, pending| BadBlocks::Sectors {
        reallocated,
        pending,
    };
    assert_eq!(bad(sectors(Some(0), None)), "0 / —");
    assert_eq!(bad(sectors(Some(8), Some(0))), "8 / 0");
    assert_eq!(bad(sectors(None, Some(2))), "— / 2");
    assert_eq!(bad(sectors(None, None)), "—");
    assert_eq!(bad(BadBlocks::Count(Some(5))), "5");
    assert_eq!(bad(BadBlocks::Count(None)), "—");
    let worn = cell(Values {
        hours: Some(28_761),
        temperature: Some(61),
        endurance_used: Some(104),
        written: Some(42_382_812 * 512_000),
        bad_blocks: BadBlocks::Count(Some(2)),
        interface_errors: None,
    });
    assert_eq!(
        worn,
        [
            "sdx",
            "通过",
            "28761 小时",
            "61 °C",
            "104%",
            "21.6 TB",
            "2",
            "—"
        ]
    );
}

#[test]
fn every_way_the_item_ends_shows_its_words() {
    let read_failed = |name: &str| Disk {
        location: named(name),
        health: Health::ReadFailed(health::Fault::Unopened),
        values: Values::default(),
    };
    let failed = |failure| health_run(Status::Failed(None), None, Some(Stop::Failed(failure)));
    let runs = [
        health_run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(health::Failure::NoDisks)),
        ),
        failed(health::Failure::Scan(health::Fault::Tool(
            job::Fault::TimedOut(Duration::from_secs(60)),
        ))),
        failed(health::Failure::Scan(health::Fault::Message(
            "Smartctl open device: /dev/sdz [SAT] failed: No such device".to_owned(),
        ))),
        failed(health::Failure::Scan(health::Fault::Unparsable)),
        health_run(
            Status::Failed(None),
            Some(output(vec![read_failed("sda"), read_failed("sdb")])),
            Some(Stop::Failed(health::Failure::AllFailed)),
        ),
        failed(health::Failure::Cancelled),
        health_run(Status::TimedOut, Some(output(owners_disks())), None),
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    跳过：smartctl 没有找到硬盘",
        "    失败：smartctl 扫描硬盘失败：超过 60 秒仍未完成",
        "    失败：smartctl 扫描硬盘失败：Smartctl open device: /dev/sdz [SAT] failed: \
         No such device",
        "    失败：smartctl 扫描硬盘失败：smartctl 的输出无法解析",
        "    失败：所有硬盘都读取失败",
        "    失败：已中断",
        "    超时，已保留超时前的结果",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}
