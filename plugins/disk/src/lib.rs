//! The disk test items: 硬盘健康 (`disk.health`), the SMART health of a dedicated server's disks,
//! in [`health`]; and 硬盘性能 (`disk.fio`), fio running YABS's disk test on the filesystem of the
//! test directory, on every platform.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Requirements;
use bench_host::Root;
use bench_host::storage;
use bench_host::storage::FilesystemKind;
use bench_tool::FIO;
use serde::Deserialize;

use crate::fio::Outcome;
use crate::report::FileSize;

pub mod fio;
pub mod health;
pub mod report;

/// YABS stops each test after 35 seconds (`timeout 35`).
const TEST_LIMIT: Duration = Duration::from_secs(35);

/// 硬盘性能. It has no settings: every parameter is YABS's.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fio {}

impl Plugin for Fio {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.fio",
        title: "硬盘性能",
        version: "0.1.0",
        requires: Requirements {
            tools: &[FIO],
            ..Requirements::NONE
        },
        // Room to lay out 2 GiB on a slow disk, plus four tests of up to 35 seconds.
        timeout: Duration::from_secs(10 * 60),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let file = ctx.work_dir().join("test.fio");
        let result = test(ctx, &file).await;
        // Free the space for later items at once. The run directory is removed later anyway,
        // so a failure here does not matter.
        let _ = tokio::fs::remove_file(&file).await;
        result
    }
}

/// What the test needs to know about the filesystem of the test directory.
struct Host {
    kind: Option<FilesystemKind>,
    available: Option<u64>,
    /// ZFS's `spa_asize_inflation`, when the zfs module is loaded.
    inflation: Option<u64>,
}

impl Host {
    fn read(dir: &Path) -> Self {
        Self {
            kind: storage::filesystem_kind(dir),
            available: dir
                .to_str()
                .and_then(storage::statvfs)
                .map(|space| space.available),
            inflation: Root::system()
                .read_line("/sys/module/zfs/parameters/spa_asize_inflation")
                .and_then(|line| line.parse().ok()),
        }
    }
}

/// fio's exit status and its output, standard output first.
struct Run {
    success: bool,
    output: String,
}

async fn test(ctx: &Context, file: &Path) -> Result<(), PluginError> {
    let fio = ctx.tool("fio")?.to_path_buf();
    let size = FileSize::for_arch(&ctx.env().arch);
    let dir = ctx.work_dir().to_path_buf();
    let host = tokio::task::spawn_blocking(move || Host::read(&dir)).await?;
    let Some(available) = host.available else {
        return Err(PluginError::fail("无法读取测试目录的可用空间"));
    };
    if let Some(reason) = report::skip_reason(host.kind, available, size) {
        return Err(PluginError::skip(reason));
    }
    ctx.emit(report::method(size));
    if let Some(note) = report::zfs_note(host.kind, host.inflation, available) {
        ctx.emit(note);
    }

    let setup = run_fio(ctx, &fio, fio::setup_args(file, size.fio)).await?;
    if !setup.success {
        if fio::lacks_direct_io(&setup.output) {
            return Err(PluginError::skip("测试目录所在的文件系统不支持直接 I/O"));
        }
        let error = fio::error_line(&setup.output);
        return Err(PluginError::fail(format!("生成测试文件失败：{error}")));
    }

    let mut any_result = false;
    for pair in fio::BLOCK_SIZES {
        let mut outcomes = Vec::new();
        for block_size in pair {
            let args = fio::test_args(file, size.fio, block_size);
            let outcome = match tokio::time::timeout(TEST_LIMIT, run_fio(ctx, &fio, args)).await {
                Ok(run) => {
                    let run = run?;
                    fio::outcome(run.success, &run.output, &fio::job_name(block_size))
                }
                Err(_) => Outcome::TimedOut,
            };
            any_result |= matches!(outcome, Outcome::Done(_));
            outcomes.push(outcome);
        }
        let shown = [(pair[0], &outcomes[0]), (pair[1], &outcomes[1])];
        ctx.emit(report::table(shown)?);
        for note in report::notes(shown) {
            ctx.emit(note);
        }
    }
    if !any_result {
        return Err(PluginError::fail("四种块大小都没有结果"));
    }
    Ok(())
}

/// Runs fio to completion. Cancellation, or dropping the future at a timeout, kills it.
async fn run_fio(ctx: &Context, fio: &Path, args: Vec<OsString>) -> Result<Run, PluginError> {
    let output = tokio::process::Command::new(fio)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let cancel = ctx.cancel_token();
    tokio::select! {
        output = output => {
            let output = output?;
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Ok(Run { success: output.status.success(), output: text })
        }
        () = cancel.cancelled() => Err(PluginError::fail("已中断")),
    }
}
