//! The measurement: STREAM's build for the plan's arrays with one thread and, when more are
//! usable, with all of them spread over the nodes; each run is tried up to
//! [`ATTEMPTS`](crate::ATTEMPTS) times, and the result is published after each run.

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
use super::result::Output;
use super::result::Run;
use crate::Tried;
use crate::plan;
use crate::plan::Plan;

/// A run with the largest arrays takes seconds; the limit leaves ample room.
pub const RUN_LIMIT: Duration = Duration::from_secs(180);

/// One run with `threads` threads of the build beside `smallest`, STREAM's build for its
/// smallest arrays, for arrays of `millions` million elements.
#[must_use]
pub fn job(smallest: &Path, millions: u64, threads: usize) -> Job {
    let program = smallest.with_file_name(format!("stream-{millions}m"));
    let mut job = Job::new(program, Vec::<String>::new(), RUN_LIMIT);
    job.envs
        .push(("OMP_NUM_THREADS".into(), threads.to_string().into()));
    if threads > 1 {
        job.envs.push(("OMP_PROC_BIND".into(), "spread".into()));
    }
    job
}

/// Measures with the build beside `smallest` as `plan` says, or fails when there is no plan for
/// want of the available memory. `run` runs an attempt's job: the shared runner, or a recording
/// in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    smallest: &Path,
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
    let Some(millions) = plan.stream else {
        return Err(Stop::Skipped(Failure::Short {
            budget: plan.budget,
        }));
    };
    let mut output = Output {
        version: None,
        millions,
        threads: plan.threads,
        below_rule: plan.below_caches(plan::array_bytes(millions)).is_some(),
        cache_unknown: plan.cache_unknown,
        runs: Vec::new(),
    };
    let columns = plan.columns();
    for &threads in &columns {
        if threads > 1 {
            ctx.progress(format!("多线程测试（{threads} 线程）"));
        } else {
            ctx.progress("单线程测试");
        }
        match crate::tried(ctx, &run, &job(smallest, millions, threads), parse::reading).await {
            Tried::Done(reading, retries) => {
                if output.version.is_none() {
                    output.version = reading.version;
                }
                output.runs.push(Run {
                    threads,
                    rates: reading.rates,
                    retries,
                });
                ctx.publish(&output);
            }
            Tried::Failed(faults) => return Err(Stop::Failed(Failure::Run { threads, faults })),
            Tried::Cancelled => return Err(Stop::Failed(Failure::Cancelled)),
        }
    }
    Ok(output)
}
