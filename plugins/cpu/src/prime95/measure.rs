//! Runner for Prime95 (`mprime -t -d`) Small-FFT torture test with timed `SIGINT` stop.

use std::ffi::OsString;
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

/// Default Small-FFT torture window before sending `SIGINT` (`65` seconds).
pub const TORTURE_SECONDS: u64 = 65;

/// Hard kill limit if `mprime` fails to shut down after `SIGINT`.
pub const RUN_LIMIT: Duration = Duration::from_secs(110);

/// Formats `local.txt` / `prime.txt` for an in-cache Small-FFT torture test (`4K..=64K`).
#[must_use]
pub fn torture_config(threads: usize) -> String {
    let workers = threads.max(1);
    format!(
        "V24OptionsConverted=1\n\
         WGUID_version=2\n\
         StressTester=1\n\
         UsePrimenet=0\n\
         TortureThreads={workers}\n\
         MinTortureFFT=4\n\
         MaxTortureFFT=64\n\
         TortureMem=0\n\
         TortureTime=1\n"
    )
}

/// Builds the `mprime -t -d -W<work_dir>` job with isolated `LD_LIBRARY_PATH` pointing to
/// `mprime`'s unpacked directory (for bundled `libgmp.so.10`) and `interrupt_after` set to
/// `duration`.
#[must_use]
pub fn attempt(program: &Path, work_dir: &Path, duration: Duration) -> Job {
    let bin_dir = program.parent().unwrap_or_else(|| Path::new("."));
    let ld_path = OsString::from(bin_dir.as_os_str());
    let mut work_arg = OsString::from("-W");
    work_arg.push(work_dir.as_os_str());
    let mut job = Job::new(
        program,
        [OsString::from("-t"), OsString::from("-d"), work_arg],
        RUN_LIMIT.max(duration + Duration::from_secs(30)),
    );
    job.envs = vec![("LD_LIBRARY_PATH".into(), ld_path)];
    job.dir = Some(work_dir.to_path_buf());
    job.interrupt_after = Some(duration);
    job.group = true;
    job.first_to_go = true;
    job
}

/// Writes Small-FFT config into `work_dir` and runs `mprime -t -d` for `duration`.
pub async fn measure<O, F, Fut>(
    ctx: &Context<O>,
    program: &Path,
    work_dir: &Path,
    threads: usize,
    duration: Duration,
    run: F,
) -> Result<Output, Failure>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    ctx.progress(Step::CpuSuite {
        suite: "prime95",
        stage: None,
        threads: Some(threads),
    });
    if let Err(err) = tokio::fs::create_dir_all(work_dir).await {
        return Err(Failure::Run(Fault::Setup(err.to_string())));
    }
    let cfg = torture_config(threads);
    for name in ["local.txt", "prime.txt"] {
        if let Err(err) = tokio::fs::write(work_dir.join(name), &cfg).await {
            let _ = tokio::fs::remove_dir_all(work_dir).await;
            return Err(Failure::Run(Fault::Setup(err.to_string())));
        }
    }
    let secs = duration.as_secs().max(1);
    let result = match run(attempt(program, work_dir, duration)).await {
        Ok(out) => parse::reading(&out, secs).map_err(Failure::Run),
        Err(RunError::Fault(fault)) => Err(Failure::Run(Fault::Tool(fault))),
        Err(RunError::Cancelled) => Err(Failure::Cancelled),
    };
    let _ = tokio::fs::remove_dir_all(work_dir).await;
    result
}
