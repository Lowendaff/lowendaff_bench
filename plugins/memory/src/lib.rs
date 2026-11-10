//! 内存性能 (design memory): sysbench's read and write bandwidth, STREAM, the latency of each
//! cache level and of memory, and on machines with several NUMA nodes the bandwidth and latency
//! between them, each in a chapter of its own.

pub mod output;
pub mod plan;
pub mod process;
pub mod report;

use std::collections::BTreeSet;
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_host::Root;
use plugin_cpu::SYSBENCH;
use serde::Deserialize;

use crate::output::AttemptError;
use crate::output::KERNELS;
use crate::output::Output;
use crate::plan::Host;
use crate::plan::Plan;
use crate::process::Job;

/// STREAM 5.10 from the release `stream-5.10-1`: six programs of one size each, the smallest
/// named; the plugin runs the others beside it (design memory, §4, §5.1).
pub const STREAM: ToolSpec = ToolSpec {
    name: "stream",
    version: "5.10",
    form: ToolForm::TarGz {
        executable: "stream-2m",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "stream-5.10-1/stream-x86_64.tar.gz"
            ),
            sha256: "cd0bba47b10c6ef5e02cb7a5218ad65183d83f23061b0cc71cf0128b0a1e585f",
            size: 1_609_410,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "stream-5.10-1/stream-aarch64.tar.gz"
            ),
            sha256: "3f422aef2895bd424071c0d2d398c379c2cb391ac671b018ec3b033087c43fa7",
            size: 1_686_248,
        },
    ],
};

/// multichase and multiload from the release `multichase-2026.10.01-1` (design memory, §4).
pub const MULTICHASE: ToolSpec = ToolSpec {
    name: "multichase",
    version: "2026.10.01",
    form: ToolForm::TarGz {
        executable: "multichase",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "multichase-2026.10.01-1/multichase-x86_64.tar.gz"
            ),
            sha256: "8a2cb5f805c2faa194852e2504f7e563592ced3d3a5da74c04b14428d4bc00d0",
            size: 976_234,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "multichase-2026.10.01-1/multichase-aarch64.tar.gz"
            ),
            sha256: "5d4188157c5579c88994a9de8df5ddcd96cfef639da5510066a61f2892aee915",
            size: 885_268,
        },
    ],
};

/// Attempts per run: the first one and 3 retries (§5.3, §5.4).
pub const ATTEMPTS: usize = 4;

/// The half-second samples of each multichase run: 2.5 seconds (§5.5).
pub const LATENCY_SAMPLES: u32 = 5;

/// The samples of each multiload run (§5.6).
pub const BANDWIDTH_SAMPLES: u32 = 3;

/// A sysbench run lasts 10 seconds; the rest is room to start and to stop (§5.3).
const SYSBENCH_LIMIT: Duration = Duration::from_secs(60);

/// A STREAM run with the largest arrays takes seconds; the limit is the design's (§5.4).
const STREAM_LIMIT: Duration = Duration::from_secs(180);

/// A multichase or multiload run samples for a few seconds after filling its buffer.
const CHASE_LIMIT: Duration = Duration::from_secs(60);

/// 内存性能. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Memory {}

impl Plugin for Memory {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "memory",
        title: "内存性能",
        version: "0.1.0",
        requires: Requirements {
            tools: &[SYSBENCH, STREAM, MULTICHASE],
            ..Requirements::NONE
        },
        // Enough for the matrix of up to 13 nodes (§5.1).
        timeout: Duration::from_secs(30 * 60),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let tools = Tools {
            sysbench: ctx.tool(SYSBENCH.name)?.to_path_buf(),
            stream: ctx.tool(STREAM.name)?.to_path_buf(),
            multichase: ctx.tool(MULTICHASE.name)?.to_path_buf(),
        };
        let kind = ctx.env().platform.kind();
        let host = tokio::task::spawn_blocking(move || Host::read(&Root::system(), kind))
            .await?
            .ok_or_else(|| PluginError::fail("读不到可用内存（/proc/meminfo）"))?;
        measure(ctx, &tools, &Plan::new(&host), |job| process::run(ctx, job)).await
    }
}

/// The programs of the three tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    /// sysbench.
    pub sysbench: PathBuf,
    /// STREAM's smallest size; the other sizes are beside it.
    pub stream: PathBuf,
    /// multichase; multiload is beside it.
    pub multichase: PathBuf,
}

/// Runs the four chapters of `plan` and emits the records; `run` runs one job (§5.8). A chapter
/// that fails leaves the others running, and the item then fails.
pub async fn measure<F, Fut>(
    ctx: &Context,
    tools: &Tools,
    plan: &Plan,
    run: F,
) -> Result<(), PluginError>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<Output, AttemptError>>,
{
    if plan.cache_unknown {
        ctx.emit(Record::fact("cache-note", "说明", report::CACHE_UNKNOWN));
    }
    let mut failed = Vec::new();
    for chapter in ["sysbench", "STREAM", "延迟", "NUMA"] {
        let end = match chapter {
            "sysbench" => sysbench(ctx, tools, plan, &run).await?,
            "STREAM" => stream(ctx, tools, plan, &run).await?,
            "延迟" => latency(ctx, tools, plan, &run).await?,
            _ => numa(ctx, tools, plan, &run).await?,
        };
        match end {
            End::Done => {}
            End::Failed => failed.push(chapter),
            End::Cancelled => return Err(PluginError::fail("已中断")),
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(PluginError::fail(report::failed(&failed)))
    }
}

/// How a chapter ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// It measured, or had nothing to measure.
    Done,
    /// A run failed every attempt.
    Failed,
    /// The item was cancelled.
    Cancelled,
}

/// One run of a chapter: its name in notes, its job, and how its output is read.
struct Step<T> {
    name: String,
    job: Job,
    read: fn(&Output) -> Result<T, AttemptError>,
}

/// Why a chapter stopped before its last run.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stop {
    /// A run failed every attempt, for this reason.
    Failed(String),
    /// The kernel would not bind memory to a node.
    Refused,
    /// The item was cancelled.
    Cancelled,
}

/// The results of a chapter's runs, in order, up to the run that stopped it.
struct Runs<T> {
    values: Vec<T>,
    /// Notes on runs that succeeded after retries.
    notes: Vec<String>,
    stop: Option<Stop>,
}

impl<T> Runs<T> {
    /// Whether there is anything to show in a table.
    fn measured(&self) -> bool {
        !self.values.is_empty() && self.stop != Some(Stop::Refused)
    }
}

/// Runs the steps of `list` in order, each up to [`ATTEMPTS`] times, until one fails every
/// attempt, is refused, or is cancelled.
async fn run_steps<T, F, Fut>(run: &F, list: Vec<Step<T>>) -> Runs<T>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<Output, AttemptError>>,
{
    let mut runs = Runs {
        values: Vec::new(),
        notes: Vec::new(),
        stop: None,
    };
    for step in list {
        let mut failures = Vec::new();
        loop {
            let attempt = run(step.job.clone())
                .await
                .and_then(|output| (step.read)(&output));
            match attempt {
                Ok(value) => {
                    runs.notes.extend(report::retried(&step.name, &failures));
                    runs.values.push(value);
                    break;
                }
                Err(AttemptError::Failed(reason)) => {
                    failures.push(reason);
                    if failures.len() == ATTEMPTS {
                        runs.stop = Some(Stop::Failed(report::failure(&step.name, &failures)));
                        return runs;
                    }
                }
                Err(AttemptError::Refused) => {
                    runs.stop = Some(Stop::Refused);
                    return runs;
                }
                Err(AttemptError::Cancelled) => {
                    runs.stop = Some(Stop::Cancelled);
                    return runs;
                }
            }
        }
    }
    runs
}

/// Emits a chapter's fact: its parameters, or why it stopped; and says how it ended.
fn chapter(ctx: &Context, id: &str, label: &str, parameters: String, stop: Option<&Stop>) -> End {
    let (value, end) = match stop {
        None => (parameters, End::Done),
        Some(Stop::Failed(reason)) => (format!("失败：{reason}"), End::Failed),
        Some(Stop::Refused) => (report::REFUSED.to_owned(), End::Done),
        Some(Stop::Cancelled) => ("已中断".to_owned(), End::Cancelled),
    };
    ctx.emit(Record::fact(id, label, value));
    end
}

/// Emits a chapter's notes, numbered after its id.
fn notes(ctx: &Context, id: &str, notes: Vec<String>) {
    for (number, note) in notes.into_iter().enumerate() {
        ctx.emit(Record::fact(
            &format!("{id}-note-{}", number + 1),
            "说明",
            note,
        ));
    }
}

/// sysbench's chapter (§5.3): reading and writing, with one thread and with all of them.
async fn sysbench<F, Fut>(
    ctx: &Context,
    tools: &Tools,
    plan: &Plan,
    run: &F,
) -> Result<End, PluginError>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<Output, AttemptError>>,
{
    let columns = plan.columns();
    let operations = [("read", "读取"), ("write", "写入")];
    let mut list = Vec::new();
    for (operation, word) in operations {
        for &threads in &columns {
            let block = if threads > 1 {
                plan.per_thread
            } else {
                plan.single
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
                format!("--time={}", report::SYSBENCH_SECONDS),
                "run".to_owned(),
            ]);
            list.push(Step {
                name: format!("{}{word}", report::threads_word(threads)),
                job: Job {
                    tool: "sysbench",
                    program: tools.sysbench.clone(),
                    args,
                    envs: Vec::new(),
                    cpus: None,
                    limit: SYSBENCH_LIMIT,
                },
                read: output::sysbench,
            });
        }
    }
    let runs = run_steps(run, list).await;
    let per_thread = (plan.threads > 1).then_some(plan.per_thread);
    let parameters = report::sysbench(SYSBENCH.version, plan.single, per_thread);
    let end = chapter(ctx, "sysbench", "sysbench", parameters, runs.stop.as_ref());
    let mut chapter_notes = Vec::new();
    if runs.measured() {
        let mut headers = vec!["项目".to_owned()];
        headers.extend(columns.iter().map(|&threads| report::column(threads)));
        let rows = operations
            .iter()
            .enumerate()
            .map(|(row, (_, word))| {
                let mut cells = vec![Some((*word).to_owned())];
                cells.extend((0..columns.len()).map(|column| {
                    let value = runs.values.get(row * columns.len() + column)?;
                    Some(format!("{} MiB/s", value.text))
                }));
                cells
            })
            .collect();
        ctx.emit(report::table(
            "sysbench-results",
            "sysbench 结果",
            headers,
            rows,
        )?);
        let all = plan.per_thread.saturating_mul(plan.threads as u64);
        let small = plan.single < plan.cache.saturating_mul(4)
            || (plan.threads > 1 && all < plan.cache.saturating_mul(4));
        if small {
            chapter_notes.push(report::sysbench_small(plan.cache));
        }
    }
    chapter_notes.extend(runs.notes);
    notes(ctx, "sysbench", chapter_notes);
    Ok(end)
}

/// STREAM's chapter (§5.4): its four kernels with one thread and with all of them.
async fn stream<F, Fut>(
    ctx: &Context,
    tools: &Tools,
    plan: &Plan,
    run: &F,
) -> Result<End, PluginError>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<Output, AttemptError>>,
{
    let Some(millions) = plan.stream else {
        let short = report::stream_short(plan.budget);
        ctx.emit(Record::fact("stream", "STREAM", short));
        return Ok(End::Done);
    };
    let columns = plan.columns();
    let program = tools.stream.with_file_name(format!("stream-{millions}m"));
    let list = columns
        .iter()
        .map(|&threads| {
            let mut envs = vec![("OMP_NUM_THREADS", threads.to_string())];
            if threads > 1 {
                envs.push(("OMP_PROC_BIND", "spread".to_owned()));
            }
            Step {
                name: format!("{}测试", report::threads_word(threads)),
                job: Job {
                    tool: "STREAM",
                    program: program.clone(),
                    args: Vec::new(),
                    envs,
                    cpus: None,
                    limit: STREAM_LIMIT,
                },
                read: output::stream,
            }
        })
        .collect();
    let runs = run_steps(run, list).await;
    let parameters = report::stream(STREAM.version, millions, plan.threads > 1);
    let end = chapter(ctx, "stream", "STREAM", parameters, runs.stop.as_ref());
    let mut chapter_notes = Vec::new();
    if runs.measured() {
        let mut headers = vec!["项目".to_owned()];
        headers.extend(columns.iter().map(|&threads| report::column(threads)));
        let rows = KERNELS
            .iter()
            .enumerate()
            .map(|(kernel, name)| {
                let mut cells = vec![Some((*name).to_owned())];
                cells.extend((0..columns.len()).map(|column| {
                    let rates = runs.values.get(column)?;
                    Some(format!("{} MB/s", rates[kernel].text))
                }));
                cells
            })
            .collect();
        ctx.emit(report::table(
            "stream-results",
            "STREAM 结果",
            headers,
            rows,
        )?);
        if plan::array_bytes(millions) < plan.cache.saturating_mul(4) {
            chapter_notes.push(report::STREAM_RULE.to_owned());
        }
    }
    chapter_notes.extend(runs.notes);
    notes(ctx, "stream", chapter_notes);
    Ok(end)
}

/// The latency chapter (§5.5): one multichase run per cache level and one in memory.
async fn latency<F, Fut>(
    ctx: &Context,
    tools: &Tools,
    plan: &Plan,
    run: &F,
) -> Result<End, PluginError>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<Output, AttemptError>>,
{
    let list = plan
        .levels
        .iter()
        .map(|level| Step {
            name: report::spaced(level.name, "测试"),
            job: Job {
                tool: "multichase",
                program: tools.multichase.clone(),
                args: vec![
                    "-m".to_owned(),
                    level.buffer.to_string(),
                    "-n".to_owned(),
                    LATENCY_SAMPLES.to_string(),
                ],
                envs: Vec::new(),
                cpus: None,
                limit: CHASE_LIMIT,
            },
            read: output::multichase,
        })
        .collect();
    let runs = run_steps(run, list).await;
    let parameters = report::LATENCY.to_owned();
    let end = chapter(ctx, "latency", "延迟", parameters, runs.stop.as_ref());
    let mut chapter_notes = Vec::new();
    if runs.measured() {
        let headers = ["层级", "缓冲区", "延迟"].map(str::to_owned).to_vec();
        let rows = plan
            .levels
            .iter()
            .enumerate()
            .map(|(index, level)| {
                let value = runs.values.get(index);
                vec![
                    Some(level.name.to_owned()),
                    Some(report::size(level.buffer)),
                    value.map(|value| format!("{} ns", value.text)),
                ]
            })
            .collect();
        ctx.emit(report::table("latency-results", "延迟结果", headers, rows)?);
        if !plan.missing.is_empty() {
            chapter_notes.push(report::missing(&plan.missing));
        }
        if plan.single < plan.cache.saturating_mul(4) {
            chapter_notes.push(report::latency_small(plan.cache));
        }
    }
    chapter_notes.extend(runs.notes);
    notes(ctx, "latency", chapter_notes);
    Ok(end)
}

/// The NUMA chapter (§5.6): bandwidth and latency from each node's CPUs to each node's memory.
async fn numa<F, Fut>(
    ctx: &Context,
    tools: &Tools,
    plan: &Plan,
    run: &F,
) -> Result<End, PluginError>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<Output, AttemptError>>,
{
    let Some(matrix) = &plan.matrix else {
        return Ok(End::Done);
    };
    let multiload = tools.multichase.with_file_name("multiload");
    let mut list = Vec::new();
    for (row, cells) in matrix.rows.iter().zip(&matrix.cells) {
        for (column, cell) in matrix.columns.iter().zip(cells) {
            let place = format!("线程在节点 {}、内存在节点 {column} 的", row.node);
            let bind = format!("{column}:1");
            list.push(Step {
                name: format!("{place}带宽测试"),
                job: Job {
                    tool: "multiload",
                    program: multiload.clone(),
                    args: vec![
                        "-l".to_owned(),
                        "stream-triad".to_owned(),
                        "-n".to_owned(),
                        BANDWIDTH_SAMPLES.to_string(),
                        "-t".to_owned(),
                        row.cpus.len().to_string(),
                        "-m".to_owned(),
                        cell.per_thread.to_string(),
                        "-W".to_owned(),
                        bind.clone(),
                    ],
                    envs: Vec::new(),
                    cpus: Some(row.cpus.clone()),
                    limit: CHASE_LIMIT,
                },
                read: output::multiload,
            });
            list.push(Step {
                name: format!("{place}延迟测试"),
                job: Job {
                    tool: "multichase",
                    program: tools.multichase.clone(),
                    args: vec![
                        "-m".to_owned(),
                        cell.latency.to_string(),
                        "-n".to_owned(),
                        LATENCY_SAMPLES.to_string(),
                        "-W".to_owned(),
                        bind,
                    ],
                    envs: Vec::new(),
                    cpus: Some(row.cpus.clone()),
                    limit: CHASE_LIMIT,
                },
                read: output::multichase,
            });
        }
    }
    let runs = run_steps(run, list).await;
    let nodes: BTreeSet<usize> = matrix
        .rows
        .iter()
        .map(|row| row.node)
        .chain(matrix.columns.iter().copied())
        .collect();
    let end = chapter(
        ctx,
        "numa",
        "NUMA",
        report::numa(nodes.len()),
        runs.stop.as_ref(),
    );
    if runs.measured() {
        let mut headers = vec![String::new()];
        headers.extend(matrix.columns.iter().map(|node| format!("节点 {node}")));
        let tables = [
            (
                "numa-bandwidth",
                "NUMA 带宽（MiB/s；行是线程所在节点，列是内存所在节点）",
            ),
            ("numa-latency", "NUMA 延迟（ns）"),
        ];
        for (kind, (id, title)) in tables.into_iter().enumerate() {
            let rows = matrix
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let mut cells = vec![Some(format!("节点 {}", row.node))];
                    cells.extend((0..matrix.columns.len()).map(|column| {
                        let run = (index * matrix.columns.len() + column) * 2 + kind;
                        Some(runs.values.get(run)?.text.clone())
                    }));
                    cells
                })
                .collect();
            ctx.emit(report::table(id, title, headers.clone(), rows)?);
        }
    }
    notes(ctx, "numa", runs.notes);
    Ok(end)
}
