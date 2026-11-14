//! sysbench's chapter with recorded runs in place of sysbench: the runs it starts and their
//! blocks, what it returns, publishes and logs, retries, failures and a cancellation.

mod common;

use std::path::Path;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_memory::plan::MIB;
use plugin_memory::plan::Plan;
use plugin_memory::sysbench::Failure;
use plugin_memory::sysbench::Fault;
use plugin_memory::sysbench::Output;
use plugin_memory::sysbench::measure;
use plugin_memory::sysbench::result::Operation;

const SYSBENCH: &str = "/t/sysbench";

struct Measured {
    result: Result<Output, Stop<Failure>>,
    published: Option<Output>,
    progress: Vec<String>,
    jobs: Vec<Job>,
}

/// Measures as `plan` says; a job is answered by `script` when it has an answer for the job and
/// the number of times the same job ran before, else as by [`fine`].
async fn measure(
    plan: Option<&Plan>,
    script: impl Fn(&Job, usize) -> Option<Result<job::Output, RunError>>,
) -> Measured {
    let (ctx, mut channels) = common::context(&CancellationToken::new());
    let recorder =
        common::Recorder::new(|job: &Job, before| script(job, before).unwrap_or_else(|| fine(job)));
    let result = measure::measure(&ctx, Path::new(SYSBENCH), plan, |job| recorder.run(job)).await;
    let (published, progress) = common::drained(&mut channels);
    Measured {
        result,
        published,
        progress,
        jobs: recorder.jobs(),
    }
}

/// What sysbench prints when a run works, with rates that tell the runs apart.
fn fine(job: &Job) -> Result<job::Output, RunError> {
    let rate = match (
        common::has(job, "--memory-oper=read"),
        common::has(job, "--threads=1"),
    ) {
        (true, true) => "100.00",
        (true, false) => "400.00",
        (false, true) => "50.00",
        (false, false) => "200.00",
    };
    common::exited(
        0,
        &format!(
            "sysbench 1.0.20 (using bundled LuaJIT 2.1.0-beta2)\n\n\
             1.00 MiB transferred ({rate} MiB/sec)\n"
        ),
    )
}

/// The rates of the runs that worked, by operation and threads.
fn rates(output: &Output) -> Vec<(Operation, usize, &str)> {
    output
        .runs
        .iter()
        .map(|run| (run.operation, run.threads, run.rate.text.as_str()))
        .collect()
}

#[tokio::test]
async fn reading_then_writing_with_one_thread_and_with_all() {
    let plan = Plan::new(&common::host());
    let measured = measure(Some(&plan), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.version.as_deref(), Some("1.0.20"));
    assert_eq!(
        (output.single, output.per_thread, output.threads),
        (32 * MIB, Some(8 * MIB), 4)
    );
    assert_eq!((output.small, output.cache_unknown), (None, false));
    assert_eq!(
        rates(output),
        [
            (Operation::Read, 1, "100.00"),
            (Operation::Read, 4, "400.00"),
            (Operation::Write, 1, "50.00"),
            (Operation::Write, 4, "200.00"),
        ]
    );
    assert!(output.runs.iter().all(|run| run.retries.is_empty()));
    assert_eq!(measured.published.as_ref(), Some(output));
    assert_eq!(
        measured.progress,
        [
            "单线程读取",
            "多线程读取（4 线程）",
            "单线程写入",
            "多线程写入（4 线程）"
        ]
    );
    assert_eq!(measured.jobs.len(), 4);
    assert_eq!(
        common::args(&measured.jobs[0]),
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
        common::args(&measured.jobs[3]),
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
    assert_eq!(
        measured.jobs[0],
        measure::job(Path::new(SYSBENCH), Operation::Read, 1, 32 * MIB),
        "the whole job: sysbench itself, within its limit, quotable, not placed"
    );
    assert_eq!(measured.jobs[0].limit, Duration::from_secs(60));
}

#[tokio::test]
async fn one_thread_runs_the_single_thread_column_only() {
    let mut host = common::host();
    host.threads = 1;
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!((output.per_thread, output.threads), (None, 1));
    assert_eq!(
        rates(output),
        [
            (Operation::Read, 1, "100.00"),
            (Operation::Write, 1, "50.00")
        ]
    );
    assert_eq!(measured.progress, ["单线程读取", "单线程写入"]);
}

#[tokio::test]
async fn a_run_that_fails_every_attempt_fails_the_chapter_and_keeps_the_runs_before() {
    let measured = measure(Some(&Plan::new(&common::host())), |job, _| {
        let write = common::has(job, "--memory-oper=write");
        (write && common::has(job, "--threads=1")).then(|| common::exited(1, "boom\n"))
    })
    .await;
    let boom = Fault::Tool(common::exit(1, "boom"));
    assert_eq!(
        measured.result,
        Err(Stop::Failed(Failure::Run {
            operation: Operation::Write,
            threads: 1,
            faults: vec![boom; 4],
        }))
    );
    assert_eq!(measured.jobs.len(), 2 + 4, "the last run never starts");
    let published = measured.published.expect("the reads were published");
    assert_eq!(
        rates(&published),
        [
            (Operation::Read, 1, "100.00"),
            (Operation::Read, 4, "400.00")
        ]
    );
}

#[tokio::test]
async fn a_run_that_works_after_failed_attempts_keeps_their_faults() {
    let timed_out = job::Fault::TimedOut(Duration::from_secs(60));
    let measured = measure(Some(&Plan::new(&common::host())), |job, before| {
        let write = common::has(job, "--memory-oper=write") && common::has(job, "--threads=4");
        (write && before < 2).then(|| Err(RunError::Fault(timed_out.clone())))
    })
    .await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(
        output.runs[3].retries,
        [Fault::Tool(timed_out.clone()), Fault::Tool(timed_out)]
    );
    let none: usize = output.runs[..3].iter().map(|run| run.retries.len()).sum();
    assert_eq!(none, 0);
}

#[tokio::test]
async fn a_cancellation_ends_the_chapter_at_once() {
    let measured = measure(Some(&Plan::new(&common::host())), |job, _| {
        common::has(job, "--threads=4").then_some(Err(RunError::Cancelled))
    })
    .await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(measured.jobs.len(), 2, "a cancellation is not retried");
    let published = measured.published.expect("the first run was published");
    assert_eq!(rates(&published), [(Operation::Read, 1, "100.00")]);
}

#[tokio::test]
async fn too_little_memory_shrinks_the_blocks_and_says_so() {
    let mut host = common::host();
    host.available = 32 * MIB;
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(
        (output.single, output.per_thread),
        (16 * MIB, Some(4 * MIB))
    );
    assert_eq!(output.small, Some(32 * MIB), "four times the 8 MiB of L3");

    let mut one = common::host();
    one.threads = 1;
    one.available = 64 * MIB;
    let measured = measure(Some(&Plan::new(&one)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.single, 32 * MIB);
    assert_eq!(
        output.small, None,
        "one block of four times the L3 is enough"
    );
}

#[tokio::test]
async fn the_threads_data_alone_can_be_too_small() {
    // One block of 32 MiB fits, but three threads' blocks of 8 MiB make 24 MiB together.
    let mut host = common::host();
    host.threads = 3;
    host.available = 64 * MIB;
    let plan = Plan::new(&host);
    assert_eq!((plan.single, plan.per_thread), (32 * MIB, 8 * MIB));
    let measured = measure(Some(&plan), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.small, Some(32 * MIB));
}

#[tokio::test]
async fn without_the_available_memory_nothing_runs() {
    let measured = measure(None, |_, _| None).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoMemoryInfo)));
    assert!(measured.jobs.is_empty());
}

#[tokio::test]
async fn unknown_caches_are_noted() {
    let mut host = common::host();
    host.caches.clear();
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert!(output.cache_unknown);
    assert_eq!(output.single, 128 * MIB, "four times the assumed 32 MiB");
}
