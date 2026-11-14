//! The measurement: one multichase run per cache level and one in memory, in the plan's order;
//! each run is tried up to [`ATTEMPTS`](crate::ATTEMPTS) times, and the result is published
//! after each run.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;

use bench_api::typed::Context;
use bench_api::typed::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Output;
use super::result::Row;
use crate::Tried;
use crate::chase;
use crate::plan::Level;
use crate::plan::Plan;

/// The arguments of one run through a buffer of `buffer` bytes.
#[must_use]
pub fn args(buffer: u64) -> Vec<OsString> {
    [
        "-m".to_owned(),
        buffer.to_string(),
        "-n".to_owned(),
        chase::SAMPLES.to_string(),
    ]
    .map(OsString::from)
    .to_vec()
}

/// One run of `program` through a buffer of `buffer` bytes.
#[must_use]
pub fn job(program: &Path, buffer: u64) -> Job {
    Job::new(program, args(buffer), chase::RUN_LIMIT)
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
    let mut output = Output {
        rows: plan
            .chases
            .iter()
            .map(|chase| Row {
                level: chase.level,
                buffer: chase.buffer,
                latency: None,
                retries: Vec::new(),
            })
            .collect(),
        missing: plan.missing.clone(),
        small: plan.below_caches(plan.single),
        cache_unknown: plan.cache_unknown,
    };
    let chases = plan.chases.iter().enumerate();
    for (index, chase) in chases {
        match chase.level {
            Level::L1 => ctx.progress("L1 测试"),
            Level::L2 => ctx.progress("L2 测试"),
            Level::L3 => ctx.progress("L3 测试"),
            Level::Memory => ctx.progress("内存测试"),
        }
        match crate::tried(ctx, &run, &job(program, chase.buffer), parse::latency).await {
            Tried::Done(latency, retries) => {
                let row = &mut output.rows[index];
                row.latency = Some(latency);
                row.retries = retries;
                ctx.publish(&output);
            }
            Tried::Failed(faults) => {
                return Err(Stop::Failed(Failure::Run {
                    level: chase.level,
                    faults,
                }));
            }
            Tried::Cancelled => return Err(Stop::Failed(Failure::Cancelled)),
        }
    }
    Ok(output)
}
