//! The CPU test items: CPU 性能, sysbench's CPU test with one thread and with every usable
//! thread, as the owner's `linux_bench` runs it, with retries; and Geekbench 7, in
//! [`geekbench`].

pub mod geekbench;
pub mod report;
pub mod sysbench;

use std::future::Future;
use std::num::NonZeroUsize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Requirements;
use bench_host::Root;
use bench_host::cpu;
use bench_tool::SYSBENCH;
use serde::Deserialize;

use crate::sysbench::AttemptError;
use crate::sysbench::Rate;
use crate::sysbench::RunError;
use crate::sysbench::Success;

/// An attempt runs for 20 seconds; the rest is room to start and to stop.
const ATTEMPT_LIMIT: Duration = Duration::from_secs(60);

/// CPU 性能. It has no settings: every parameter is `linux_bench`'s.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sysbench {}

impl Plugin for Sysbench {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "cpu.sysbench",
        title: "CPU 性能",
        version: "0.1.0",
        requires: Requirements {
            tools: &[SYSBENCH],
            ..Requirements::NONE
        },
        // Two runs of up to four attempts of up to 60 seconds each.
        timeout: Duration::from_secs(10 * 60),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let sysbench = ctx.tool("sysbench")?.to_path_buf();
        let kind = ctx.env().platform.kind();
        let counted =
            tokio::task::spawn_blocking(move || cpu::usable_threads(&Root::system(), kind)).await?;
        let threads = counted
            .or_else(|| {
                std::thread::available_parallelism()
                    .ok()
                    .map(NonZeroUsize::get)
            })
            .unwrap_or(1);
        measure(ctx, threads, |threads| {
            run_sysbench(ctx, &sysbench, threads)
        })
        .await
    }
}

/// Measures with one thread and, when there are more, with `threads`, and emits the records.
/// `attempt` runs sysbench once with the threads it is given.
pub async fn measure<F, Fut>(ctx: &Context, threads: usize, attempt: F) -> Result<(), PluginError>
where
    F: Fn(usize) -> Fut,
    Fut: Future<Output = Result<Rate, AttemptError>>,
{
    let single = match sysbench::retried(|| attempt(1)).await {
        Ok(single) => single,
        Err(RunError::Failed(failures)) => {
            return Err(PluginError::fail(report::failure(&failures)));
        }
        Err(RunError::Cancelled) => return Err(PluginError::fail("已中断")),
    };
    if threads <= 1 {
        ctx.emit(report::table(SYSBENCH.version, &single.rate, None)?);
        emit_retries(ctx, &single, None);
        return Ok(());
    }
    match sysbench::retried(|| attempt(threads)).await {
        Ok(multi) => {
            let shown = Some((threads, &multi.rate));
            ctx.emit(report::table(SYSBENCH.version, &single.rate, shown)?);
            ctx.emit(report::ratio(&single.rate, &multi.rate));
            emit_retries(ctx, &single, Some(&multi));
            Ok(())
        }
        Err(failed) => {
            ctx.emit(report::table(SYSBENCH.version, &single.rate, None)?);
            emit_retries(ctx, &single, None);
            Err(PluginError::fail(match failed {
                RunError::Failed(failures) => {
                    format!("多线程测试失败：{}", report::failure(&failures))
                }
                RunError::Cancelled => "已中断".to_owned(),
            }))
        }
    }
}

/// Notes on runs that succeeded only after retries.
fn emit_retries(ctx: &Context, single: &Success, multi: Option<&Success>) {
    if let Some(note) = report::retries("note-single", "单线程测试", &single.failures) {
        ctx.emit(note);
    }
    if let Some(note) =
        multi.and_then(|multi| report::retries("note-multi", "多线程测试", &multi.failures))
    {
        ctx.emit(note);
    }
}

/// One run of sysbench with `threads` threads, without a shell, killed on cancellation and
/// after [`ATTEMPT_LIMIT`].
async fn run_sysbench(
    ctx: &Context,
    sysbench: &Path,
    threads: usize,
) -> Result<Rate, AttemptError> {
    let output = tokio::process::Command::new(sysbench)
        .args(sysbench::args(threads))
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let cancel = ctx.cancel_token();
    tokio::select! {
        finished = tokio::time::timeout(ATTEMPT_LIMIT, output) => match finished {
            Err(_) => Err(AttemptError::Failed("超过 60 秒仍未结束，已停止".to_owned())),
            Ok(Err(err)) => Err(AttemptError::Failed(format!("无法运行 sysbench：{err}"))),
            Ok(Ok(output)) => {
                let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                text.push_str(&String::from_utf8_lossy(&output.stderr));
                sysbench::outcome(output.status.code(), &text).map_err(AttemptError::Failed)
            }
        },
        () = cancel.cancelled() => Err(AttemptError::Cancelled),
    }
}
