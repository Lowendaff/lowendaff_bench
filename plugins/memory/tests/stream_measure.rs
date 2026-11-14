//! STREAM's chapter with recorded runs in place of STREAM: the build and the environment of its
//! runs, what it returns, publishes and logs, retries, failures, a cancellation, and arrays that
//! do not fit.

mod common;

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Stop;
use bench_host::cache::CacheKind;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_memory::plan::MIB;
use plugin_memory::plan::Plan;
use plugin_memory::stream::Failure;
use plugin_memory::stream::Fault;
use plugin_memory::stream::Output;
use plugin_memory::stream::measure;

const SMALLEST: &str = "/t/stream/stream-2m";

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
    let result = measure::measure(&ctx, Path::new(SMALLEST), plan, |job| recorder.run(job)).await;
    let (published, progress) = common::drained(&mut channels);
    Measured {
        result,
        published,
        progress,
        jobs: recorder.jobs(),
    }
}

/// Whether `job` runs with one thread.
fn single(job: &Job) -> bool {
    job.envs.contains(&("OMP_NUM_THREADS".into(), "1".into()))
}

/// What STREAM prints when a run works, with rates that tell the runs apart.
fn fine(job: &Job) -> Result<job::Output, RunError> {
    let [copy, scale, add, triad] = if single(job) {
        ["10000.1", "9000.2", "11000.3", "11500.4"]
    } else {
        ["40000.1", "36000.2", "44000.3", "46000.4"]
    };
    common::exited(
        0,
        &format!(
            "STREAM version $Revision: 5.10 $\nFunction    Best Rate MB/s  Avg time\n\
             Copy:  {copy}  0.1\nScale:  {scale}  0.1\nAdd:  {add}  0.1\nTriad:  {triad}  0.1\n\
             Solution Validates: avg error less than 1.000000e-13 on all three arrays\n"
        ),
    )
}

/// The triad rate of each run that worked, by threads.
fn triads(output: &Output) -> Vec<(usize, &str)> {
    output
        .runs
        .iter()
        .map(|run| (run.threads, run.rates[3].text.as_str()))
        .collect()
}

#[tokio::test]
async fn the_builds_arrays_with_one_thread_and_with_all_spread_over_the_nodes() {
    let plan = Plan::new(&common::host());
    let measured = measure(Some(&plan), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.version.as_deref(), Some("5.10"));
    assert_eq!((output.millions, output.threads), (8, 4));
    assert!(!output.below_rule);
    assert!(!output.cache_unknown);
    assert_eq!(triads(output), [(1, "11500.4"), (4, "46000.4")]);
    let copies: Vec<&str> = output
        .runs
        .iter()
        .map(|run| run.rates[0].text.as_str())
        .collect();
    assert_eq!(copies, ["10000.1", "40000.1"], "Copy first");
    assert_eq!(measured.published.as_ref(), Some(output));
    assert_eq!(measured.progress, ["单线程测试", "多线程测试（4 线程）"]);

    let build = PathBuf::from("/t/stream/stream-8m");
    assert!(measured.jobs.iter().all(|job| job.program == build));
    assert!(measured.jobs.iter().all(|job| job.args.is_empty()));
    assert_eq!(
        measured.jobs[0].envs,
        [("OMP_NUM_THREADS".into(), "1".into())]
    );
    assert_eq!(
        measured.jobs[1].envs,
        [
            ("OMP_NUM_THREADS".into(), "4".into()),
            ("OMP_PROC_BIND".into(), "spread".into()),
        ]
    );
    assert_eq!(measured.jobs[1], measure::job(Path::new(SMALLEST), 8, 4));
    assert_eq!(measured.jobs[0].limit, Duration::from_secs(180));
}

#[tokio::test]
async fn one_thread_runs_once() {
    let mut host = common::host();
    host.threads = 1;
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(triads(output), [(1, "11500.4")]);
    assert_eq!(measured.jobs.len(), 1);
}

#[tokio::test]
async fn a_run_that_fails_every_attempt_fails_the_chapter_before_the_next_run() {
    let measured = measure(Some(&Plan::new(&common::host())), |job, _| {
        single(job).then(|| common::exited(1, "boom\n"))
    })
    .await;
    assert_eq!(
        measured.result,
        Err(Stop::Failed(Failure::Run {
            threads: 1,
            faults: vec![Fault::Tool(common::exit(1, "boom")); 4],
        }))
    );
    assert_eq!(measured.jobs.len(), 4, "the multi-thread run never starts");
    assert_eq!(measured.published, None);
}

#[tokio::test]
async fn results_that_do_not_validate_are_tried_again() {
    let invalid = "Failed Validation on array a[], AvgRelAbsErr > epsilon (1.000000e-13)";
    let measured = measure(Some(&Plan::new(&common::host())), |job, before| {
        (!single(job) && before == 0).then(|| common::exited(0, &format!("{invalid}\n")))
    })
    .await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert!(output.runs[0].retries.is_empty());
    assert_eq!(output.runs[1].retries, [Fault::Invalid(invalid.to_owned())]);
}

#[tokio::test]
async fn a_cancellation_keeps_the_run_that_ended() {
    let measured = measure(Some(&Plan::new(&common::host())), |job, _| {
        (!single(job)).then_some(Err(RunError::Cancelled))
    })
    .await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(measured.jobs.len(), 2, "a cancellation is not retried");
    let published = measured.published.expect("the first run was published");
    assert_eq!(triads(&published), [(1, "11500.4")]);
}

#[tokio::test]
async fn arrays_that_do_not_fit_skip_the_chapter() {
    let mut host = common::host();
    host.available = 32 * MIB;
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    assert_eq!(
        measured.result,
        Err(Stop::Skipped(Failure::Short { budget: 16 * MIB }))
    );
    assert!(measured.jobs.is_empty());
    assert!(measured.progress.is_empty());
}

#[tokio::test]
async fn arrays_below_four_times_the_caches_are_noted() {
    // L3 caches of 48 MiB ask for arrays of 192 MiB; half of 300 MiB fits three of 32 MB.
    let mut host = common::host();
    host.available = 300 * MIB;
    host.caches.push(common::cache(
        3,
        CacheKind::Unified,
        40 * MIB,
        &[0, 1, 2, 3],
    ));
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.millions, 4);
    assert!(output.below_rule);

    // Arrays of exactly four times the caches meet the rule.
    let mut exact = common::host();
    exact.caches.pop();
    exact.caches.push(common::cache(
        3,
        CacheKind::Unified,
        16_000_000,
        &[0, 1, 2, 3],
    ));
    let measured = measure(Some(&Plan::new(&exact)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.millions, 8, "arrays of 64 MB for caches of 16 MB");
    assert!(!output.below_rule);
}

#[tokio::test]
async fn without_the_available_memory_nothing_runs() {
    let measured = measure(None, |_, _| None).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoMemoryInfo)));
    assert!(measured.jobs.is_empty());
}
