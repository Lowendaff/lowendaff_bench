//! The progress log on stderr: one line per event after the time since the start, the tools'
//! downloads, each plugin's start and end in the report's words, the debug events of `-v` with
//! each tool's run, and nothing from the libraries.

use std::borrow::Cow;
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
use bench::provision::DownloadError;
use bench::provision::Part;
use bench::provision::Provisioned;
use bench::results::Measured;
use bench_api::CancellationToken;
use bench_api::Net;
use bench_api::Step;
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
        all_arches: false,
        form: ToolForm::Executable,
        raw_sockets: false,
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
        failed: BTreeMap::from([("fio".to_owned(), "download failed: HTTP 404".to_owned())]),
    };
    let lines = logged(false, || progress::tools(&tools, &provisioned));
    assert_eq!(
        lines,
        [
            "Tools: sysbench 1.0.20, stream 5.10 ready",
            "Tools: fio 3.43 failed: download failed: HTTP 404",
        ]
    );
    let none_ready = Provisioned {
        ready: BTreeMap::new(),
        failed: BTreeMap::from([("fio".to_owned(), "download failed: HTTP 404".to_owned())]),
    };
    let lines = logged(false, || progress::tools(&tools[1..2], &none_ready));
    assert_eq!(lines, ["Tools: fio 3.43 failed: download failed: HTTP 404"]);
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
        log.on_progress("memory.stream", &Step::Test { threads: 1 });
        log.on_finish(&finished("memory.stream", Status::Completed, Some(21.3)));
        log.on_finish(&finished(
            "cpu.bench",
            Status::Skipped(Some(Unmet::Network(Net::Ipv4))),
            None,
        ));
    });
    assert_eq!(
        lines,
        [
            "memory.stream started",
            "memory.stream single-thread test",
            "memory.stream done in 21.3 s",
            "cpu.bench skipped: needs an IPv4 connection",
        ]
    );
}

#[test]
fn each_step_has_its_words() {
    let steps = [
        (Step::Test { threads: 1 }, "single-thread test"),
        (Step::Test { threads: 4 }, "multi-thread test (4 threads)"),
        (Step::Read { threads: 1 }, "single-thread read"),
        (Step::Read { threads: 4 }, "multi-thread read (4 threads)"),
        (Step::Write { threads: 1 }, "single-thread write"),
        (Step::Write { threads: 4 }, "multi-thread write (4 threads)"),
        (Step::TestFile, "creating the test file"),
        (
            Step::RandomReadWrite { block_size: "512k" },
            "512K random read and write test",
        ),
        (Step::Cache { level: 2 }, "L2 test"),
        (Step::MainMemory, "memory test"),
        (
            Step::NodeBandwidth { cpus: 0, memory: 1 },
            "bandwidth test of threads on node 0 and memory on node 1",
        ),
        (
            Step::NodeLatency { cpus: 1, memory: 0 },
            "latency test of threads on node 1 and memory on node 0",
        ),
        (Step::RunAgain { run: 2 }, "the upload failed; run 2"),
        (
            Step::Upload {
                location: Cow::Borrowed("Tokyo"),
                country: Cow::Borrowed("JP"),
                ipv6: false,
            },
            "upload to Tokyo, JP over IPv4",
        ),
        (
            Step::Download {
                location: Cow::Borrowed("Frankfurt"),
                country: Cow::Borrowed("DE"),
                ipv6: true,
            },
            "download from Frankfurt, DE over IPv6",
        ),
        (
            Step::Trace {
                target: Cow::Borrowed("Tokyo"),
                ipv6: false,
            },
            "trace to Tokyo over IPv4",
        ),
        (
            Step::Media {
                group: "global",
                ipv6: true,
            },
            "probing Global over IPv6",
        ),
        (
            Step::CpuSuite {
                suite: "7zip",
                stage: None,
                threads: Some(4),
            },
            "7-Zip: multi-thread test (4 threads)",
        ),
        (
            Step::CpuSuite {
                suite: "unixbench",
                stage: Some("dhry2reg"),
                threads: Some(1),
            },
            "UnixBench: dhry2reg (1 copy)",
        ),
    ];
    let lines = logged(false, || {
        let mut log = Log;
        for (step, _) in &steps {
            log.on_progress("test.item", step);
        }
    });
    let expected: Vec<String> = steps
        .iter()
        .map(|(_, words)| format!("test.item {words}"))
        .collect();
    assert_eq!(lines, expected);
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
            "cpu.sysbench ran sysbench cpu --threads=1: exit code 1",
            "cpu.sysbench output: FATAL: bad option",
            "cpu.sysbench output: second",
        ]
    );
    let ends = [
        (ToolEnd::Exit(0), "exit code 0"),
        (ToolEnd::Signal, "ended by a signal"),
        (
            ToolEnd::TimedOut(Duration::from_secs(60)),
            "still running after 60 s, stopped",
        ),
        (ToolEnd::Cancelled, "interrupted"),
        (
            ToolEnd::Spawn("No such file or directory (os error 2)".to_owned()),
            "cannot run: No such file or directory (os error 2)",
        ),
        (
            ToolEnd::Read("Broken pipe (os error 32)".to_owned()),
            "cannot read its output: Broken pipe (os error 32)",
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
            [format!("disk.fio ran fio: {words}")]
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
        1,
        &CancellationToken::new(),
        |_url, _range| async {
            Err::<Part<&[u8]>, DownloadError>(DownloadError::Final(
                "download failed: offline".to_owned(),
            ))
        },
    ));
    assert_eq!(provisioned.failed.len(), 1);
    tracing::info!(target: "hyper_util::client::legacy::connect::http", "connecting to 1.1.1.1");
    tracing::debug!(target: "reqwest::connect", "starting new connection");
}

#[test]
fn verbose_adds_each_download_and_the_libraries_stay_silent() {
    let result = "Tools: fio 3.43 failed: download failed: offline";
    assert_eq!(logged(false, download), [result]);
    assert_eq!(
        logged(true, download),
        ["Downloading https://tools.example/download", result]
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
            "Host: KVM virtual machine, x86_64",
            "Network: IPv4 available; public address 203.0.*.* (from Cloudflare)",
            "Network: IPv6 available; public address 2001:db8:* (from ipify)",
        ]
    );
}
