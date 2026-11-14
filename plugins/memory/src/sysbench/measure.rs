//! The measurement: reading, then writing, each with one thread and, when more are usable, with
//! all of them; each run is tried up to [`ATTEMPTS`](crate::ATTEMPTS) times, and the result is
//! published after each run.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::typed::Context;
use bench_api::typed::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Operation;
use super::result::Output;
use super::result::Run;
use crate::Tried;
use crate::plan::Plan;

/// Seconds of each run.
pub const SECONDS: u32 = 10;

/// A run lasts [`SECONDS`]; the rest is room to start and to stop.
pub const RUN_LIMIT: Duration = Duration::from_secs(60);

/// The arguments of one run: `operation` with `threads` threads, each through a block of
/// `block` bytes.
#[must_use]
pub fn args(operation: Operation, threads: usize, block: u64) -> Vec<OsString> {
    let operation = match operation {
        Operation::Read => "read",
        Operation::Write => "write",
    };
    let mut args = vec![
        "memory".to_owned(),
        format!("--memory-block-size={block}"),
        "--memory-total-size=0".to_owned(),
        format!("--memory-oper={operation}"),
        "--memory-access-mode=seq".to_owned(),
    ];
    if threads > 1 {
        args.push("--memory-scope=local".to_owned());
    }
    args.extend([
        format!("--threads={threads}"),
        format!("--time={SECONDS}"),
        "run".to_owned(),
    ]);
    args.into_iter().map(OsString::from).collect()
}

/// One run of `program`.
#[must_use]
pub fn job(program: &Path, operation: Operation, threads: usize, block: u64) -> Job {
    Job::new(program, args(operation, threads, block), RUN_LIMIT)
}

/// Measures with `program` as `plan` says, or fails when there is no plan for want of the
/// available memory. `run` runs an attempt's job: the shared runner, or a recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    program: &Path,
    plan: Option<&Plan>,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let Some(plan) = plan else {
        return Err(Stop::Failed(Failure::NoMemoryInfo));
    };
    let multi = plan.threads > 1;
    let all = plan.per_thread.saturating_mul(plan.threads as u64);
    let small = plan
        .below_caches(plan.single)
        .or_else(|| plan.below_caches(all).filter(|_| multi));
    let mut output = Output {
        version: None,
        single: plan.single,
        per_thread: multi.then_some(plan.per_thread),
        threads: plan.threads,
        small,
        cache_unknown: plan.cache_unknown,
        runs: Vec::new(),
    };
    let columns = plan.columns();
    for operation in [Operation::Read, Operation::Write] {
        for &threads in &columns {
            match (operation, threads) {
                (Operation::Read, 1) => ctx.progress("单线程读取"),
                (Operation::Read, threads) => ctx.progress(format!("多线程读取（{threads} 线程）")),
                (Operation::Write, 1) => ctx.progress("单线程写入"),
                (Operation::Write, threads) => {
                    ctx.progress(format!("多线程写入（{threads} 线程）"));
                }
            }
            let block = if threads > 1 {
                plan.per_thread
            } else {
                plan.single
            };
            let job = job(program, operation, threads, block);
            match crate::tried(ctx, &run, &job, parse::reading).await {
                Tried::Done(reading, retries) => {
                    if output.version.is_none() {
                        output.version = reading.version;
                    }
                    output.runs.push(Run {
                        operation,
                        threads,
                        rate: reading.rate,
                        retries,
                    });
                    ctx.publish(&output);
                }
                Tried::Failed(faults) => {
                    return Err(Stop::Failed(Failure::Run {
                        operation,
                        threads,
                        faults,
                    }));
                }
                Tried::Cancelled => return Err(Stop::Failed(Failure::Cancelled)),
            }
        }
    }
    Ok(output)
}
