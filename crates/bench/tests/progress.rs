//! The progress log on stderr: one line per event after the time since the start, the tools'
//! downloads, each plugin's start and end in the report's words, the debug events of `-v` with
//! each tool's run, and nothing from the libraries.

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
use bench_api::ToolEnd;
use bench_api::ToolForm;
use bench_api::ToolRun;
use bench_api::ToolSpec;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::HostProfile;
use common::Captured;

mod common;

/// Runs `events` with the program's subscriber writing into memory, and returns the lines.
fn logged(verbose: bool, events: impl FnOnce()) -> Vec<String> {
    let (captured, capturing) = Captured::start(verbose);
    events();
    drop(capturing);
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

#[test]
fn tool_runs_are_logged_with_v_only() {
    let run = ToolRun {
        command: ["sysbench", "cpu", "--threads=1"]
            .map(str::to_owned)
            .to_vec(),
        end: ToolEnd::Exit(1),
        tail: vec!["FATAL: bad option".to_owned(), "second".to_owned()],
    };
    assert!(logged(false, || Log.on_tool("cpu.sysbench", &run)).is_empty());
    assert_eq!(
        logged(true, || Log.on_tool("cpu.sysbench", &run)),
        [
            "cpu.sysbench 运行 sysbench cpu --threads=1：退出码 1",
            "cpu.sysbench 输出：FATAL: bad option",
            "cpu.sysbench 输出：second",
        ]
    );
    let ends = [
        (ToolEnd::Exit(0), "退出码 0"),
        (ToolEnd::Signal, "被信号终止"),
        (
            ToolEnd::TimedOut(Duration::from_secs(60)),
            "超过 60 秒仍未结束，已停止",
        ),
        (ToolEnd::Cancelled, "已中断"),
        (
            ToolEnd::Spawn("No such file or directory (os error 2)".to_owned()),
            "无法运行：No such file or directory (os error 2)",
        ),
        (
            ToolEnd::Read("Broken pipe (os error 32)".to_owned()),
            "无法读取输出：Broken pipe (os error 32)",
        ),
    ];
    for (end, words) in ends {
        let run = ToolRun {
            command: vec!["fio".to_owned()],
            end,
            tail: Vec::new(),
        };
        assert_eq!(
            logged(true, || Log.on_tool("disk.fio", &run)),
            [format!("disk.fio 运行 fio：{words}")]
        );
    }
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

fn found(target: &str, address: &str, source: &'static str) -> Family {
    Family {
        targets: vec![(target.parse().expect("a target"), Reach::Connected)],
        address: Address::Found {
            address: address.parse().expect("an address"),
            source,
        },
    }
}

#[test]
fn the_host_and_its_network_are_logged_with_masked_addresses() {
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        arch: "x86_64".to_owned(),
        network: Network {
            ipv4: found("1.1.1.1:443", "203.0.113.7", "Cloudflare"),
            ipv6: found("[2606:4700:4700::1111]:443", "2001:db8::7", "ipify"),
        },
        ..HostProfile::default()
    };
    let lines = logged(true, || {
        progress::host(&host);
        progress::network(&host.network);
    });
    assert_eq!(
        lines,
        [
            "主机概况：KVM 虚拟机，x86_64",
            "网络：IPv4 可用；公网地址 203.0.*.*（来自 Cloudflare）",
            "网络：IPv6 可用；公网地址 2001:db8:*（来自 ipify）",
        ]
    );
}
