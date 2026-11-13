//! Chinese terminal output: section layout, display-width alignment, outcomes, tools and
//! the summary line.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bench::orchestrator::Fault;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::provision::Provisioned;
use bench::results::Measured;
use bench::results::Report;
use bench::terminal;
use bench::terminal::Terminal;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::Net;
use bench_api::Record;
use bench_api::Table;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed::Stop;
use bench_host::glibc::GlibcVersion;

/// A run of the test item 磁盘（fio）.
fn disk(status: Status, elapsed: Option<Duration>, records: Option<Vec<Record>>) -> Run<Measured> {
    Run {
        id: "disk.fio",
        group: "disk",
        title: Some("磁盘（fio）"),
        started: elapsed.map(|_| Duration::ZERO),
        elapsed,
        status,
        result: records.map(|records| {
            Measured::Legacy(Report {
                output: Some(records),
                stop: None,
            })
        }),
    }
}

fn text(terminal: Terminal<Vec<u8>>) -> String {
    String::from_utf8(terminal.into_inner()).expect("the terminal writes UTF-8")
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

#[test]
fn records_line_up_by_display_width() {
    let rows = vec![
        vec!["4k".into(), "171.01 MB/s".into()],
        vec!["1m".into(), "980 MB/s".into()],
    ];
    let table =
        Table::try_new("rw", "随机读写", vec!["块大小".into(), "读取".into()], rows).unwrap();
    let records = vec![
        Record::fact("version", "版本", "fio-3.39"),
        Record::metric(
            "read-4k",
            "4K 随机读",
            Measurement::try_new(171.01, "MB/s").unwrap(),
        ),
        Record::metric("node-2", "节点 2", Measurement::new_missing(Missing::Busy)),
        Record::Table(table),
    ];
    let mut terminal = Terminal::new(Vec::new());
    terminal.run(&disk(
        Status::Completed,
        Some(Duration::from_millis(1_500)),
        Some(records),
    ));

    let output = text(terminal);
    let lines: Vec<&str> = output.lines().collect();
    // Labels pad to 16 columns: 版本 is 4 wide, "4K 随机读" 9, "节点 2" 6.
    let expected = [
        String::new(),
        "【磁盘（fio）】".to_owned(),
        format!("  版本{}  fio-3.39", spaces(12)),
        format!("  4K 随机读{}  171.01 MB/s", spaces(7)),
        format!("  节点 2{}  繁忙", spaces(10)),
        "  随机读写".to_owned(),
        "  块大小  读取".to_owned(),
        format!("  4k{}  171.01 MB/s", spaces(4)),
        format!("  1m{}  980 MB/s", spaces(4)),
    ];
    assert_eq!(lines[..expected.len()], expected);
    assert_eq!(lines[expected.len()], "  完成，用时 1.5 秒", "{output}");
}

#[test]
fn a_plugin_that_never_started_still_gets_its_section() {
    let unmet = Unmet::Tool {
        name: "fio",
        why: "HTTP 404".to_owned(),
    };
    let mut terminal = Terminal::new(Vec::new());
    terminal.run(&disk(Status::Skipped(Some(unmet)), None, None));

    assert_eq!(
        text(terminal),
        "\n【磁盘（fio）】\n  跳过：工具 fio 不可用\n"
    );
}

#[test]
fn tool_section_shows_each_tool_with_its_result() {
    let fio = ToolSpec {
        name: "fio",
        version: "3.39",
        form: ToolForm::Executable,
        assets: &[],
    };
    let iperf3 = ToolSpec {
        name: "iperf3",
        version: "3.19",
        form: ToolForm::Executable,
        assets: &[],
    };
    let provisioned = Provisioned {
        ready: BTreeMap::from([("fio".to_owned(), PathBuf::from("/run/tools/fio"))]),
        failed: BTreeMap::from([("iperf3".to_owned(), "HTTP 404".to_owned())]),
    };
    let mut terminal = Terminal::new(Vec::new());
    terminal.tools(&[fio, iperf3], &provisioned);

    let expected = format!(
        "\n【准备工具】\n  fio 3.39{}  就绪\n  iperf3 3.19{}  失败：HTTP 404\n",
        spaces(8),
        spaces(5)
    );
    assert_eq!(text(terminal), expected);
}

#[test]
fn summary_counts_every_outcome() {
    let run = |status| disk(status, None, None);
    let runs = [
        run(Status::Completed),
        run(Status::Skipped(None)),
        run(Status::Failed(None)),
        run(Status::TimedOut),
        run(Status::Cancelled),
        run(Status::Completed),
    ];
    let mut terminal = Terminal::new(Vec::new());
    terminal.summary(&runs, Duration::from_millis(12_340));

    assert_eq!(
        text(terminal),
        "\n共 6 项：完成 2，跳过 1，失败 1，超时 1，中断 1。总用时 12.3 秒\n"
    );
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
        assert_eq!(terminal::unmet_text(&unmet), text);
    }
    assert_eq!(
        terminal::fault_text(&Fault::Panic("boom".into())),
        "插件崩溃：boom"
    );
    assert_eq!(
        terminal::fault_text(&Fault::WorkDir("No space left on device".into())),
        "无法创建工作目录：No space left on device"
    );
    assert_eq!(
        terminal::fault_text(&Fault::Aborted("task was cancelled".into())),
        "任务异常结束：task was cancelled"
    );
}

#[test]
fn a_stop_shows_the_plugins_own_reason() {
    let mut run = disk(Status::Failed(None), Some(Duration::from_secs(2)), None);
    run.result = Some(Measured::Legacy(Report {
        output: Some(Vec::new()),
        stop: Some(Stop::Failed("fio 报告错误 28".to_owned())),
    }));
    let mut terminal = Terminal::new(Vec::new());
    terminal.run(&run);
    assert_eq!(
        text(terminal),
        "\n【磁盘（fio）】\n  失败：fio 报告错误 28\n"
    );
}
