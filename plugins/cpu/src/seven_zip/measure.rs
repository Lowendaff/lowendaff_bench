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

/// Selects an explicit `7zz b -md<exp>` dictionary cap (`18..=24`) when available host/cgroup
/// memory cannot comfortably fit upstream `7zz b`'s `-md25` (32 MiB) multi-instance footprint.
///
/// Upstream `7zz b` without `-md` sizes its dictionary against `sysinfo().totalram`, ignoring
/// container cgroup memory limits and occupied RAM while spawning `(threads / 2).max(1)` LZMA
/// encoder/decoder instances (~460 MiB each at `-md25`). Returning `Some(18)` below 1 GiB
/// preserves `< 1 GiB => -md18` while returning `Some(exp)` (`18..=24`) caps both single-thread
/// and multi-thread runs to the same dictionary size on memory-constrained hosts/containers.
#[must_use]
pub fn choose_dict_exp(threads: usize, available_bytes: Option<u64>) -> Option<u8> {
    let avail = available_bytes?;
    if avail < (1024 << 20) {
        return Some(18);
    }
    let budget = (avail / 4) * 3;
    let instances = (threads.max(1) / 2).max(1) as u64;
    let exp = (18_u8..=25)
        .rev()
        .find(|&e| {
            let per_instance = ((1_u64 << e) * 14) + (12 << 20);
            instances.saturating_mul(per_instance) <= budget
        })
        .unwrap_or(18);
    (exp < 25).then_some(exp)
}

/// Builds `7zz b` arguments for `threads` (clamped to `>= 1`) and optional `-md<exp>` cap.
#[must_use]
pub fn args(threads: usize, dict_exp: Option<u8>) -> Vec<OsString> {
    let threads = threads.max(1);
    let mut out = vec![
        OsString::from("b"),
        OsString::from(format!("-mmt{threads}")),
    ];
    if let Some(exp) = dict_exp {
        out.push(OsString::from(format!("-md{}", exp.clamp(18, 25))));
    }
    out
}

/// Builds one `7zz b` job.
#[must_use]
pub fn attempt(program: &Path, threads: usize, dict_exp: Option<u8>) -> Job {
    let mut job = Job::new(program, args(threads, dict_exp), ATTEMPT_LIMIT);
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
    available_bytes: Option<u64>,
    mut on_partial: P,
    run: F,
) -> Result<Output, (Option<Box<Output>>, Failure)>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
    P: FnMut(&Output),
{
    if ctx.cancel_token().is_cancelled() {
        return Err((None, Failure::Cancelled));
    }
    let dict_exp = choose_dict_exp(threads, available_bytes);
    ctx.progress(Step::CpuSuite {
        suite: "7zip",
        stage: None,
        threads: Some(1),
    });
    let (single_reading, single_retries) = match tried(ctx, &run, program, 1, dict_exp).await {
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
    if ctx.cancel_token().is_cancelled() {
        return Err((Some(Box::new(output)), Failure::Cancelled));
    }
    ctx.progress(Step::CpuSuite {
        suite: "7zip",
        stage: None,
        threads: Some(threads),
    });
    match tried(ctx, &run, program, threads, dict_exp).await {
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
    dict_exp: Option<u8>,
) -> Tried
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let cancel = ctx.cancel_token();
    if cancel.is_cancelled() {
        return Tried::Cancelled;
    }
    let once = || async {
        match run(attempt(program, threads, dict_exp)).await {
            Ok(output) => parse::reading(&output).map_err(Attempt::Failed),
            Err(RunError::Fault(fault)) => Err(Attempt::Failed(Fault::Tool(fault))),
            Err(RunError::Cancelled) => Err(Attempt::Cancelled),
        }
    };
    let failed = |attempt: &Attempt| matches!(attempt, Attempt::Failed(_));
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
        Err(_) if cancel.is_cancelled() => Tried::Cancelled,
        Err(Attempt::Failed(fault)) => {
            faults.push(fault);
            Tried::Failed(faults)
        }
        Err(Attempt::Cancelled) => Tried::Cancelled,
    }
}
