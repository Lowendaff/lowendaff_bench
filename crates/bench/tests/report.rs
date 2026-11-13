//! The terminal report, from constructed runs: sections by group in the report's order, each
//! status, numbers aligned right, wrapping to a terminal's width, and the summary. Whole reports
//! are compared with snapshots in `tests/snapshots/`.

use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Fault;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::Net;
use bench_api::Record;
use bench_api::Table;
use bench_api::typed::Stop;
use bench_host::cpu::Cpu;
use bench_host::cpu::Virtualization;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::MemoryFacts;
use bench_host::profile::Read;
use bench_host::profile::Seen;
use bench_host::profile::StorageFacts;
use bench_host::profile::SystemFacts;
use bench_host::storage::Disk;
use bench_host::storage::Filesystem;
use bench_host::storage::Space;
use bench_host::storage::Use;

/// A small virtual machine, as the report starts with it: its IPv4 address is a documentation
/// address, and it has no IPv6.
fn host() -> HostProfile {
    let cpu = Cpu {
        model: Some("Example CPU @ 2.00GHz".to_owned()),
        logical: Some(2),
        mhz: Some(2000.0),
        caches: vec![("L1d".to_owned(), 32 << 10), ("L2".to_owned(), 4 << 20)],
        aes: Some(true),
        virtualization: Virtualization::Absent,
        ..Cpu::default()
    };
    let memory = MemoryFacts {
        total: 2 << 30,
        used: 1 << 30,
        estimated: false,
        swap_total: 0,
        swap_used: 0,
    };
    let disk = Disk {
        name: "vda".to_owned(),
        interface: "virtio",
        size: 40 << 30,
        rotational: None,
        model: None,
        uses: vec![Use::Mount("/".to_owned())],
    };
    let root = Filesystem {
        mount_point: "/".to_owned(),
        fs_type: "ext4".to_owned(),
        devices: vec![(254, 1)],
        zfs_pool: None,
        disks: vec!["vda".to_owned()],
        space: Some(Space {
            total: 39 << 30,
            used: 10 << 30,
            available: 27 << 30,
        }),
    };
    let system = SystemFacts {
        distribution: Some("Debian GNU/Linux 13 (trixie)".to_owned()),
        kernel: Some("6.12.73+deb13-amd64".to_owned()),
        uptime: Some(Seen {
            value: Duration::from_secs(3 * 86_400 + 4 * 3600),
            host: false,
        }),
        load: Some(Seen {
            value: [0.1, 0.2, 0.3],
            host: false,
        }),
        congestion_control: Some("bbr".to_owned()),
        qdisc: Some("fq".to_owned()),
    };
    let target = |text: &str| text.parse().expect("a target");
    HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        arch: "x86_64".to_owned(),
        libc: Libc::Glibc(GlibcVersion {
            major: 2,
            minor: 41,
        }),
        cpu: Read::Done(CpuFacts {
            cpu,
            usable: Some(2),
            quota: None,
        }),
        memory: Read::Done(Some(memory)),
        storage: Read::Done(StorageFacts {
            disks: vec![disk],
            filesystems: vec![root],
            raid: None,
            work_dir: Some((254, 1)),
        }),
        system: Read::Done(system),
        hardware: None,
        network: Network {
            ipv4: Family {
                targets: vec![(target("1.1.1.1:443"), Reach::Connected)],
                address: Address::Found {
                    address: "203.0.113.7".parse().expect("an address"),
                    source: "Cloudflare",
                },
            },
            ipv6: Family {
                targets: vec![
                    (target("[2606:4700:4700::1111]:443"), Reach::NoRoute),
                    (target("[2400:3200::1]:443"), Reach::NoRoute),
                ],
                address: Address::NotAsked,
            },
        },
    }
}

/// A run of a plugin of the first contract.
fn run(
    id: &'static str,
    title: &'static str,
    status: Status,
    seconds: Option<f64>,
    records: Option<Vec<Record>>,
    stop: Option<Stop<String>>,
) -> Run<Measured> {
    Run {
        id,
        group: orchestrator::group_of(id),
        title: Some(title),
        started: seconds.map(|_| Duration::ZERO),
        elapsed: seconds.map(Duration::from_secs_f64),
        status,
        result: records.map(|records| {
            Measured::Legacy(Report {
                output: Some(records),
                stop,
            })
        }),
    }
}

fn table(id: &str, title: &str, headers: &[&str], rows: &[&[&str]]) -> Record {
    let headers = headers.iter().map(|cell| (*cell).to_owned()).collect();
    let rows = rows
        .iter()
        .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
        .collect();
    Record::Table(Table::try_new(id, title, headers, rows).expect("rows as wide as the headers"))
}

/// Every status, in the run order, which differs from the report's; the two plugins of a group
/// that the report does not know come last, in a section of their own.
fn runs() -> Vec<Run<Measured>> {
    vec![
        run(
            "disk.health",
            "硬盘健康",
            Status::TimedOut,
            Some(60.0),
            Some(vec![table(
                "disks",
                "硬盘",
                &["设备", "健康", "温度"],
                &[&["sda", "通过", "25 °C"]],
            )]),
            None,
        ),
        run(
            "cpu.sysbench",
            "CPU 性能",
            Status::Completed,
            Some(41.2),
            Some(vec![
                Record::fact("sysbench", "sysbench", "1.0.20：素数上限 10000，每项 20 秒"),
                table(
                    "results",
                    "结果",
                    &["项目", "线程数", "每秒事件数"],
                    &[&["单线程", "1", "859.00"], &["多线程", "64", "44631.33"]],
                ),
            ]),
            None,
        ),
        run(
            "memory",
            "内存性能",
            Status::Failed(None),
            Some(36.3),
            Some(vec![Record::metric(
                "latency",
                "内存延迟",
                Measurement::new_missing(Missing::Error),
            )]),
            Some(Stop::Failed("STREAM 出错".to_owned())),
        ),
        run(
            "disk.fio",
            "硬盘性能",
            Status::Skipped(Some(Unmet::Tool {
                name: "fio",
                why: "下载失败：HTTP 404".to_owned(),
            })),
            None,
            None,
            None,
        ),
        run(
            "cpu.geekbench",
            "Geekbench 7",
            Status::Cancelled,
            None,
            None,
            None,
        ),
        run(
            "test.own-skip",
            "自行跳过的测试",
            Status::Skipped(None),
            Some(0.1),
            Some(Vec::new()),
            Some(Stop::Skipped("测试目录在内存盘上".to_owned())),
        ),
        run(
            "test.panic",
            "崩溃的测试",
            Status::Failed(Some(Fault::Panic("index out of bounds".to_owned()))),
            Some(0.2),
            None,
            None,
        ),
    ]
}

#[test]
fn every_status_in_its_group() {
    let runs = runs();
    let page = Page {
        version: "0.1.0",
        host: &host(),
        show_ip: false,
        not_applicable: &["示例独服测试"],
        runs: &runs,
        total: Duration::from_secs_f64(137.8),
    };
    insta::assert_snapshot!(report::terminal::render(&page, None));
}

#[test]
fn a_run_with_one_plugin_per_section() {
    let runs = vec![
        run(
            "disk.fio",
            "硬盘性能",
            Status::Completed,
            Some(0.2),
            Some(vec![Record::fact("dir", "测试目录", "/root（ext4）")]),
            None,
        ),
        run(
            "cpu.geekbench",
            "Geekbench 7",
            Status::Skipped(Some(Unmet::Network(Net::Ipv4))),
            None,
            None,
            None,
        ),
    ];
    let page = Page {
        version: "0.1.0",
        host: &host(),
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_secs(1),
    };
    insta::assert_snapshot!(report::terminal::render(&page, None));
}

#[test]
fn tables_wrap_to_a_terminals_width_only() {
    let long = "很长的说明文字，在窄终端上需要换行才能看全，写到文件里则保持一行";
    let runs = vec![run(
        "disk.fio",
        "硬盘性能",
        Status::Completed,
        Some(1.0),
        Some(vec![table(
            "notes",
            "说明",
            &["项目", "内容"],
            &[&["4K", long]],
        )]),
        None,
    )];
    let page = Page {
        version: "0.1.0",
        host: &host(),
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_secs(1),
    };
    let unwrapped = report::terminal::render(&page, None);
    assert!(
        unwrapped.lines().any(|line| line.ends_with(long)),
        "without a terminal a row stays on one line: {unwrapped}"
    );
    let narrow = report::terminal::render(&page, Some(40));
    assert!(
        !narrow.lines().any(|line| line.contains(long)),
        "on a 40-column terminal the cell wraps: {narrow}"
    );
    insta::assert_snapshot!(narrow);
}

#[test]
fn a_table_without_rows_has_no_column_of_numbers() {
    // Its headers stay on the left, which shows once they wrap.
    let runs = vec![run(
        "disk.fio",
        "硬盘性能",
        Status::Completed,
        Some(1.0),
        Some(vec![table("empty", "空表", &["项目说明 甲乙", "x"], &[])]),
        None,
    )];
    let page = Page {
        version: "0.1.0",
        // Without the host's tables, which a 14-column terminal would break up too.
        host: &HostProfile {
            storage: Read::Done(StorageFacts::default()),
            ..host()
        },
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_secs(1),
    };
    insta::assert_snapshot!(report::terminal::render(&page, Some(14)));
}

#[test]
fn requirements_and_faults_are_written_in_chinese() {
    let needed = GlibcVersion {
        major: 2,
        minor: 35,
    };
    let found = Some(GlibcVersion {
        major: 2,
        minor: 34,
    });
    let texts = [
        (
            Unmet::Glibc { needed, found },
            "需要 glibc 2.35 或更新，本机是 2.34",
        ),
        (
            Unmet::Glibc {
                needed,
                found: None,
            },
            "需要 glibc 2.35 或更新，本机没有 glibc",
        ),
        (Unmet::Network(Net::Ipv4), "需要 IPv4 网络连接"),
        (Unmet::Network(Net::Ipv6), "需要 IPv6 网络连接"),
        (Unmet::Network(Net::Any), "需要网络连接"),
        (
            Unmet::Tool {
                name: "fio",
                why: "下载失败：HTTP 404".to_owned(),
            },
            "工具 fio 不可用：下载失败：HTTP 404",
        ),
        (
            Unmet::Tool {
                name: "fio",
                why: String::new(),
            },
            "工具 fio 不可用",
        ),
    ];
    for (unmet, text) in texts {
        assert_eq!(report::terminal::unmet_text(&unmet), text);
    }
    assert_eq!(
        report::terminal::fault_text(&Fault::Panic("boom".into())),
        "插件崩溃：boom"
    );
    assert_eq!(
        report::terminal::fault_text(&Fault::WorkDir("No space left on device".into())),
        "无法创建工作目录：No space left on device"
    );
    assert_eq!(
        report::terminal::fault_text(&Fault::Aborted("task was cancelled".into())),
        "任务异常结束：task was cancelled"
    );
}
