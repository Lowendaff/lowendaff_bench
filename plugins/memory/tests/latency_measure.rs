//! The latency chapter with recorded runs in place of multichase: a run per level with its
//! buffer, what it returns, publishes and logs, retries, failures, a cancellation, levels without
//! a size and too little memory.

mod common;

use std::path::Path;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::typed::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_memory::latency::Failure;
use plugin_memory::latency::Fault;
use plugin_memory::latency::Output;
use plugin_memory::latency::measure;
use plugin_memory::plan::KIB;
use plugin_memory::plan::Level;
use plugin_memory::plan::MIB;
use plugin_memory::plan::Plan;

const MULTICHASE: &str = "/t/multichase/multichase";

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
    let result = measure::measure(&ctx, Path::new(MULTICHASE), plan, |job| recorder.run(job)).await;
    let (published, progress) = common::drained(&mut channels);
    Measured {
        result,
        published,
        progress,
        jobs: recorder.jobs(),
    }
}

/// What multichase prints when a run works, with latencies that tell the levels apart.
fn fine(job: &Job) -> Result<job::Output, RunError> {
    let latency = match common::arg(job, "-m") {
        Some("16384") => "  1.500",
        Some("524288") => "  4.200",
        Some("4194304") => " 12.000",
        _ => " 90.500",
    };
    common::exited(0, &format!("{latency}\n"))
}

/// Each row's level, buffer and latency, as measured.
fn rows(output: &Output) -> Vec<(Level, u64, Option<&str>)> {
    output
        .rows
        .iter()
        .map(|row| {
            let latency = row.latency.as_ref().map(|value| value.text.as_str());
            (row.level, row.buffer, latency)
        })
        .collect()
}

#[tokio::test]
async fn a_run_per_cache_level_and_one_in_memory() {
    let plan = Plan::new(&common::host());
    let measured = measure(Some(&plan), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(
        rows(output),
        [
            (Level::L1, 16 * KIB, Some("1.500")),
            (Level::L2, 512 * KIB, Some("4.200")),
            (Level::L3, 4 * MIB, Some("12.000")),
            (Level::Memory, 32 * MIB, Some("90.500")),
        ]
    );
    assert!(output.missing.is_empty());
    assert_eq!((output.small, output.cache_unknown), (None, false));
    assert_eq!(measured.published.as_ref(), Some(output));
    assert_eq!(
        measured.progress,
        ["L1 测试", "L2 测试", "L3 测试", "内存测试"]
    );
    let args: Vec<Vec<String>> = measured.jobs.iter().map(common::args).collect();
    assert_eq!(
        args,
        [
            ["-m", "16384", "-n", "5"],
            ["-m", "524288", "-n", "5"],
            ["-m", "4194304", "-n", "5"],
            ["-m", "33554432", "-n", "5"],
        ]
    );
    assert_eq!(
        measured.jobs[0],
        measure::job(Path::new(MULTICHASE), 16 * KIB)
    );
    assert_eq!(measured.jobs[0].limit, Duration::from_secs(60));
    assert!(
        measured.jobs.iter().all(|job| job.cpus.is_none()),
        "nothing is placed"
    );
}

#[tokio::test]
async fn a_run_that_fails_every_attempt_fails_the_chapter_and_keeps_the_levels_before() {
    let measured = measure(Some(&Plan::new(&common::host())), |job, _| {
        (common::arg(job, "-m") == Some("4194304")).then(|| common::exited(0, "\n"))
    })
    .await;
    assert_eq!(
        measured.result,
        Err(Stop::Failed(Failure::Run {
            level: Level::L3,
            faults: vec![Fault::NoLatency; 4],
        }))
    );
    let published = measured.published.expect("two levels were published");
    assert_eq!(
        rows(&published)[1..],
        [
            (Level::L2, 512 * KIB, Some("4.200")),
            (Level::L3, 4 * MIB, None),
            (Level::Memory, 32 * MIB, None),
        ]
    );
}

#[tokio::test]
async fn a_level_that_works_after_failed_attempts_keeps_their_faults() {
    let signalled = job::Fault::Signal { last: None };
    let measured = measure(Some(&Plan::new(&common::host())), |job, before| {
        let l1 = common::arg(job, "-m") == Some("16384");
        (l1 && before == 0).then(|| Err(RunError::Fault(signalled.clone())))
    })
    .await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.rows[0].retries, [Fault::Tool(signalled)]);
    assert!(output.rows[1..].iter().all(|row| row.retries.is_empty()));
}

#[tokio::test]
async fn a_cancellation_ends_the_chapter_at_once() {
    let measured = measure(Some(&Plan::new(&common::host())), |job, _| {
        (common::arg(job, "-m") == Some("524288")).then_some(Err(RunError::Cancelled))
    })
    .await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(measured.jobs.len(), 2);
    let published = measured.published.expect("L1 was published");
    assert_eq!(rows(&published)[0], (Level::L1, 16 * KIB, Some("1.500")));
}

#[tokio::test]
async fn levels_the_kernel_says_nothing_of_are_left_out() {
    let mut host = common::host();
    host.caches.retain(|cache| cache.level < 3);
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.missing, [Level::L3]);
    let levels: Vec<Level> = output.rows.iter().map(|row| row.level).collect();
    assert_eq!(levels, [Level::L1, Level::L2, Level::Memory]);

    let mut unknown = common::host();
    unknown.caches.clear();
    let measured = measure(Some(&Plan::new(&unknown)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.missing, [Level::L1, Level::L2, Level::L3]);
    assert_eq!(rows(output), [(Level::Memory, 128 * MIB, Some("90.500"))]);
    assert!(output.cache_unknown);
}

#[tokio::test]
async fn too_little_memory_shrinks_memorys_buffer_and_says_so() {
    let mut host = common::host();
    host.available = 32 * MIB;
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(
        rows(output).last(),
        Some(&(Level::Memory, 16 * MIB, Some("90.500")))
    );
    assert_eq!(output.small, Some(32 * MIB));
}

#[tokio::test]
async fn without_the_available_memory_nothing_runs() {
    let measured = measure(None, |_, _| None).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoMemoryInfo)));
    assert!(measured.jobs.is_empty());
}
