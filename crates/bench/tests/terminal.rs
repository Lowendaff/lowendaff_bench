//! Chinese terminal output: section layout, display-width alignment, outcomes, tools and
//! the summary line.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bench::orchestrator::Observer;
use bench::orchestrator::Outcome;
use bench::orchestrator::Run;
use bench::provision::Provisioned;
use bench::terminal::Terminal;
use bench_api::Descriptor;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::Table;
use bench_api::ToolSpec;

const DISK: Descriptor = Descriptor {
    id: "disk.fio",
    title: "磁盘（fio）",
    version: "0.1.0",
    requires: Requirements::NONE,
    timeout: Duration::from_secs(60),
};

fn text(terminal: Terminal<Vec<u8>>) -> String {
    String::from_utf8(terminal.into_inner()).expect("the terminal writes UTF-8")
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

#[test]
fn records_line_up_by_display_width() {
    let mut terminal = Terminal::new(Vec::new());
    terminal.on_start(&DISK);
    terminal.on_record(&DISK, &Record::fact("version", "版本", "fio-3.39"));
    terminal.on_record(
        &DISK,
        &Record::metric(
            "read-4k",
            "4K 随机读",
            Measurement::try_new(171.01, "MB/s").unwrap(),
        ),
    );
    terminal.on_record(
        &DISK,
        &Record::metric("node-2", "节点 2", Measurement::new_missing(Missing::Busy)),
    );
    let rows = vec![
        vec!["4k".into(), "171.01 MB/s".into()],
        vec!["1m".into(), "980 MB/s".into()],
    ];
    let table =
        Table::try_new("rw", "随机读写", vec!["块大小".into(), "读取".into()], rows).unwrap();
    terminal.on_record(&DISK, &Record::Table(table));
    terminal.on_finish(&DISK, &Outcome::Completed);

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
    assert!(
        lines[expected.len()].starts_with("  完成，用时 "),
        "{output}"
    );
}

#[test]
fn a_plugin_that_never_started_still_gets_its_section() {
    let mut terminal = Terminal::new(Vec::new());
    terminal.on_finish(
        &DISK,
        &Outcome::Skipped {
            reason: "工具 fio 不可用".into(),
        },
    );

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
        assets: &[],
    };
    let iperf3 = ToolSpec {
        name: "iperf3",
        version: "3.19",
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
    let run = |outcome| Run {
        plugin: "x",
        outcome,
        elapsed: None,
        records: Vec::new(),
    };
    let runs = [
        run(Outcome::Completed),
        run(Outcome::Skipped {
            reason: String::new(),
        }),
        run(Outcome::Failed {
            error: String::new(),
        }),
        run(Outcome::TimedOut),
        run(Outcome::Cancelled),
        run(Outcome::Completed),
    ];
    let mut terminal = Terminal::new(Vec::new());
    terminal.summary(&runs, Duration::from_millis(12_340));

    assert_eq!(
        text(terminal),
        "\n共 6 项：完成 2，跳过 1，失败 1，超时 1，中断 1。总用时 12.3 秒\n"
    );
}
