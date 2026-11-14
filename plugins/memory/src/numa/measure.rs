//! The measurement: for each cell of the plan's matrix, row by row, multiload's bandwidth and
//! then multichase's latency, run on the CPUs of the row's node with their memory bound to the
//! column's node; each run is tried up to [`ATTEMPTS`](crate::ATTEMPTS) times, but a refused
//! binding only once, and the result is published after each run.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;

use bench_api::typed::Context;
use bench_api::typed::Stop;
use bench_host::numa::Threads;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Cell;
use super::result::Failure;
use super::result::Fault;
use super::result::Kind;
use super::result::Measured;
use super::result::Output;
use crate::Tried;
use crate::chase;
use crate::plan;
use crate::plan::Plan;

/// The samples of each multiload run.
pub const BANDWIDTH_SAMPLES: u32 = 3;

/// multiload's arguments: stream-triad with `threads` threads of `per_thread` bytes each,
/// their memory bound to `node`.
#[must_use]
pub fn bandwidth_args(threads: usize, per_thread: u64, node: usize) -> Vec<OsString> {
    [
        "-l".to_owned(),
        "stream-triad".to_owned(),
        "-n".to_owned(),
        BANDWIDTH_SAMPLES.to_string(),
        "-t".to_owned(),
        threads.to_string(),
        "-m".to_owned(),
        per_thread.to_string(),
        "-W".to_owned(),
        format!("{node}:1"),
    ]
    .map(OsString::from)
    .to_vec()
}

/// multichase's arguments: a buffer of `buffer` bytes bound to `node`.
#[must_use]
pub fn latency_args(buffer: u64, node: usize) -> Vec<OsString> {
    [
        "-m".to_owned(),
        buffer.to_string(),
        "-n".to_owned(),
        chase::SAMPLES.to_string(),
        "-W".to_owned(),
        format!("{node}:1"),
    ]
    .map(OsString::from)
    .to_vec()
}

/// The bandwidth run of `multiload` for threads on `row`, memory on `column`, with the buffers
/// of `cell`.
#[must_use]
pub fn bandwidth(multiload: &Path, row: &Threads, column: usize, cell: plan::Cell) -> Job {
    let args = bandwidth_args(row.cpus.len(), cell.per_thread, column);
    let mut job = Job::new(multiload, args, chase::RUN_LIMIT);
    job.cpus = Some(row.cpus.clone());
    job
}

/// The latency run of `multichase` for threads on `row`, memory on `column`, with the buffers
/// of `cell`.
#[must_use]
pub fn latency(multichase: &Path, row: &Threads, column: usize, cell: plan::Cell) -> Job {
    let mut job = Job::new(
        multichase,
        latency_args(cell.latency, column),
        chase::RUN_LIMIT,
    );
    job.cpus = Some(row.cpus.clone());
    job
}

/// Measures with `multichase`, and `multiload` beside it, as `plan` says, or fails when there
/// is no plan for want of the available memory. `run` runs an attempt's job: the shared
/// runner, or a recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    multichase: &Path,
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
    let Some(matrix) = &plan.matrix else {
        return Err(Stop::Skipped(Failure::OneNode));
    };
    let multiload = multichase.with_file_name("multiload");
    let rows: Vec<usize> = matrix.rows.iter().map(|row| row.node).collect();
    let mut nodes: Vec<usize> = rows.iter().chain(&matrix.columns).copied().collect();
    nodes.sort_unstable();
    nodes.dedup();
    let mut output = Output {
        nodes: nodes.len(),
        rows,
        columns: matrix.columns.clone(),
        cells: vec![vec![Cell::default(); matrix.columns.len()]; matrix.rows.len()],
        cache_unknown: plan.cache_unknown,
    };
    let rows = matrix.rows.iter().zip(&matrix.cells).enumerate();
    for (r, (row, cells)) in rows {
        let columns = matrix.columns.iter().zip(cells).enumerate();
        for (c, (&column, &cell)) in columns {
            let node = row.node;
            ctx.progress(format!("线程在节点 {node}、内存在节点 {column} 的带宽测试"));
            let job = bandwidth(&multiload, row, column, cell);
            let place = (node, column, Kind::Bandwidth);
            output.cells[r][c].bandwidth =
                Some(measured(ctx, &run, &job, parse::bandwidth, place).await?);
            ctx.publish(&output);
            ctx.progress(format!("线程在节点 {node}、内存在节点 {column} 的延迟测试"));
            let job = latency(multichase, row, column, cell);
            let place = (node, column, Kind::Latency);
            output.cells[r][c].latency =
                Some(measured(ctx, &run, &job, parse::latency, place).await?);
            ctx.publish(&output);
        }
    }
    Ok(output)
}

/// One run of the cell of threads on `row`, memory on `column`, tried as every run is: its value,
/// or why the plugin stops.
async fn measured<F, Fut>(
    ctx: &Context<Output>,
    run: &F,
    job: &Job,
    read: fn(&job::Output) -> Result<crate::Value, Fault>,
    (row, column, kind): (usize, usize, Kind),
) -> Result<Measured, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    match crate::tried(ctx, run, job, read).await {
        Tried::Done(value, retries) => Ok(Measured { value, retries }),
        Tried::Failed(faults) if faults.last() == Some(&Fault::Refused) => {
            Err(Stop::Skipped(Failure::Refused))
        }
        Tried::Failed(faults) => Err(Stop::Failed(Failure::Run {
            row,
            column,
            kind,
            faults,
        })),
        Tried::Cancelled => Err(Stop::Failed(Failure::Cancelled)),
    }
}
