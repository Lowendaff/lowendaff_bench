//! The terminal report, from constructed runs: sections by group in the report's order, each
//! status, numbers aligned right, wrapping to a terminal's width, and the summary. Whole reports
//! are compared with snapshots in `tests/snapshots/`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Fault;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::provision::Provisioned;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::Net;
use bench_api::Record;
use bench_api::Table;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed::Stop;
use bench_host::glibc::GlibcVersion;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;

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

const SYSBENCH: ToolSpec = ToolSpec {
    name: "sysbench",
    version: "1.0.20",
    form: ToolForm::Executable,
    assets: &[],
};

const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.43",
    form: ToolForm::Executable,
    assets: &[],
};

/// One plugin per group and status, in the run order, which differs from the report's.
fn runs() -> Vec<Run<Measured>> {
    vec![
        run(
            "system.vm",
            "系统基础信息",
            Status::Completed,
            Some(0.31),
            Some(vec![
                Record::fact("os", "系统", "Debian GNU/Linux 13 (trixie)"),
                Record::fact("uptime", "运行时间", "3 天 4 小时（宿主机）"),
            ]),
            None,
        ),
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
    ]
}

fn page<'a>(runs: &'a [Run<Measured>], provisioned: &'a Provisioned) -> Page<'a> {
    Page {
        version: "0.1.0",
        platform: Platform::Vm(Hypervisor::Kvm),
        not_applicable: &["示例独服测试"],
        tools: Some((&[SYSBENCH, FIO], provisioned)),
        runs,
        total: Duration::from_secs_f64(137.8),
    }
}

fn provisioned() -> Provisioned {
    Provisioned {
        ready: BTreeMap::from([("sysbench".to_owned(), PathBuf::from("/run/tools/sysbench"))]),
        failed: BTreeMap::from([("fio".to_owned(), "下载失败：HTTP 404".to_owned())]),
    }
}

#[test]
fn every_status_in_its_group() {
    let runs = runs();
    let provisioned = provisioned();
    insta::assert_snapshot!(report::terminal::render(&page(&runs, &provisioned), None));
}

#[test]
fn a_run_with_one_plugin_per_section() {
    let runs = vec![
        run(
            "system.vm",
            "系统基础信息",
            Status::Completed,
            Some(0.2),
            Some(vec![Record::fact("os", "系统", "Alpine Linux v3.22")]),
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
        platform: Platform::Dedicated,
        not_applicable: &[],
        tools: None,
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
        platform: Platform::Dedicated,
        not_applicable: &[],
        tools: None,
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
                why: "HTTP 404".to_owned(),
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
