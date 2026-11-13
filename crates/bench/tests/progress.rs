//! The progress log on stderr: one line per event after the time since the start, the tools'
//! downloads, each plugin's start and end in the report's words, the debug events of `-v`, and
//! nothing from the libraries.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bench::orchestrator::Observer;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::progress;
use bench::progress::Log;
use bench::provision;
use bench::provision::Provisioned;
use bench::results::Measured;
use bench_api::Net;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use common::Captured;

mod common;

/// Runs `events` with the program's subscriber writing into memory, and returns the lines.
fn logged(verbose: bool, events: impl FnOnce()) -> Vec<String> {
    let (captured, guard) = Captured::start(verbose);
    events();
    drop(guard);
    captured.lines()
}

const fn tool(name: &'static str, version: &'static str) -> ToolSpec {
    ToolSpec {
        name,
        version,
        form: ToolForm::Executable,
        assets: &[ToolAsset {
            arch: "x86_64",
            url: "https://tools.example/download",
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            size: 1,
        }],
    }
}

#[test]
fn the_ready_tools_share_a_line_and_each_failure_has_its_own() {
    let tools = [
        tool("sysbench", "1.0.20"),
        tool("fio", "3.43"),
        tool("stream", "5.10"),
    ];
    let provisioned = Provisioned {
        ready: BTreeMap::from([
            ("sysbench".to_owned(), PathBuf::from("/run/tools/sysbench")),
            ("stream".to_owned(), PathBuf::from("/run/tools/stream")),
        ]),
        failed: BTreeMap::from([("fio".to_owned(), "下载失败：HTTP 404".to_owned())]),
    };
    let lines = logged(false, || progress::tools(&tools, &provisioned));
    assert_eq!(
        lines,
        [
            "准备工具：sysbench 1.0.20、stream 5.10 就绪",
            "准备工具：fio 3.43 失败：下载失败：HTTP 404",
        ]
    );
    let none_ready = Provisioned {
        ready: BTreeMap::new(),
        failed: BTreeMap::from([("fio".to_owned(), "下载失败：HTTP 404".to_owned())]),
    };
    let lines = logged(false, || progress::tools(&tools[1..2], &none_ready));
    assert_eq!(lines, ["准备工具：fio 3.43 失败：下载失败：HTTP 404"]);
}

fn finished(id: &'static str, status: Status, seconds: Option<f64>) -> Run<Measured> {
    Run {
        id,
        group: "memory",
        title: None,
        started: seconds.map(|_| Duration::ZERO),
        elapsed: seconds.map(Duration::from_secs_f64),
        status,
        result: None,
    }
}

#[test]
fn plugins_are_logged_by_id_in_the_reports_words() {
    let lines = logged(false, || {
        let mut log = Log;
        log.on_start("memory.stream");
        log.on_progress("memory.stream", "单线程测试");
        log.on_finish(&finished("memory.stream", Status::Completed, Some(21.3)));
        log.on_finish(&finished(
            "cpu.geekbench",
            Status::Skipped(Some(Unmet::Network(Net::Ipv4))),
            None,
        ));
    });
    assert_eq!(
        lines,
        [
            "memory.stream 开始",
            "memory.stream 单线程测试",
            "memory.stream 完成，用时 21.3 秒",
            "cpu.geekbench 跳过：需要 IPv4 网络连接",
        ]
    );
}

/// Provisions one tool whose download fails, and lets two libraries log meanwhile.
fn download() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime for the test");
    let dir = tempfile::tempdir().expect("a directory for the tools");
    let provisioned = runtime.block_on(provision::provision_all(
        &[tool("fio", "3.43")],
        "x86_64",
        dir.path(),
        |_url| async { Err::<&[u8], String>("下载失败：离线".to_owned()) },
    ));
    assert_eq!(provisioned.failed.len(), 1);
    tracing::info!(target: "hyper_util::client::legacy::connect::http", "connecting to 1.1.1.1");
    tracing::debug!(target: "reqwest::connect", "starting new connection");
}

#[test]
fn verbose_adds_each_download_and_the_libraries_stay_silent() {
    let result = "准备工具：fio 3.43 失败：下载失败：离线";
    assert_eq!(logged(false, download), [result]);
    assert_eq!(
        logged(true, download),
        ["下载 https://tools.example/download", result]
    );
}
