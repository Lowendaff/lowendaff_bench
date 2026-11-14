//! Tool provisioning: verification before execution, cleanup on failure, per-tool
//! failure reasons, and a plugin running the tool it was given.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Quiet;
use bench::orchestrator::Settings;
use bench::orchestrator::Status;
use bench::provision;
use bench::registry;
use bench::results::Measured;
use bench::results::Report;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use plugin_disk::fio;
use serde::Deserialize;

const SCRIPT: &[u8] = b"#!/bin/sh\necho fio-3.39\n";
const SCRIPT_SHA: &str = "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8";
const SCRIPT_LEN: u64 = SCRIPT.len() as u64;
const FIO_URL: &str = "https://tools.example/fio";

const fn asset(arch: &'static str, sha256: &'static str, size: u64) -> ToolAsset {
    ToolAsset {
        arch,
        url: FIO_URL,
        sha256,
        size,
    }
}

const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.39",
    form: ToolForm::Executable,
    assets: &[
        asset("x86_64", SCRIPT_SHA, SCRIPT_LEN),
        asset("aarch64", SCRIPT_SHA, SCRIPT_LEN),
    ],
};

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read the test directory")
        .map(|entry| {
            entry
                .expect("read a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[tokio::test]
async fn install_makes_only_a_verified_file_executable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = provision::install(SCRIPT, &FIO, &FIO.assets[0], dir.path())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), SCRIPT);
    assert_ne!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
        0
    );
    assert_eq!(entries(dir.path()), ["fio"]);
}

#[tokio::test]
async fn install_rejects_a_hash_mismatch_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let wrong = asset(
        "x86_64",
        "0000000000000000000000000000000000000000000000000000000000000000",
        SCRIPT_LEN,
    );
    let err = provision::install(SCRIPT, &FIO, &wrong, dir.path())
        .await
        .unwrap_err();

    assert!(err.starts_with("SHA-256 不符：应为 0000"), "{err}");
    assert!(entries(dir.path()).is_empty());
}

#[tokio::test]
async fn install_stops_reading_past_the_declared_size() {
    let dir = tempfile::tempdir().unwrap();
    let err = provision::install(
        SCRIPT,
        &FIO,
        &asset("x86_64", SCRIPT_SHA, SCRIPT_LEN - 1),
        dir.path(),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "大小超过声明的 23 字节");
    assert!(entries(dir.path()).is_empty());
}

#[tokio::test]
async fn install_rejects_a_truncated_download() {
    let dir = tempfile::tempdir().unwrap();
    let err = provision::install(
        SCRIPT,
        &FIO,
        &asset("x86_64", SCRIPT_SHA, SCRIPT_LEN + 5),
        dir.path(),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "大小不符：应为 29 字节，实际 24 字节");
    assert!(entries(dir.path()).is_empty());
}

const IPERF3_ARM_ONLY: ToolSpec = ToolSpec {
    name: "iperf3",
    version: "3.19",
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "aarch64",
        url: "https://tools.example/iperf3",
        sha256: SCRIPT_SHA,
        size: SCRIPT_LEN,
    }],
};

const SYSBENCH: ToolSpec = ToolSpec {
    name: "sysbench",
    version: "1.0.20",
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/sysbench",
        sha256: SCRIPT_SHA,
        size: SCRIPT_LEN,
    }],
};

#[tokio::test]
async fn provision_all_reports_every_failure_with_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let fetch = |url: &'static str| async move {
        if url == FIO_URL {
            Ok(SCRIPT)
        } else {
            Err("HTTP 404".to_owned())
        }
    };
    let provisioned = provision::provision_all(
        &[FIO, IPERF3_ARM_ONLY, SYSBENCH],
        "x86_64",
        dir.path(),
        fetch,
    )
    .await;

    assert_eq!(provisioned.ready.keys().collect::<Vec<_>>(), ["fio"]);
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([
            ("iperf3".to_owned(), "没有适用于 x86_64 的版本".to_owned()),
            ("sysbench".to_owned(), "HTTP 404".to_owned()),
        ])
    );
}

#[test]
fn required_tools_are_unique_and_in_plan_order() {
    let requires = |tools: &'static [ToolSpec]| Requirements {
        tools,
        ..Requirements::NONE
    };
    let tools = provision::required_tools([
        requires(&[FIO, SYSBENCH]),
        requires(&[SYSBENCH, IPERF3_ARM_ONLY]),
    ]);

    assert_eq!(
        tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
        ["fio", "sysbench", "iperf3"]
    );
}

/// Runs the fio it was given, and returns what it printed as the version in a result of fio's
/// type.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunsFio {}

impl Plugin for RunsFio {
    type Output = fio::Output;
    type Failure = fio::Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.fio",
        requires: Requirements {
            tools: &[FIO],
            net: Net::None,
            platforms: &[],
            glibc: None,
            numa: false,
        },
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, ctx: &Context<fio::Output>) -> Result<fio::Output, Stop<fio::Failure>> {
        let program = ctx
            .tool("fio")
            .expect("the orchestrator runs a plugin only when its tools are ready");
        let output = tokio::process::Command::new(program)
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|_| Stop::Failed(fio::Failure::NoResult))?;
        Ok(printed(String::from_utf8_lossy(&output.stdout).trim()))
    }
}

/// A result of fio's type that holds only the version.
fn printed(version: &str) -> fio::Output {
    fio::Output {
        version: Some(version.to_owned()),
        size: 0,
        zfs_needed: None,
        tests: Vec::new(),
    }
}

#[tokio::test]
async fn a_plugin_runs_the_tool_it_was_provisioned_with() {
    let run_dir = tempfile::tempdir().unwrap();
    let registry = [registry::entry::<RunsFio>(Measured::DiskFio)];
    let planned = orchestrator::plan(&registry, &["disk.fio"], &BTreeMap::new()).unwrap();
    let tools = provision::required_tools(planned.iter().map(|planned| *planned.requires()));
    let provisioned =
        provision::provision_all(&tools, std::env::consts::ARCH, run_dir.path(), |_| async {
            Ok::<_, String>(SCRIPT)
        })
        .await;
    let settings = Settings {
        grace: Duration::from_secs(1),
        work_root: run_dir.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        tools: Arc::new(provisioned.ready),
        failed_tools: BTreeMap::new(),
        host: Arc::default(),
    };

    let runs =
        orchestrator::run_plan(planned, &CancellationToken::new(), &settings, &mut Quiet).await;

    assert_eq!(runs[0].status, Status::Completed);
    assert_eq!(
        runs[0].result,
        Some(Measured::DiskFio(Report {
            output: Some(printed("fio-3.39")),
            stop: None,
        }))
    );
}
