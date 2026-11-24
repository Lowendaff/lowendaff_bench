//! Runner for the 12 standard Byte-UnixBench System Index tests (`1-copy` and `N-copy`).

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use bench_api::Context;
use bench_api::Measurement;
use bench_api::Step;
use bench_tool::UNIXBENCH;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Fault;
use super::result::IndexItem;
use super::result::IndexRun;
use super::result::Output;

/// Time limit for one sub-benchmark invocation.
pub const ITEM_LIMIT: Duration = Duration::from_secs(120);

/// Specification of one of the 12 standard UnixBench System Index sub-benchmarks.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Canonical UnixBench test id (`index.base` key).
    pub id: &'static str,
    /// Unit string (`lps`, `MWIPS`, `KBps`, `lpm`).
    pub unit: &'static str,
    /// Baseline score from `index.base` (`SPARCstation 20-61 = 10.0`).
    pub baseline: f64,
    /// How to invoke the sub-benchmark.
    pub kind: SpecKind,
}

/// Command shape of a UnixBench sub-benchmark.
#[derive(Debug, Clone, Copy)]
pub enum SpecKind {
    /// `<id> <seconds>` (`dhry2reg`, `execl`, `pipe`, `context1`, `spawn`, `syscall`).
    Timed {
        /// Target duration in seconds.
        seconds: u32,
    },
    /// `whetstone-double` (self-calibrates to ~10 s).
    Whetstone,
    /// `fstime -c -t <seconds> -d <dir> -b <bufsize> -m <max_blocks>`.
    FsCopy {
        /// Per-phase duration in seconds (Write, Read, Copy).
        seconds: u32,
        /// Buffer size in bytes.
        bufsize: u32,
        /// Maximum 1 KiB blocks.
        max_blocks: u32,
    },
    /// `looper <seconds> <bin_dir>/multi.sh <concurrent>`.
    Shell {
        /// Target duration in seconds.
        seconds: u32,
        /// Concurrent `tst.sh` pipelines per copy (`1` or `8`).
        concurrent: u32,
    },
}

/// All 12 sub-benchmarks of the official Byte-UnixBench System Index, in canonical order.
pub const INDEX_SPECS: [Spec; 12] = [
    Spec {
        id: "dhry2reg",
        unit: "lps",
        baseline: 116_700.0,
        kind: SpecKind::Timed { seconds: 10 },
    },
    Spec {
        id: "whetstone-double",
        unit: "MWIPS",
        baseline: 55.0,
        kind: SpecKind::Whetstone,
    },
    Spec {
        id: "execl",
        unit: "lps",
        baseline: 43.0,
        kind: SpecKind::Timed { seconds: 10 },
    },
    Spec {
        id: "fstime",
        unit: "KBps",
        baseline: 3_960.0,
        kind: SpecKind::FsCopy {
            seconds: 5,
            bufsize: 1024,
            max_blocks: 2000,
        },
    },
    Spec {
        id: "fsbuffer",
        unit: "KBps",
        baseline: 1_655.0,
        kind: SpecKind::FsCopy {
            seconds: 5,
            bufsize: 256,
            max_blocks: 500,
        },
    },
    Spec {
        id: "fsdisk",
        unit: "KBps",
        baseline: 5_800.0,
        kind: SpecKind::FsCopy {
            seconds: 5,
            bufsize: 4096,
            max_blocks: 8000,
        },
    },
    Spec {
        id: "pipe",
        unit: "lps",
        baseline: 12_440.0,
        kind: SpecKind::Timed { seconds: 10 },
    },
    Spec {
        id: "context1",
        unit: "lps",
        baseline: 4_000.0,
        kind: SpecKind::Timed { seconds: 10 },
    },
    Spec {
        id: "spawn",
        unit: "lps",
        baseline: 126.0,
        kind: SpecKind::Timed { seconds: 10 },
    },
    Spec {
        id: "shell1",
        unit: "lpm",
        baseline: 42.4,
        kind: SpecKind::Shell {
            seconds: 15,
            concurrent: 1,
        },
    },
    Spec {
        id: "shell8",
        unit: "lpm",
        baseline: 6.0,
        kind: SpecKind::Shell {
            seconds: 15,
            concurrent: 8,
        },
    },
    Spec {
        id: "syscall",
        unit: "lps",
        baseline: 15_000.0,
        kind: SpecKind::Timed { seconds: 10 },
    },
];

/// Builds the [`Job`] for one copy of `spec`, given `bin_dir` (`tools/unixbench`) and `copy_dir`.
#[must_use]
pub fn copy_job(spec: &Spec, bin_dir: &Path, copy_dir: &Path) -> Job {
    let (program, args): (_, Vec<OsString>) = match spec.kind {
        SpecKind::Timed { seconds } => (
            bin_dir.join(spec.id),
            vec![OsString::from(seconds.to_string())],
        ),
        SpecKind::Whetstone => (bin_dir.join("whetstone-double"), Vec::new()),
        SpecKind::FsCopy {
            seconds,
            bufsize,
            max_blocks,
        } => (
            bin_dir.join("fstime"),
            vec![
                OsString::from("-c"),
                OsString::from("-t"),
                OsString::from(seconds.to_string()),
                OsString::from("-d"),
                copy_dir.as_os_str().to_owned(),
                OsString::from("-b"),
                OsString::from(bufsize.to_string()),
                OsString::from("-m"),
                OsString::from(max_blocks.to_string()),
            ],
        ),
        SpecKind::Shell {
            seconds,
            concurrent,
        } => (
            bin_dir.join("looper"),
            vec![
                OsString::from(seconds.to_string()),
                bin_dir.join("multi.sh").into_os_string(),
                OsString::from(concurrent.to_string()),
            ],
        ),
    };
    let mut job = Job::new(program, args, ITEM_LIMIT);
    job.envs = vec![("UB_BINDIR".into(), bin_dir.as_os_str().to_owned())];
    job.dir = Some(copy_dir.to_path_buf());
    job.group = true;
    job.first_to_go = true;
    job
}

/// Runs the 12-item System Index at `1` copy and (when `threads > 1`) `threads` copies.
pub async fn measure<O, F, Fut, P>(
    ctx: &Context<O>,
    dhry2reg_path: &Path,
    work_dir: &Path,
    threads: usize,
    mut on_partial: P,
    run: F,
) -> Result<Output, (Option<Box<Output>>, Failure)>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
    P: FnMut(&Output),
{
    let bin_dir = dhry2reg_path.parent().unwrap_or_else(|| Path::new("."));
    let single = match run_pass(ctx, bin_dir, work_dir, 1, &run).await {
        Ok(pass) => pass,
        Err(RunPassErr::Fault(fault)) => {
            let _ = tokio::fs::remove_dir_all(work_dir).await;
            return Err((None, Failure::Single(fault)));
        }
        Err(RunPassErr::Cancelled) => {
            let _ = tokio::fs::remove_dir_all(work_dir).await;
            return Err((None, Failure::Cancelled));
        }
    };
    let mut output = Output {
        version: UNIXBENCH.version.to_owned(),
        single,
        multi: None,
    };
    if threads <= 1 {
        let _ = tokio::fs::remove_dir_all(work_dir).await;
        return Ok(output);
    }
    on_partial(&output);
    let result = match run_pass(ctx, bin_dir, work_dir, threads, &run).await {
        Ok(pass) => {
            output.multi = Some(pass);
            Ok(output)
        }
        Err(RunPassErr::Fault(fault)) => Err((Some(Box::new(output)), Failure::Multi(fault))),
        Err(RunPassErr::Cancelled) => Err((Some(Box::new(output)), Failure::Cancelled)),
    };
    let _ = tokio::fs::remove_dir_all(work_dir).await;
    result
}

enum RunPassErr {
    Fault(Fault),
    Cancelled,
}

async fn run_pass<O, F, Fut>(
    ctx: &Context<O>,
    bin_dir: &Path,
    work_dir: &Path,
    copies: usize,
    run: &F,
) -> Result<IndexRun, RunPassErr>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let pass_dir = work_dir.join(format!("copies-{copies}"));
    let sort_src = bin_dir.join("sort.src");
    let mut copy_dirs = Vec::with_capacity(copies);
    for idx in 0..copies {
        let dir = pass_dir.join(format!("c{idx}"));
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|err| RunPassErr::Fault(Fault::Setup(err.to_string())))?;
        if tokio::fs::metadata(&sort_src).await.is_ok() {
            tokio::fs::copy(&sort_src, dir.join("sort.src"))
                .await
                .map_err(|err| RunPassErr::Fault(Fault::Setup(err.to_string())))?;
        }
        copy_dirs.push(dir);
    }

    let mut items: Vec<IndexItem> = Vec::with_capacity(INDEX_SPECS.len());
    for spec in &INDEX_SPECS {
        if ctx.cancel_token().is_cancelled() {
            let _ = tokio::fs::remove_dir_all(&pass_dir).await;
            return Err(RunPassErr::Cancelled);
        }
        ctx.progress(Step::CpuSuite {
            suite: "unixbench",
            stage: Some(spec.id),
            threads: Some(copies),
        });
        let futs = copy_dirs.iter().map(|dir| {
            let job = copy_job(spec, bin_dir, dir);
            async move {
                let started = Instant::now();
                let res = run(job).await;
                (res, started.elapsed().as_secs_f64())
            }
        });
        let outcomes = futures_util::future::join_all(futs).await;
        let mut readings = Vec::with_capacity(copies);
        for (res, elapsed) in outcomes {
            match res {
                Ok(out) => {
                    let reading =
                        parse::copy_reading(spec.id, &out, elapsed).map_err(RunPassErr::Fault)?;
                    readings.push(reading);
                }
                Err(RunError::Fault(fault)) => {
                    let _ = tokio::fs::remove_dir_all(&pass_dir).await;
                    return Err(RunPassErr::Fault(Fault::Tool {
                        item: spec.id,
                        fault,
                    }));
                }
                Err(RunError::Cancelled) => {
                    let _ = tokio::fs::remove_dir_all(&pass_dir).await;
                    return Err(RunPassErr::Cancelled);
                }
            }
        }
        let item = parse::combine_copies(spec.id, spec.unit, spec.baseline, &readings)
            .ok_or(RunPassErr::Fault(Fault::NoCount { item: spec.id }))?;
        items.push(item);
    }
    let _ = tokio::fs::remove_dir_all(&pass_dir).await;
    let index = parse::geometric_index(&items)
        .ok_or(RunPassErr::Fault(Fault::NoCount { item: "index" }))?;
    Ok(IndexRun {
        copies,
        index,
        items,
    })
}
