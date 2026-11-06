//! Tool provisioning: verification before execution, cleanup on failure, per-tool
//! failure reasons, and a plugin running the tool it was given.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench::orchestrator::Observer;
use bench::orchestrator::Outcome;
use bench::orchestrator::Settings;
use bench::orchestrator::plan;
use bench::orchestrator::run_plan;
use bench::provision::install;
use bench::provision::provision_all;
use bench::provision::required_tools;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Env;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolSpec;
use bench_api::register;
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
    assets: &[
        asset("x86_64", SCRIPT_SHA, SCRIPT_LEN),
        asset("aarch64", SCRIPT_SHA, SCRIPT_LEN),
    ],
};

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn install_makes_only_a_verified_file_executable() {
    let dir = tempfile::tempdir().unwrap();
    let path = install(SCRIPT, &FIO, &FIO.assets[0], dir.path())
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
    let err = install(SCRIPT, &FIO, &wrong, dir.path()).await.unwrap_err();

    assert!(err.starts_with("SHA-256 不符：应为 0000"), "{err}");
    assert!(entries(dir.path()).is_empty());
}

#[tokio::test]
async fn install_stops_reading_past_the_declared_size() {
    let dir = tempfile::tempdir().unwrap();
    let err = install(
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
    let err = install(
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
    let provisioned = provision_all(
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
    let descriptor = |tools: &'static [ToolSpec]| Descriptor {
        id: "x",
        title: "x",
        version: "0.1.0",
        requires: Requirements {
            tools,
            net: Net::None,
        },
        timeout: Duration::from_secs(1),
    };
    let tools = required_tools([
        descriptor(&[FIO, SYSBENCH]),
        descriptor(&[SYSBENCH, IPERF3_ARM_ONLY]),
    ]);

    assert_eq!(
        tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
        ["fio", "sysbench", "iperf3"]
    );
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunsFio {}

impl Plugin for RunsFio {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.fio",
        title: "磁盘（fio）",
        version: "0.1.0",
        requires: Requirements {
            tools: &[FIO],
            net: Net::None,
        },
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let output = tokio::process::Command::new(ctx.tool("fio")?)
            .kill_on_drop(true)
            .output()
            .await?;
        ctx.emit(Record::fact(
            "version",
            "版本",
            String::from_utf8_lossy(&output.stdout).trim(),
        ));
        Ok(())
    }
}

struct Quiet;

impl Observer for Quiet {}

#[tokio::test]
async fn a_plugin_runs_the_tool_it_was_provisioned_with() {
    let run_dir = tempfile::tempdir().unwrap();
    let registry = [register::<RunsFio>()];
    let planned = plan(&registry, &["disk.fio"], &BTreeMap::new()).unwrap();
    let tools = required_tools(planned.iter().map(|p| p.descriptor()));
    let provisioned = provision_all(&tools, std::env::consts::ARCH, run_dir.path(), |_| async {
        Ok::<_, String>(SCRIPT)
    })
    .await;
    let env = Env {
        tools: provisioned.ready,
        ..Env::default()
    };
    let settings = Settings {
        grace: Duration::from_secs(1),
        work_root: run_dir.path().to_path_buf(),
    };

    let runs = run_plan(
        planned,
        Arc::new(env),
        &CancellationToken::new(),
        &settings,
        &mut Quiet,
    )
    .await;

    assert_eq!(runs[0].outcome, Outcome::Completed);
    assert_eq!(
        runs[0].records,
        [Record::fact("version", "版本", "fio-3.39")]
    );
}
