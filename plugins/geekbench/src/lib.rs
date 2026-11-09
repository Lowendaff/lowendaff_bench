//! Geekbench 7 (design cpu, §6, §7): Primate Labs' own build, downloaded at run time (CP-02)
//! and run as the free version, which uploads its result (CP-05). Temporary swap helps when
//! memory is short (CP-06), and the kernel kills Geekbench first when memory runs out anyway
//! (CP-13). Only failed uploads are retried (CP-15).

pub mod memory;
pub mod output;
pub mod process;
pub mod report;

use std::future::Future;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::GlibcVersion;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_host::Root;
use serde::Deserialize;
use swap_file::SwapFile;

use crate::memory::Plan;
use crate::output::Scores;
use crate::output::Verdict;

/// Geekbench 7.1.0 for Linux as Primate Labs publishes it: the release for x86_64 and the
/// preview for aarch64 (design cpu, §3.2).
const GEEKBENCH: ToolSpec = ToolSpec {
    name: "geekbench",
    version: "7.1.0",
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://cdn.geekbench.com/Geekbench-7.1.0-Linux.tar.gz",
            sha256: "411b72a9c851b69e6f91d390649d419e2e625f475e3983880169651ca27a8672",
            size: 484_515_924,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://cdn.geekbench.com/Geekbench-7.1.0-LinuxARMPreview.tar.gz",
            sha256: "bb6abc61a68a9b33fecf1f3c0b800050dd76e93666a13eaabfa52cbd26ad3319",
            size: 419_635_785,
        },
    ],
};

/// The longest one run may take (design cpu, §6.4).
const RUN_LIMIT: Duration = Duration::from_secs(60 * 60);

/// Runs: the first one and 3 more after failed uploads (CP-15).
pub const ATTEMPTS: usize = 4;

/// The wait before running again after a failed upload (CP-15).
pub const RETRY_WAIT: Duration = Duration::from_secs(30);

/// Why a run ended without scores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptError {
    /// The benchmark ran, but the upload failed for this reason; worth another run.
    UploadFailed(String),
    /// Geekbench failed for this reason.
    Failed(String),
    /// The kernel killed it for lack of memory.
    OutOfMemory,
    /// It went over [`RUN_LIMIT`].
    TimedOut,
    /// The item was cancelled.
    Cancelled,
}

/// Geekbench 7. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Geekbench {}

impl Plugin for Geekbench {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "cpu.geekbench",
        title: "Geekbench 7",
        version: "0.1.0",
        requires: Requirements {
            tools: &[GEEKBENCH],
            // cdn.geekbench.com has IPv4 addresses only (design cpu, §3.2).
            net: Net::Ipv4,
            platforms: &[],
            glibc: Some(GlibcVersion {
                major: 2,
                minor: 35,
            }),
        },
        // Up to four runs of up to an hour each, and the swap (CP-15).
        timeout: Duration::from_secs(250 * 60),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let geekbench = ctx.tool("geekbench")?.to_path_buf();
        let kind = ctx.env().platform.kind();
        let work_dir = ctx.work_dir().to_path_buf();
        let host = tokio::task::spawn_blocking(move || {
            memory::Host::read(&Root::system(), kind, &work_dir)
        })
        .await?;
        let swap = add_swap(ctx, &host).await?;
        let warned = AtomicBool::new(false);
        let result = measure(ctx, || run_geekbench(ctx, &geekbench, &warned)).await;
        if let Some(swap) = swap {
            remove_swap(ctx, swap).await;
        }
        result
    }
}

/// Runs Geekbench through `attempt` until it gives scores, again after a failed upload up to
/// [`ATTEMPTS`] runs in all, and emits the records.
pub async fn measure<F, Fut>(ctx: &Context, attempt: F) -> Result<(), PluginError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Scores, AttemptError>>,
{
    let mut uploads = Vec::new();
    loop {
        let reason = match attempt().await {
            Ok(scores) => {
                for record in report::scores(&scores) {
                    ctx.emit(record);
                }
                if let Some(note) = report::retried(&uploads) {
                    ctx.emit(note);
                }
                return Ok(());
            }
            Err(AttemptError::UploadFailed(reason)) => reason,
            Err(AttemptError::Failed(reason)) => return Err(PluginError::fail(reason)),
            Err(AttemptError::OutOfMemory) => return Err(PluginError::fail(report::OUT_OF_MEMORY)),
            Err(AttemptError::TimedOut) => return Err(PluginError::fail(report::TIMED_OUT)),
            Err(AttemptError::Cancelled) => return Err(PluginError::fail(report::CANCELLED)),
        };
        if uploads.len() + 1 == ATTEMPTS {
            return Err(PluginError::fail(report::upload_failed(&reason)));
        }
        uploads.push(reason);
        let cancel = ctx.cancel_token();
        tokio::select! {
            () = tokio::time::sleep(RETRY_WAIT) => {}
            () = cancel.cancelled() => return Err(PluginError::fail(report::CANCELLED)),
        }
    }
}

/// Temporary swap when Geekbench's estimated need exceeds what the host has, with a note
/// whenever memory is short (CP-06).
async fn add_swap(ctx: &Context, host: &memory::Host) -> Result<Option<SwapFile>, PluginError> {
    let Some(available) = host.available else {
        return Ok(None);
    };
    let need = memory::need(host.threads);
    let gib = match memory::plan(need, available, host.free_space, host.swap_unused) {
        Plan::Enough => return Ok(None),
        Plan::Unused => {
            ctx.emit(report::swap_impossible(
                need,
                available,
                report::SWAP_UNUSED,
            ));
            return Ok(None);
        }
        Plan::NoRoom => {
            let reason = report::no_room(host.free_space);
            ctx.emit(report::swap_impossible(need, available, &reason));
            return Ok(None);
        }
        Plan::Swap(gib) => gib,
    };
    let path = ctx.work_dir().join("swapfile");
    let cancel = ctx.cancel_token();
    let created = tokio::task::spawn_blocking(move || {
        SwapFile::try_new(&path, gib * memory::GIB, &|| cancel.is_cancelled())
    })
    .await?;
    match created {
        Ok(swap) => {
            ctx.emit(report::swap_added(need, available, gib));
            Ok(Some(swap))
        }
        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
            Err(PluginError::fail(report::CANCELLED))
        }
        Err(err) => {
            let reason = report::swap_error(&err);
            ctx.emit(report::swap_impossible(need, available, &reason));
            Ok(None)
        }
    }
}

/// Turns the swap off and removes it, or says what to do by hand.
async fn remove_swap(ctx: &Context, swap: SwapFile) {
    let path = swap.path().to_path_buf();
    let removed = tokio::task::spawn_blocking(move || swap.remove())
        .await
        .unwrap_or_else(|err| Err(io::Error::other(err.to_string())));
    if let Err(err) = removed {
        ctx.emit(report::swap_left_on(&path, &err));
    }
}

/// One run of Geekbench, without a shell, in a process group of its own and with
/// `oom_score_adj` 1000. The group is killed as a whole on cancellation and after
/// [`RUN_LIMIT`]. `warned` keeps the note on `oom_score_adj` to one per item.
async fn run_geekbench(
    ctx: &Context,
    geekbench: &Path,
    warned: &AtomicBool,
) -> Result<Scores, AttemptError> {
    let root = Root::system();
    let kills_before = process::oom_kills(&root);
    let mut command = tokio::process::Command::new(geekbench);
    command
        .args(["--cpu", "--upload"])
        .current_dir(ctx.work_dir())
        // English output, which is what `output` reads.
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let (child, adjustment) = process::spawn_first_to_go(&mut command);
    if let Some(note) = report::oom(&adjustment)
        && !warned.swap(true, Ordering::Relaxed)
    {
        ctx.emit(note);
    }
    let child = child.map_err(|err| AttemptError::Failed(format!("无法运行 Geekbench：{err}")))?;
    let group = process::Group::of(&child);
    let cancel = ctx.cancel_token();
    // The group is killed while its leader still lives, so that its id cannot have been
    // taken by another group yet.
    let output = tokio::select! {
        finished = tokio::time::timeout(RUN_LIMIT, child.wait_with_output()) => match finished {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => {
                group.kill();
                return Err(AttemptError::Failed(format!("无法读取 Geekbench 的输出：{err}")));
            }
            Err(_) => {
                group.kill();
                return Err(AttemptError::TimedOut);
            }
        },
        () = cancel.cancelled() => {
            group.kill();
            return Err(AttemptError::Cancelled);
        }
    };
    group.disarm();
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let verdict = output::verdict(output.status.code(), &text);
    let killed = matches!(
        (kills_before, process::oom_kills(&root)),
        (Some(before), Some(after)) if after > before
    );
    match verdict {
        Verdict::Scores(scores) => Ok(scores),
        // The kernel kills the worker, so the launcher's own end does not show it.
        _ if killed => Err(AttemptError::OutOfMemory),
        Verdict::UploadFailed(reason) => Err(AttemptError::UploadFailed(reason)),
        Verdict::Failed(reason) => Err(AttemptError::Failed(reason)),
    }
}
