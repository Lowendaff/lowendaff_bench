//! Runner for `y-cruncher skip-warnings bench <25m|100m>`.

use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Measurement;
use bench_api::Step;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Fault;
use super::result::Output;

/// Time limit for one y-cruncher Pi benchmark run.
pub const RUN_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Chooses `("100m", 100_000_000)` when at least 1.5 GiB RAM and 2 threads are available, else
/// `("25m", 25_000_000)` (~140 MiB working memory) so low-end 512 MiB / 1-vCPU hosts succeed.
#[must_use]
pub fn choose_size(threads: usize, available_bytes: Option<u64>) -> (&'static str, u64) {
    let enough_ram = available_bytes.is_some_and(|avail| avail >= (1536 << 20));
    if threads >= 2 && enough_ram {
        ("100m", 100_000_000)
    } else {
        ("25m", 25_000_000)
    }
}

/// Builds the y-cruncher benchmark job.
#[must_use]
pub fn attempt(program: &Path, work_dir: &Path, preset: &str) -> Job {
    let mut job = Job::new(program, ["skip-warnings", "bench", preset], RUN_LIMIT);
    job.dir = Some(work_dir.to_path_buf());
    job.group = true;
    job.first_to_go = true;
    job
}

/// Executes y-cruncher in `work_dir` and parses its Pi computation metrics.
pub async fn measure<O, F, Fut>(
    ctx: &Context<O>,
    program: &Path,
    work_dir: &Path,
    threads: usize,
    available_bytes: Option<u64>,
    run: F,
) -> Result<Output, Failure>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    if ctx.cancel_token().is_cancelled() {
        return Err(Failure::Cancelled);
    }
    let threads = threads.max(1);
    let (preset, digits) = choose_size(threads, available_bytes);
    ctx.progress(Step::CpuSuite {
        suite: "ycruncher",
        stage: Some(preset),
        threads: Some(threads),
    });
    if let Err(err) = tokio::fs::create_dir_all(work_dir).await {
        return Err(Failure::Run(Fault::Setup(err.to_string())));
    }
    let result = match run(attempt(program, work_dir, preset)).await {
        Ok(out) => parse::reading(&out, digits).map_err(Failure::Run),
        Err(RunError::Fault(fault)) => Err(Failure::Run(Fault::Tool(fault))),
        Err(RunError::Cancelled) => Err(Failure::Cancelled),
    };
    let _ = tokio::fs::remove_dir_all(work_dir).await;
    result
}
