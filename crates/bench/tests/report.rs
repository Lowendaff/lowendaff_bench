//! The terminal report, from constructed runs: sections by group in the report's order, each
//! status, numbers aligned right, wrapping to a terminal's width, and the summary. Whole reports
//! are compared with snapshots in `tests/snapshots/`; each group's words have tests of their own.

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
use bench_api::Net;
use bench_api::Stop;
use bench_host::cpu::Cpu;
use bench_host::cpu::Model;
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
use plugin_cpu::sysbench;
use plugin_cpu::sysbench::result::Rate;
use plugin_disk::fio;
use plugin_disk::fio::result::Outcome;
use plugin_disk::fio::result::Rates;
use plugin_disk::fio::result::Test;
use plugin_disk::health;
use plugin_disk::health::location::Location;
use plugin_disk::health::result::Disk as HealthDisk;
use plugin_disk::health::result::Health;
use plugin_disk::health::result::Values;
use plugin_memory::stream;

/// A small virtual machine, as the report starts with it: its IPv4 address is a documentation
/// address, and it has no IPv6.
fn host() -> HostProfile {
    let cpu = Cpu {
        model: Some(Model::Name("Example CPU @ 2.00GHz".to_owned())),
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
        numa: Read::default(),
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

/// A run that the reporter names as the program does.
fn run(
    id: &'static str,
    status: Status,
    seconds: Option<f64>,
    result: Option<Measured>,
) -> Run<Measured> {
    Run {
        id,
        group: orchestrator::group_of(id),
        title: report::terminal::name(id),
        started: seconds.map(|_| Duration::ZERO),
        elapsed: seconds.map(Duration::from_secs_f64),
        status,
        result,
    }
}

/// smartctl's reading of one disk that passed, with its temperature only.
fn healthy(name: &str) -> HealthDisk {
    HealthDisk {
        location: Location::Named(name.to_owned()),
        health: Health::Passed,
        values: Values {
            temperature: Some(25),
            ..Values::default()
        },
    }
}

fn health(disks: Vec<HealthDisk>) -> Measured {
    Measured::DiskHealth(Report {
        output: Some(health::Output {
            version: Some("7.5".to_owned()),
            disks,
            nodes_left: Vec::new(),
        }),
        stop: None,
    })
}

fn rate(value: f64) -> Rate {
    Rate {
        text: format!("{value:.2}"),
        value,
    }
}

fn sysbench_run(threads: usize, value: f64) -> sysbench::result::Run {
    sysbench::result::Run {
        threads,
        rate: rate(value),
        retries: Vec::new(),
    }
}

/// Every status, in the run order, which differs from the report's; the two plugins of a group
/// that the report does not know come last, in a section of their own.
fn runs() -> Vec<Run<Measured>> {
    vec![
        run(
            "disk.health",
            Status::TimedOut,
            Some(60.0),
            Some(health(vec![healthy("sda")])),
        ),
        run(
            "cpu.sysbench",
            Status::Completed,
            Some(41.2),
            Some(Measured::CpuSysbench(Report {
                output: Some(sysbench::Output {
                    version: Some("1.0.20".to_owned()),
                    single: sysbench_run(1, 859.0),
                    multi: Some(sysbench_run(64, 44631.33)),
                }),
                stop: None,
            })),
        ),
        run(
            "memory.stream",
            Status::Failed(None),
            Some(36.3),
            Some(Measured::MemoryStream(Report {
                output: None,
                stop: Some(Stop::Failed(stream::Failure::Run {
                    threads: 1,
                    faults: vec![stream::Fault::NoResult; 4],
                })),
            })),
        ),
        run(
            "disk.fio",
            Status::Skipped(Some(Unmet::Tool {
                name: "fio",
                why: "下载失败：HTTP 404".to_owned(),
            })),
            None,
            None,
        ),
        run("cpu.geekbench", Status::Cancelled, None, None),
        run(
            "test.own-skip",
            Status::Skipped(None),
            Some(0.1),
            Some(Measured::DiskFio(Report {
                output: None,
                stop: Some(Stop::Skipped(fio::Failure::InMemory)),
            })),
        ),
        run(
            "test.panic",
            Status::Failed(Some(Fault::Panic("index out of bounds".to_owned()))),
            Some(0.2),
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
        not_applicable: &["内存 NUMA"],
        runs: &runs,
        total: Duration::from_secs_f64(137.8),
    };
    insta::assert_snapshot!(report::terminal::render(&page, None));
}

#[test]
fn a_run_with_one_plugin_per_section() {
    let rates = Rates {
        read_kib: 95_045,
        read_iops: 23_761,
        write_kib: 95_294,
        write_iops: 23_823,
    };
    let fio = fio::Output {
        version: Some("3.43".to_owned()),
        size: 2 << 30,
        zfs_needed: None,
        tests: vec![Test {
            block_size: "4k",
            outcome: Outcome::Done(rates),
        }],
    };
    let runs = vec![
        run(
            "disk.fio",
            Status::Completed,
            Some(0.2),
            Some(Measured::DiskFio(Report {
                output: Some(fio),
                stop: None,
            })),
        ),
        run(
            "cpu.geekbench",
            Status::Skipped(Some(Unmet::Network(Net::Ipv4))),
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
    let long = "很长的设备名称，在窄终端上需要换行才能看全，写到文件里则保持一行";
    let runs = vec![run(
        "disk.health",
        Status::Completed,
        Some(1.0),
        Some(health(vec![healthy(long)])),
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
        unwrapped.lines().any(|line| line.contains(long)),
        "without a terminal a row stays on one line: {unwrapped}"
    );
    let narrow = report::terminal::render(&page, Some(80));
    assert!(
        !narrow.lines().any(|line| line.contains(long)),
        "on an 80-column terminal the cell wraps: {narrow}"
    );
    insta::assert_snapshot!(narrow);
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
