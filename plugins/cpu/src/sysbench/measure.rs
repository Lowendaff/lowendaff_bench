//! The measurement: a run with one thread and, when more are usable, a run with all of them,
//! each tried up to [`ATTEMPTS`] times, with the result published after the first.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::typed::Context;
use bench_api::typed::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::retried::Retried;

use super::parse;
use super::parse::Reading;
use super::result::Failure;
use super::result::Fault;
use super::result::Output;
use super::result::Run;

/// Seconds each run lasts, as in the owner's `linux_bench`.
pub const SECONDS: u32 = 20;

/// The largest number tested for primality, as in `linux_bench`.
pub const MAX_PRIME: u32 = 10_000;

/// Attempts per run: the first one and 3 retries.
pub const ATTEMPTS: usize = 4;

/// An attempt runs for [`SECONDS`]; the rest is room to start and to stop.
pub const ATTEMPT_LIMIT: Duration = Duration::from_secs(60);

/// The arguments of one run with `threads` threads.
#[must_use]
pub fn args(threads: usize) -> Vec<OsString> {
    [
        "cpu".to_owned(),
        format!("--cpu-max-prime={MAX_PRIME}"),
        format!("--threads={threads}"),
        format!("--time={SECONDS}"),
        "run".to_owned(),
    ]
    .map(OsString::from)
    .to_vec()
}

/// An attempt of `program` with `threads` threads.
#[must_use]
pub fn attempt(program: &Path, threads: usize) -> Job {
    Job::new(program, args(threads), ATTEMPT_LIMIT)
}

/// Measures with one thread and, when `threads` is more, with `threads`. `run` runs an
/// attempt's job: the shared runner, or a recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    program: &Path,
    threads: usize,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    ctx.progress("单线程测试");
    let (reading, retries) = match tried(ctx, &run, program, 1).await {
        Tried::Done(reading, retries) => (reading, retries),
        Tried::Failed(faults) => return Err(Stop::Failed(Failure::Single(faults))),
        Tried::Cancelled => return Err(Stop::Failed(Failure::Cancelled)),
    };
    let mut output = Output {
        version: reading.version,
        single: Run {
            threads: 1,
            rate: reading.rate,
            retries,
        },
        multi: None,
    };
    if threads <= 1 {
        return Ok(output);
    }
    ctx.publish(&output);
    ctx.progress(format!("多线程测试（{threads} 线程）"));
    match tried(ctx, &run, program, threads).await {
        Tried::Done(reading, retries) => {
            output.multi = Some(Run {
                threads,
                rate: reading.rate,
                retries,
            });
            Ok(output)
        }
        Tried::Failed(faults) => Err(Stop::Failed(Failure::Multi(faults))),
        Tried::Cancelled => Err(Stop::Failed(Failure::Cancelled)),
    }
}

/// How one run, tried up to [`ATTEMPTS`] times, ended.
enum Tried {
    /// An attempt worked: its reading, and the faults of the attempts before it.
    Done(Reading, Vec<Fault>),
    /// Every attempt failed: their faults, oldest first.
    Failed(Vec<Fault>),
    /// The plugin was cancelled.
    Cancelled,
}

/// Why an attempt gave no reading.
enum Attempt {
    Failed(Fault),
    Cancelled,
}

/// Runs with `threads` threads until an attempt works, up to [`ATTEMPTS`] times.
async fn tried<F, Fut>(ctx: &Context<Output>, run: &F, program: &Path, threads: usize) -> Tried
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let once = || async {
        match run(attempt(program, threads)).await {
            Ok(output) => parse::reading(&output).map_err(Attempt::Failed),
            Err(RunError::Fault(fault)) => Err(Attempt::Failed(Fault::Tool(fault))),
            Err(RunError::Cancelled) => Err(Attempt::Cancelled),
        }
    };
    let failed = |attempt: &Attempt| matches!(attempt, Attempt::Failed(_));
    let cancel = ctx.cancel_token();
    let retried = Retried::run(ATTEMPTS, Duration::ZERO, &cancel, failed, once).await;
    // Only failures are tried again, so the earlier attempts all failed.
    let mut faults: Vec<Fault> = retried
        .failures
        .into_iter()
        .filter_map(|attempt| match attempt {
            Attempt::Failed(fault) => Some(fault),
            Attempt::Cancelled => None,
        })
        .collect();
    match retried.last {
        Ok(reading) => Tried::Done(reading, faults),
        Err(Attempt::Failed(fault)) => {
            faults.push(fault);
            Tried::Failed(faults)
        }
        Err(Attempt::Cancelled) => Tried::Cancelled,
    }
}
