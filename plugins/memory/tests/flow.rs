//! The item's flow with a scripted runner in place of the tools (design memory, §5.3–§5.8):
//! the jobs it starts, what it shows, and failures, retries, cancellation and `mbind` refused.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Env;
use bench_api::PluginError;
use bench_api::Record;
use bench_host::cache::Cache;
use bench_host::cache::CacheKind;
use bench_host::numa::Node;
use plugin_memory::Tools;
use plugin_memory::output::AttemptError;
use plugin_memory::output::Output;
use plugin_memory::plan::GIB;
use plugin_memory::plan::Host;
use plugin_memory::plan::KIB;
use plugin_memory::plan::MIB;
use plugin_memory::plan::Plan;
use plugin_memory::process::Job;
use tokio::time::Instant;

struct Outcome {
    result: Result<(), PluginError>,
    records: Vec<Record>,
    jobs: Vec<Job>,
}

impl Outcome {
    /// The records as lines: `label = value` for facts; a table's title in brackets, then its
    /// header and rows with cells joined by ` | `.
    fn shown(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for record in &self.records {
            match record {
                Record::Fact { label, value, .. } => lines.push(format!("{label} = {value}")),
                Record::Table(table) => {
                    lines.push(format!("[{}]", table.title()));
                    lines.push(table.headers().join(" | "));
                    lines.extend(table.rows().iter().map(|row| row.join(" | ")));
                }
                Record::Metric { .. } => panic!("no metrics: {record:?}"),
            }
        }
        lines
    }

    fn failure(&self) -> &str {
        match &self.result {
            Err(PluginError::Fail(reason)) => reason,
            other => panic!("the item fails: {other:?}"),
        }
    }

    /// The jobs of one tool, in order.
    fn of(&self, tool: &str) -> Vec<&Job> {
        self.jobs.iter().filter(|job| job.tool == tool).collect()
    }
}

fn tools() -> Tools {
    Tools {
        sysbench: "/t/sysbench".into(),
        stream: "/t/stream/stream-2m".into(),
        multichase: "/t/multichase/multichase".into(),
    }
}

/// Runs the item on `host`. Each job is answered by `script` when it has an answer for the job
/// and the number of times the same job ran before, else as by [`fine`].
async fn measure(
    host: &Host,
    script: impl Fn(&Job, usize) -> Option<Result<Output, AttemptError>>,
) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(600);
    let (ctx, mut receiver) = Context::new(
        Arc::new(Env::default()),
        deadline,
        ".".into(),
        CancellationToken::new(),
    );
    let jobs = RefCell::new(Vec::new());
    let plan = Plan::new(host);
    let result = plugin_memory::measure(&ctx, &tools(), &plan, |job: Job| {
        let before = jobs.borrow().iter().filter(|seen| **seen == job).count();
        let answer = script(&job, before).unwrap_or_else(|| fine(&job));
        jobs.borrow_mut().push(job);
        async move { answer }
    })
    .await;
    let mut records = Vec::new();
    while let Ok(record) = receiver.try_recv() {
        records.push(record);
    }
    let ids: BTreeSet<&str> = records.iter().map(Record::id).collect();
    assert_eq!(
        ids.len(),
        records.len(),
        "record ids are unique: {records:?}"
    );
    Outcome {
        result,
        records,
        jobs: jobs.into_inner(),
    }
}

/// An argument's value, such as that of `-m`.
fn arg<'a>(job: &'a Job, name: &str) -> Option<&'a str> {
    let at = job.args.iter().position(|arg| arg == name)?;
    job.args.get(at + 1).map(String::as_str)
}

fn has(job: &Job, arg: &str) -> bool {
    job.args.iter().any(|seen| seen == arg)
}

/// What the tools print when every run works, with numbers that tell the runs apart.
fn fine(job: &Job) -> Result<Output, AttemptError> {
    match job.tool {
        "sysbench" => Ok(sysbench(
            match (has(job, "--memory-oper=read"), has(job, "--threads=1")) {
                (true, true) => "100.00",
                (true, false) => "400.00",
                (false, true) => "50.00",
                (false, false) => "200.00",
            },
        )),
        "STREAM" => Ok(stream(
            if job.envs.contains(&("OMP_NUM_THREADS", "1".to_owned())) {
                ["10000.1", "9000.2", "11000.3", "11500.4"]
            } else {
                ["40000.1", "36000.2", "44000.3", "46000.4"]
            },
        )),
        "multichase" => Ok(number(match (arg(job, "-W"), job.cpus.as_deref()) {
            (None, _) => match arg(job, "-m") {
                Some("16384") => "  1.500",
                Some("524288") => "  4.200",
                Some("4194304") => " 12.000",
                _ => " 90.500",
            },
            (Some("0:1"), Some([0, 1])) | (Some("1:1"), Some([2, 3])) => " 95.000",
            _ => " 140.1",
        })),
        "multiload" => Ok(load(match (arg(job, "-W"), job.cpus.as_deref()) {
            (Some("0:1"), Some([0, 1])) | (Some("1:1"), Some([2, 3])) => "52000",
            _ => "31000",
        })),
        tool => panic!("no such tool: {tool}"),
    }
}

fn sysbench(rate: &str) -> Output {
    Output {
        code: Some(0),
        text: format!(
            "Total operations: 1 (  1.00 per second)\n\n1.00 MiB transferred ({rate} MiB/sec)\n"
        ),
    }
}

fn stream(rates: [&str; 4]) -> Output {
    let [copy, scale, add, triad] = rates;
    Output {
        code: Some(0),
        text: format!(
            "Function    Best Rate MB/s  Avg time\nCopy:  {copy}  0.1\nScale:  {scale}  0.1\n\
             Add:  {add}  0.1\nTriad:  {triad}  0.1\n\
             Solution Validates: avg error less than 1.000000e-13 on all three arrays\n"
        ),
    }
}

fn number(text: &str) -> Output {
    Output {
        code: Some(0),
        text: format!("{text}\n"),
    }
}

fn load(mibs: &str) -> Output {
    Output {
        code: Some(0),
        text: format!(
            "Samples\t, Byte/thd\t, LoadThds\t, LdMaxMibs\t, LdAvgMibs\n3     \t, 1   \t, 2   \t, \
             {mibs}   \t, 1\n"
        ),
    }
}

fn exited(code: i32, text: &str) -> Option<Result<Output, AttemptError>> {
    Some(Ok(Output {
        code: Some(code),
        text: text.to_owned(),
    }))
}

fn cache(level: u32, kind: CacheKind, size: u64, cpus: &[usize]) -> Cache {
    Cache {
        level,
        kind,
        size,
        cpus: cpus.to_vec(),
    }
}

/// Four CPUs with an L1d and an L2 each and one L3 of 8 MiB, 8 GiB available, one node.
fn host() -> Host {
    let mut caches: Vec<Cache> = (0..4)
        .flat_map(|cpu| {
            [
                cache(1, CacheKind::Data, 32 * KIB, &[cpu]),
                cache(1, CacheKind::Instruction, 32 * KIB, &[cpu]),
                cache(2, CacheKind::Unified, MIB, &[cpu]),
            ]
        })
        .collect();
    caches.push(cache(3, CacheKind::Unified, 8 * MIB, &[0, 1, 2, 3]));
    Host {
        available: 8 * GIB,
        threads: 4,
        caches,
        nodes: vec![Node {
            id: 0,
            cpus: vec![0, 1, 2, 3],
            total: Some(16 * GIB),
            free: Some(8 * GIB),
        }],
        cpus: Some(vec![0, 1, 2, 3]),
        mems: Some(vec![0]),
    }
}

/// [`host`] with its CPUs on two nodes, each with an L3 of 8 MiB.
fn two_nodes() -> Host {
    let mut host = host();
    host.caches.pop();
    host.caches
        .push(cache(3, CacheKind::Unified, 8 * MIB, &[0, 1]));
    host.caches
        .push(cache(3, CacheKind::Unified, 8 * MIB, &[2, 3]));
    host.nodes = (0..2)
        .map(|id| Node {
            id,
            cpus: vec![2 * id, 2 * id + 1],
            total: Some(16 * GIB),
            free: Some(8 * GIB),
        })
        .collect();
    host.mems = Some(vec![0, 1]);
    host
}

/// The lines of the latency chapter on [`host`].
const LATENCY: [&str; 7] = [
    "延迟 = multichase：随机指针追逐，每档 2.5 秒",
    "[延迟结果]",
    "层级 | 缓冲区 | 延迟",
    "L1 | 16 KiB | 1.500 ns",
    "L2 | 512 KiB | 4.200 ns",
    "L3 | 4 MiB | 12.000 ns",
    "内存 | 32 MiB | 90.500 ns",
];

#[tokio::test]
async fn a_machine_of_one_node_shows_three_chapters() {
    let outcome = measure(&host(), |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let mut expected = vec![
        "sysbench = 1.0.20：顺序读写，每项 10 秒；单线程一块 32 MiB，多线程每个线程一块 8 MiB",
        "[sysbench 结果]",
        "项目 | 单线程 | 多线程（4 线程）",
        "读取 | 100.00 MiB/s | 400.00 MiB/s",
        "写入 | 50.00 MiB/s | 200.00 MiB/s",
        "STREAM = 5.10：每个数组 800 万个元素（61 MiB，三个合计 183 MiB），\
         每项取 10 遍中最快的一遍；\
         多线程按节点均匀绑定",
        "[STREAM 结果]",
        "项目 | 单线程 | 多线程（4 线程）",
        "Copy | 10000.1 MB/s | 40000.1 MB/s",
        "Scale | 9000.2 MB/s | 36000.2 MB/s",
        "Add | 11000.3 MB/s | 44000.3 MB/s",
        "Triad | 11500.4 MB/s | 46000.4 MB/s",
    ];
    expected.extend(LATENCY);
    assert_eq!(outcome.shown(), expected);

    let sysbench = outcome.of("sysbench");
    assert_eq!(sysbench.len(), 4);
    assert_eq!(
        sysbench[0].args,
        [
            "memory",
            "--memory-block-size=33554432",
            "--memory-total-size=0",
            "--memory-oper=read",
            "--memory-access-mode=seq",
            "--threads=1",
            "--time=10",
            "run",
        ]
    );
    assert_eq!(
        sysbench[3].args,
        [
            "memory",
            "--memory-block-size=8388608",
            "--memory-total-size=0",
            "--memory-oper=write",
            "--memory-access-mode=seq",
            "--memory-scope=local",
            "--threads=4",
            "--time=10",
            "run",
        ]
    );
    assert!(sysbench.iter().all(|job| job.program == tools().sysbench));
    let stream = outcome.of("STREAM");
    assert_eq!(stream.len(), 2);
    assert!(
        stream
            .iter()
            .all(|job| job.program.ends_with("stream/stream-8m"))
    );
    assert_eq!(stream[0].envs, [("OMP_NUM_THREADS", "1".to_owned())]);
    assert_eq!(
        stream[1].envs,
        [
            ("OMP_NUM_THREADS", "4".to_owned()),
            ("OMP_PROC_BIND", "spread".to_owned()),
        ]
    );
    let latency: Vec<&[String]> = outcome
        .of("multichase")
        .iter()
        .map(|job| job.args.as_slice())
        .collect();
    assert_eq!(
        latency,
        [
            ["-m", "16384", "-n", "5"],
            ["-m", "524288", "-n", "5"],
            ["-m", "4194304", "-n", "5"],
            ["-m", "33554432", "-n", "5"],
        ]
    );
    assert!(
        outcome.jobs.iter().all(|job| job.cpus.is_none()),
        "nothing is placed"
    );
    assert!(
        outcome.of("multiload").is_empty(),
        "no matrix with one node"
    );
}

#[tokio::test]
async fn one_thread_runs_the_single_thread_column_only() {
    let mut host = host();
    host.threads = 1;
    let outcome = measure(&host, |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert_eq!(
        shown[..5],
        [
            "sysbench = 1.0.20：顺序读写，每项 10 秒；单线程一块 32 MiB",
            "[sysbench 结果]",
            "项目 | 单线程",
            "读取 | 100.00 MiB/s",
            "写入 | 50.00 MiB/s",
        ]
    );
    assert_eq!(
        shown[5],
        "STREAM = 5.10：每个数组 800 万个元素（61 MiB，三个合计 183 MiB），每项取 10 遍中最快的一遍"
    );
    assert_eq!(shown[8], "Copy | 10000.1 MB/s");
    assert_eq!(outcome.of("sysbench").len(), 2);
    assert_eq!(outcome.of("STREAM").len(), 1);
}

#[tokio::test]
async fn a_chapter_that_fails_leaves_the_others_running_and_fails_the_item() {
    let outcome = measure(&host(), |job, _| {
        (job.tool == "STREAM").then(|| {
            Ok(Output {
                code: Some(1),
                text: "boom\n".to_owned(),
            })
        })
    })
    .await;
    assert_eq!(outcome.failure(), "STREAM 出错");
    let shown = outcome.shown();
    assert_eq!(
        shown[5],
        "STREAM = 失败：单线程测试：STREAM 退出码 1：boom（共试了 4 次）"
    );
    assert_eq!(
        shown[6..],
        LATENCY,
        "no STREAM table; the latency chapter runs"
    );
    assert_eq!(
        outcome.of("STREAM").len(),
        4,
        "the multi-thread run never starts"
    );
}

#[tokio::test]
async fn runs_that_succeeded_after_retries_get_notes() {
    let outcome = measure(&host(), |job, before| {
        let write = has(job, "--memory-oper=write") && has(job, "--threads=4");
        if write && before < 2 {
            return Some(Err(AttemptError::Failed(
                "超过 60 秒仍未结束，已停止".to_owned(),
            )));
        }
        let multi = job.tool == "STREAM" && job.envs.len() == 2;
        (multi && before == 0).then(|| {
            exited(
                0,
                "Failed Validation on array a[], AvgRelAbsErr > epsilon (1.000000e-13)\n",
            )
            .expect("an output")
        })
    })
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert_eq!(
        shown[5],
        "说明 = 多线程写入失败 2 次后成功，最后一次失败的原因：超过 60 秒仍未结束，已停止"
    );
    assert_eq!(
        shown[13],
        "说明 = 多线程测试失败 1 次后成功，最后一次失败的原因：STREAM 的结果没有通过校验：\
         Failed Validation on array a[], AvgRelAbsErr > epsilon (1.000000e-13)"
    );
}

#[tokio::test]
async fn a_cancellation_stops_the_item_and_keeps_what_was_measured() {
    let outcome = measure(&host(), |job, _| {
        (job.tool == "STREAM" && job.envs.len() == 2).then_some(Err(AttemptError::Cancelled))
    })
    .await;
    assert_eq!(outcome.failure(), "已中断");
    assert_eq!(
        outcome.shown()[5..],
        [
            "STREAM = 已中断",
            "[STREAM 结果]",
            "项目 | 单线程 | 多线程（4 线程）",
            "Copy | 10000.1 MB/s | —",
            "Scale | 9000.2 MB/s | —",
            "Add | 11000.3 MB/s | —",
            "Triad | 11500.4 MB/s | —",
        ]
    );
    assert!(
        outcome.of("multichase").is_empty(),
        "no chapter runs after a cancellation"
    );
    assert_eq!(
        outcome.of("STREAM").len(),
        2,
        "a cancellation is not retried"
    );
}

#[tokio::test]
async fn the_matrix_runs_on_each_nodes_cpus_with_memory_bound_to_each_node() {
    let outcome = measure(&two_nodes(), |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert_eq!(
        shown[shown.len() - 9..],
        [
            "NUMA = 2 个节点；带宽用 multiload 的 stream-triad，每格 3 次采样取最快；延迟同上",
            "[NUMA 带宽（MiB/s；行是线程所在节点，列是内存所在节点）]",
            " | 节点 0 | 节点 1",
            "节点 0 | 52000 | 31000",
            "节点 1 | 31000 | 52000",
            "[NUMA 延迟（ns）]",
            " | 节点 0 | 节点 1",
            "节点 0 | 95.000 | 140.1",
            "节点 1 | 140.1 | 95.000",
        ]
    );
    let placed: Vec<(&str, &[usize], &[String])> = outcome
        .jobs
        .iter()
        .filter_map(|job| Some((job.tool, job.cpus.as_deref()?, job.args.as_slice())))
        .collect();
    let bandwidth = |node: &str| {
        [
            "-l",
            "stream-triad",
            "-n",
            "3",
            "-t",
            "2",
            "-m",
            "16777216",
            "-W",
            node,
        ]
        .map(str::to_owned)
        .to_vec()
    };
    let latency = |node: &str| {
        ["-m", "33554432", "-n", "5", "-W", node]
            .map(str::to_owned)
            .to_vec()
    };
    assert_eq!(placed.len(), 8, "two runs in each of four cells");
    assert_eq!(
        placed[..4],
        [
            ("multiload", &[0, 1][..], &bandwidth("0:1")[..]),
            ("multichase", &[0, 1][..], &latency("0:1")[..]),
            ("multiload", &[0, 1][..], &bandwidth("1:1")[..]),
            ("multichase", &[0, 1][..], &latency("1:1")[..]),
        ]
    );
    assert_eq!(placed[4].1, [2, 3], "the second row's threads");
    assert!(
        outcome
            .of("multiload")
            .iter()
            .all(|job| job.program == tools().multichase.with_file_name("multiload"))
    );
}

#[tokio::test]
async fn a_refused_binding_leaves_the_numa_chapter_unavailable_without_failing() {
    let outcome = measure(&two_nodes(), |job, _| {
        (job.tool == "multiload")
            .then(|| exited(1, "mbind: Operation not permitted\n").expect("an output"))
    })
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.shown().last().map(String::as_str),
        Some("NUMA = 不可用：容器不允许把内存绑定到指定节点")
    );
    assert_eq!(outcome.of("multiload").len(), 1, "a refusal is not retried");
}

#[tokio::test]
async fn too_little_memory_shrinks_the_runs_and_leaves_stream_out() {
    let mut host = host();
    host.available = 32 * MIB;
    let outcome = measure(&host, |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert_eq!(
        shown[0],
        "sysbench = 1.0.20：顺序读写，每项 10 秒；单线程一块 16 MiB，多线程每个线程一块 4 MiB"
    );
    assert_eq!(
        shown[5..7],
        [
            "说明 = 可用内存不足，测试数据小于末级缓存总和的 4 倍（32 MiB），\
             一部分读写落在缓存里，结果偏高",
            "STREAM = 可用内存不足：三个数组最少要 46 MiB，可用内存的一半只有 16 MiB",
        ]
    );
    assert_eq!(shown[13], "内存 | 16 MiB | 90.500 ns");
    assert_eq!(
        shown[14],
        "说明 = 可用内存不足，内存一档的缓冲区小于末级缓存总和的 4 倍（32 MiB），\
         一部分访问落在缓存里，延迟偏低"
    );
    assert!(outcome.of("STREAM").is_empty());
}

#[tokio::test]
async fn a_refusal_after_measured_cells_still_leaves_the_numa_chapter_unavailable() {
    let outcome = measure(&two_nodes(), |job, _| {
        (job.tool == "multiload" && arg(job, "-W") == Some("1:1"))
            .then(|| exited(1, "mbind: Operation not permitted\n").expect("an output"))
    })
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert_eq!(
        shown.last().map(String::as_str),
        Some("NUMA = 不可用：容器不允许把内存绑定到指定节点")
    );
    assert!(
        !shown.iter().any(|line| line.starts_with("[NUMA")),
        "no table of a matrix that could not be finished: {shown:?}"
    );
}

#[tokio::test]
async fn a_level_the_kernel_says_nothing_of_gets_a_note() {
    let mut host = host();
    host.caches.retain(|cache| cache.level < 3);
    let outcome = measure(&host, |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.shown().last().map(String::as_str),
        Some("说明 = 没有 L3 的信息，跳过这一档")
    );
}

#[tokio::test]
async fn unknown_caches_count_as_32_mib_with_notes() {
    let mut host = host();
    host.caches.clear();
    let outcome = measure(&host, |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert_eq!(shown[0], "说明 = 没有末级缓存的信息，按 32 MiB 计算");
    assert_eq!(
        shown[shown.len() - 4..],
        [
            "[延迟结果]",
            "层级 | 缓冲区 | 延迟",
            "内存 | 128 MiB | 90.500 ns",
            "说明 = 没有 L1、L2、L3 的信息，跳过这三档",
        ]
    );
}

#[tokio::test]
async fn stream_at_exactly_four_times_the_caches_meets_the_rule() {
    let mut host = host();
    host.caches.pop();
    host.caches
        .push(cache(3, CacheKind::Unified, 16_000_000, &[0, 1, 2, 3]));
    let outcome = measure(&host, |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    assert!(
        shown
            .iter()
            .any(|line| line.contains("800 万个元素（61 MiB")),
        "arrays of 64 MB for caches of 16 MB: {shown:?}"
    );
    assert!(
        !shown
            .iter()
            .any(|line| line.contains("未完全符合 STREAM 运行规则")),
        "{shown:?}"
    );
}

#[tokio::test]
async fn stream_below_four_times_the_caches_is_labelled() {
    let mut host = host();
    host.available = 300 * MIB;
    host.caches
        .push(cache(3, CacheKind::Unified, 40 * MIB, &[0, 1, 2, 3]));
    let outcome = measure(&host, |_, _| None).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let shown = outcome.shown();
    let rule = shown
        .iter()
        .position(|line| {
            line == "说明 = 未完全符合 STREAM 运行规则（每个数组至少为末级缓存总和的 4 倍）"
        })
        .expect("the run rule's note");
    assert!(
        shown[rule - 1].starts_with("Triad | "),
        "it follows STREAM's table"
    );
}
