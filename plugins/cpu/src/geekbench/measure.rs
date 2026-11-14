//! The measurement: Geekbench runs once, and again after a failed upload, up to [`ATTEMPTS`]
//! runs in all, each in a process group of its own and as the kernel's first choice when memory
//! runs out.

use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench_api::Context;
use bench_tool::job;
use bench_tool::job::Adjustment;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::retried::Retried;

use super::parse;
use super::parse::Verdict;
use super::result::Failure;
use super::result::Fault;
use super::result::NoSwap;
use super::result::Output;
use super::result::Scores;

/// The longest one run may take.
pub const RUN_LIMIT: Duration = Duration::from_secs(60 * 60);

/// Runs: the first one and 3 more after failed uploads.
pub const ATTEMPTS: usize = 4;

/// The wait before running again after a failed upload.
pub const RETRY_WAIT: Duration = Duration::from_secs(30);

/// A run of `program`: the CPU benchmark, uploaded, since the free version shows scores only
/// after an upload; in `work_dir`, in English, which `parse` reads, in a process group of its
/// own, since Geekbench starts a worker, and as the kernel's first choice when memory runs out.
/// The log never quotes its output, which can hold the link that claims the result.
#[must_use]
pub fn attempt(program: &Path, work_dir: &Path) -> Job {
    let mut job = Job::new(program, ["--cpu", "--upload"], RUN_LIMIT);
    job.envs = vec![("LANG".into(), "C".into())];
    job.dir = Some(work_dir.to_path_buf());
    job.group = true;
    job.first_to_go = true;
    job.quotable = false;
    job
}

/// One run of Geekbench, as the measurement reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// How the run ended, and what became of `oom_score_adj`.
    pub outcome: job::Outcome,
    /// Whether the kernel killed a process for lack of memory during the run.
    pub killed: bool,
}

/// Why a run gave no scores.
enum Try {
    Upload(String),
    Failed(Fault),
    OutOfMemory,
    Cancelled,
}

/// Runs Geekbench through `run` until it gives scores, again after a failed upload, and adds to
/// `output` what the runs tell, publishing it after a run that could not go first. `run` runs a
/// job and counts the kernel's kills: the shared runner, or a recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    output: Output,
    program: &Path,
    run: F,
) -> (Output, Result<(), Failure>)
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Ran>,
{
    // The plugin's future runs on any of the runtime's threads, so the runs share these by lock.
    let output = Mutex::new(output);
    let runs = AtomicUsize::new(0);
    let once = || async {
        let number = runs.fetch_add(1, Ordering::Relaxed) + 1;
        if number > 1 {
            ctx.progress(format!("上传失败，第 {number} 次运行"));
        }
        let ran = run(attempt(program, ctx.work_dir())).await;
        if let Some(adjustment) = &ran.outcome.adjustment
            && *adjustment != Adjustment::Done
        {
            let mut output = output.lock().unwrap_or_else(PoisonError::into_inner);
            if output.first_to_go.is_none() {
                output.first_to_go = Some(adjustment.clone());
                ctx.publish(&output);
            }
        }
        scores(ran)
    };
    let upload = |reason: &Try| matches!(reason, Try::Upload(_));
    let cancel = ctx.cancel_token();
    let retried = Retried::run(ATTEMPTS, RETRY_WAIT, &cancel, upload, once).await;
    let mut output = output.into_inner().unwrap_or_else(PoisonError::into_inner);
    output.uploads = retried
        .failures
        .into_iter()
        .filter_map(|reason| match reason {
            Try::Upload(reason) => Some(reason),
            Try::Failed(_) | Try::OutOfMemory | Try::Cancelled => None,
        })
        .collect();
    let result = match retried.last {
        Ok(scores) => {
            output.scores = Some(scores);
            Ok(())
        }
        // A run that stops short once the plugin is cancelled stops because of it.
        Err(_) if cancel.is_cancelled() => Err(Failure::Cancelled),
        Err(Try::Upload(reason)) => Err(Failure::Upload(reason)),
        Err(Try::Failed(fault)) => Err(Failure::Run(fault)),
        Err(Try::OutOfMemory) => Err(Failure::OutOfMemory),
        Err(Try::Cancelled) => Err(Failure::Cancelled),
    };
    (output, result)
}

/// The scores of a run, or why there are none.
fn scores(ran: Ran) -> Result<Scores, Try> {
    match ran.outcome.ended {
        Err(RunError::Cancelled) => Err(Try::Cancelled),
        Err(RunError::Fault(fault)) => Err(Try::Failed(Fault::Tool(fault))),
        Ok(output) => match parse::verdict(&output) {
            Verdict::Scores(scores) => Ok(scores),
            // The kernel kills the worker, so the launcher's own end does not show it.
            _ if ran.killed => Err(Try::OutOfMemory),
            Verdict::UploadFailed(reason) => Err(Try::Upload(reason)),
            Verdict::Failed(fault) => Err(Try::Failed(fault)),
        },
    }
}

/// Why a swap file could not be made or turned on.
#[must_use]
pub fn no_swap(err: &io::Error) -> NoSwap {
    match err.kind() {
        // EPERM: swapon needs CAP_SYS_ADMIN, which containers seldom have.
        io::ErrorKind::PermissionDenied => NoSwap::Denied,
        // EINVAL: ZFS, tmpfs, overlay and copy-on-write btrfs files cannot hold swap.
        io::ErrorKind::InvalidInput => NoSwap::Unsupported,
        _ => NoSwap::Error(err.to_string()),
    }
}
