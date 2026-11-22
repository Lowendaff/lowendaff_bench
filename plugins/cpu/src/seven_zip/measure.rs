//! Runner for the 7-Zip LZMA2 benchmark (`7zz b -mmt1` and `-mmt<threads>`).

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
use bench_tool::retried::Retried;

use super::parse;
use super::parse::Reading;
use super::result::Failure;
use super::result::Fault;
use super::result::Output;
use super::result::Run;

/// Attempts per thread count.
pub const ATTEMPTS: usize = 2;

/// Time limit per `7zz b` invocation.
pub const ATTEMPT_LIMIT: Duration = Duration::from_secs(180);

/// Dictionary size exponent (`-md18` = 256 KiB; `7zz b` default on systems with < 1 GiB is smaller,
/// while `-md22` = 4 MiB is standard when memory permits; leaving `7zz b` to pick its standard
/// dictionary or capping on tiny RAM keeps runs fast and OOM-safe).
#[must_use]
pub fn args(threads: usize, low_memory: bool) -> Vec<OsString> {
    let mut out = vec![
        OsString::from("b"),
        OsString::from(format!("-mmt{threads}")),
    ];
    if low_memory {
        out.push(OsString::from("-md18"));
    }
    out
}

/// Builds one `7zz b` job.
#[must_use]
pub fn attempt(program: &Path, threads: usize, low_memory: bool) -> Job {
    let mut job = Job::new(program, args(threads, low_memory), ATTEMPT_LIMIT);
    job.group = true;
    job.first_to_go = true;
    job
}

/// Runs single-thread and optional multi-thread `7zz b`, returning either `Ok(Output)` or
/// `Err((Option<Box<Output>>, Failure))`.
pub async fn measure<O, F, Fut, P>(
    ctx: &Context<O>,
    program: &Path,
    threads: usize,
    low_memory: bool,
    mut on_partial: P,
    run: F,
) -> Result<Output, (Option<Box<Output>>, Failure)>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
    P: FnMut(&Output),
{
    ctx.progress(Step::CpuSuite {
        suite: "7zip",
        stage: None,
        threads: Some(1),
    });
    let (single_reading, single_retries) = match tried(ctx, &run, program, 1, low_memory).await {
        Tried::Done(reading, retries) => (reading, retries),
        Tried::Failed(faults) => return Err((None, Failure::Single(faults))),
        Tried::Cancelled => return Err((None, Failure::Cancelled)),
    };
    let mut output = Output {
        version: single_reading.version.clone(),
        single: to_run(1, &single_reading, single_retries),
        multi: None,
    };
    if threads <= 1 {
        return Ok(output);
    }
    on_partial(&output);
    ctx.progress(Step::CpuSuite {
        suite: "7zip",
        stage: None,
        threads: Some(threads),
    });
    match tried(ctx, &run, program, threads, low_memory).await {
        Tried::Done(reading, retries) => {
            if output.version.is_none() {
                output.version = reading.version.clone();
            }
            output.multi = Some(to_run(threads, &reading, retries));
            Ok(output)
        }
        Tried::Failed(faults) => Err((Some(Box::new(output)), Failure::Multi(faults))),
        Tried::Cancelled => Err((Some(Box::new(output)), Failure::Cancelled)),
    }
}

fn to_run(threads: usize, reading: &Reading, retries: Vec<Fault>) -> Run {
    Run {
        threads,
        compress_kib_s: reading.compress_kib_s,
        compress_mips: reading.compress_mips,
        decompress_kib_s: reading.decompress_kib_s,
        decompress_mips: reading.decompress_mips,
        usage_pct: reading.usage_pct,
        ru_mips: reading.ru_mips,
        total_mips: reading.total_mips,
        retries,
    }
}

enum Tried {
    Done(Reading, Vec<Fault>),
    Failed(Vec<Fault>),
    Cancelled,
}

enum Attempt {
    Failed(Fault),
    Cancelled,
}

async fn tried<O, F, Fut>(
    ctx: &Context<O>,
    run: &F,
    program: &Path,
    threads: usize,
    low_memory: bool,
) -> Tried
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let once = || async {
        match run(attempt(program, threads, low_memory)).await {
            Ok(output) => parse::reading(&output).map_err(Attempt::Failed),
            Err(RunError::Fault(fault)) => Err(Attempt::Failed(Fault::Tool(fault))),
            Err(RunError::Cancelled) => Err(Attempt::Cancelled),
        }
    };
    let failed = |attempt: &Attempt| matches!(attempt, Attempt::Failed(_));
    let cancel = ctx.cancel_token();
    let retried = Retried::run(ATTEMPTS, Duration::ZERO, &cancel, failed, once).await;
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
