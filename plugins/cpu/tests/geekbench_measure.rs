//! The measurement with recorded runs instead of Geekbench: its command, its runs again after
//! failed uploads, its other failures, a cancellation, and its note on the kernel's first choice.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::typed::Context;
use bench_api::typed::Progress;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::Adjustment;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_cpu::geekbench::Failure;
use plugin_cpu::geekbench::Fault;
use plugin_cpu::geekbench::Output;
use plugin_cpu::geekbench::measure;
use plugin_cpu::geekbench::measure::Ran;
use plugin_cpu::geekbench::result::Scores;
use tokio::time::Instant;

const GEEKBENCH: &str = "/tools/geekbench/geekbench7";

const UNREACHABLE: &str = "unable to resolve host browser.geekbench.com";

struct Outcome {
    output: Output,
    result: Result<(), Failure>,
    published: Option<Output>,
    progress: Vec<String>,
    jobs: Vec<Job>,
    work_dir: PathBuf,
}

/// Measures with runs that return `script` in order; with `cancel_after_first`, the plugin is
/// cancelled once the first run returns.
async fn measure(script: Vec<Ran>, cancel_after_first: bool) -> Outcome {
    let work = tempfile::tempdir().expect("a temporary directory");
    let cancel = CancellationToken::new();
    let (ctx, mut channels) = Context::<Output>::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(4 * 3600),
        work.path().into(),
        cancel.clone(),
    );
    let script = Mutex::new(script.into_iter());
    let jobs = Mutex::new(Vec::new());
    let (output, result) = measure::measure(&ctx, Output::default(), Path::new(GEEKBENCH), |job| {
        jobs.lock().expect("the jobs").push(job);
        let next = script
            .lock()
            .expect("the script")
            .next()
            .expect("every run is scripted");
        if cancel_after_first {
            cancel.cancel();
        }
        async move { next }
    })
    .await;
    assert!(
        script.lock().expect("the script").next().is_none(),
        "every scripted run happened"
    );
    let mut progress = Vec::new();
    while let Ok(item) = channels.progress.try_recv() {
        if let Progress::Line(line) = item {
            progress.push(line);
        }
    }
    let published = channels.published.borrow().clone();
    Outcome {
        output,
        result,
        published,
        progress,
        jobs: jobs.into_inner().expect("the jobs"),
        work_dir: work.path().to_path_buf(),
    }
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/geekbench")
        .join(name);
    std::fs::read_to_string(&path).expect("fixtures are committed next to the tests")
}

fn ran(code: Option<i32>, text: &str, adjustment: Adjustment) -> Ran {
    Ran {
        outcome: job::Outcome {
            ended: Ok(job::Output {
                code,
                text: text.to_owned(),
            }),
            adjustment: Some(adjustment),
        },
        killed: false,
    }
}

fn uploaded() -> Ran {
    ran(Some(0), &fixture("x86_64-upload.txt"), Adjustment::Done)
}

fn offline() -> Ran {
    ran(Some(255), &fixture("x86_64-offline.txt"), Adjustment::Done)
}

fn ended(ended: Result<job::Output, RunError>) -> Ran {
    Ran {
        outcome: job::Outcome {
            ended,
            adjustment: Some(Adjustment::Done),
        },
        killed: false,
    }
}

fn the_scores() -> Scores {
    Scores {
        version: Some("7.1.0".to_owned()),
        single: 1771,
        multi: 4347,
        url: Some("https://browser.geekbench.com/v7/cpu/840201".to_owned()),
    }
}

#[test]
fn geekbench_runs_its_cpu_benchmark_with_an_upload() {
    let attempt = measure::attempt(Path::new(GEEKBENCH), Path::new("/run/work"));
    assert_eq!(
        attempt,
        Job {
            program: PathBuf::from(GEEKBENCH),
            args: vec!["--cpu".into(), "--upload".into()],
            envs: vec![("LANG".into(), "C".into())],
            dir: Some(PathBuf::from("/run/work")),
            limit: Duration::from_secs(3600),
            group: true,
            first_to_go: true,
            quotable: false,
        }
    );
}

#[tokio::test(start_paused = true)]
async fn the_scores_and_the_link_are_kept() {
    let outcome = measure(vec![uploaded()], false).await;
    assert_eq!(outcome.result, Ok(()));
    assert_eq!(outcome.output.scores, Some(the_scores()));
    assert!(outcome.output.uploads.is_empty());
    assert_eq!(
        outcome.jobs,
        [measure::attempt(Path::new(GEEKBENCH), &outcome.work_dir)],
        "one run, in the plugin's work directory"
    );
    assert!(outcome.progress.is_empty());
    assert_eq!(outcome.published, None, "nothing to note");
}

#[tokio::test(start_paused = true)]
async fn failed_uploads_are_run_again_30_seconds_later() {
    let started = Instant::now();
    let outcome = measure(vec![offline(), offline(), uploaded()], false).await;
    assert_eq!(outcome.result, Ok(()));
    assert_eq!(outcome.output.scores, Some(the_scores()));
    assert_eq!(outcome.output.uploads, [UNREACHABLE; 2]);
    assert_eq!(outcome.jobs.len(), 3);
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(60),
        "two waits of 30 seconds"
    );
    assert_eq!(
        outcome.progress,
        ["上传失败，第 2 次运行", "上传失败，第 3 次运行"]
    );
}

#[tokio::test(start_paused = true)]
async fn four_failed_uploads_fail_the_measurement() {
    let outcome = measure(vec![offline(), offline(), offline(), offline()], false).await;
    assert_eq!(outcome.result, Err(Failure::Upload(UNREACHABLE.to_owned())));
    assert_eq!(outcome.jobs.len(), 4, "the first run and 3 more");
    assert_eq!(outcome.output.uploads, [UNREACHABLE; 3]);
    assert_eq!(outcome.output.scores, None);
}

#[tokio::test(start_paused = true)]
async fn other_failures_are_not_run_again() {
    let killed = Ran {
        killed: true,
        ..offline()
    };
    let limit = job::Fault::TimedOut(Duration::from_secs(3600));
    let cases = [
        (
            ran(Some(1), "x\n", Adjustment::Done),
            Failure::Run(Fault::Tool(job::Fault::Exit {
                code: 1,
                last: Some("x".to_owned()),
            })),
        ),
        (killed, Failure::OutOfMemory),
        (
            ended(Err(RunError::Fault(limit.clone()))),
            Failure::Run(Fault::Tool(limit)),
        ),
        (ended(Err(RunError::Cancelled)), Failure::Cancelled),
        (
            ran(Some(0), "Geekbench 7.1.0\n", Adjustment::Done),
            Failure::Run(Fault::NoScores),
        ),
    ];
    for (run, failure) in cases {
        let outcome = measure(vec![run], false).await;
        assert_eq!(outcome.result, Err(failure.clone()));
        assert_eq!(outcome.jobs.len(), 1, "{failure:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn scores_count_even_when_the_kernel_killed_something() {
    let outcome = measure(
        vec![Ran {
            killed: true,
            ..uploaded()
        }],
        false,
    )
    .await;
    assert_eq!(outcome.result, Ok(()));
}

#[tokio::test(start_paused = true)]
async fn a_cancellation_during_the_wait_ends_the_measurement() {
    let started = Instant::now();
    let outcome = measure(vec![offline()], true).await;
    assert_eq!(outcome.result, Err(Failure::Cancelled));
    assert_eq!(outcome.jobs.len(), 1);
    assert_eq!(started.elapsed(), Duration::ZERO, "no wait");
}

#[tokio::test(start_paused = true)]
async fn the_first_run_that_could_not_go_first_is_noted_and_published() {
    let not_set = Adjustment::NotSet("No such file or directory (os error 2)".to_owned());
    let outcome = measure(
        vec![
            ran(Some(255), &fixture("x86_64-offline.txt"), not_set.clone()),
            ran(
                Some(0),
                &fixture("x86_64-upload.txt"),
                Adjustment::NotRestored("Permission denied (os error 13)".to_owned()),
            ),
        ],
        false,
    )
    .await;
    assert_eq!(outcome.result, Ok(()));
    assert_eq!(outcome.output.first_to_go, Some(not_set.clone()));
    let published = outcome.published.expect("the note was published");
    assert_eq!(published.first_to_go, Some(not_set));
    assert_eq!(published.scores, None, "published before the scores");
}
