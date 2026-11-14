//! The measurement with recorded runs instead of sysbench: the runs it makes, what it returns,
//! publishes and logs, its retries, its failures and a cancellation.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Progress;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_cpu::sysbench::Failure;
use plugin_cpu::sysbench::Fault;
use plugin_cpu::sysbench::Output;
use plugin_cpu::sysbench::measure;
use plugin_cpu::sysbench::result::Rate;
use plugin_cpu::sysbench::result::Run;
use tokio::time::Instant;

const SYSBENCH: &str = "/tools/sysbench";

struct Outcome {
    result: Result<Output, Stop<Failure>>,
    published: Option<Output>,
    progress: Vec<String>,
    jobs: Vec<Job>,
}

impl Outcome {
    /// The threads of each attempt, in order.
    fn threads(&self) -> Vec<usize> {
        let mut threads = Vec::new();
        for job in &self.jobs {
            let count = (1..=64)
                .find(|count| *job == measure::attempt(Path::new(SYSBENCH), *count))
                .expect("each job is an attempt of sysbench");
            threads.push(count);
        }
        threads
    }
}

/// Measures with `threads`; the attempts return `script` in order.
async fn measure(threads: usize, script: Vec<Result<job::Output, RunError>>) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(600);
    let (ctx, mut channels) = Context::<Output>::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        deadline,
        ".".into(),
        CancellationToken::new(),
    );
    let script = RefCell::new(VecDeque::from(script));
    let jobs = RefCell::new(Vec::new());
    let result = measure::measure(&ctx, Path::new(SYSBENCH), threads, |job| {
        jobs.borrow_mut().push(job);
        let next = script
            .borrow_mut()
            .pop_front()
            .expect("every attempt is scripted");
        async move { next }
    })
    .await;
    assert!(script.borrow().is_empty(), "every scripted attempt ran");
    let mut progress = Vec::new();
    while let Ok(item) = channels.progress.try_recv() {
        if let Progress::Line(line) = item {
            progress.push(line);
        }
    }
    let published = channels.published.borrow().clone();
    Outcome {
        result,
        published,
        progress,
        jobs: jobs.into_inner(),
    }
}

fn ran(rate: &str) -> Result<job::Output, RunError> {
    Ok(job::Output {
        code: Some(0),
        text: format!(
            "sysbench 1.0.20 (using bundled LuaJIT 2.1.0-beta2)\n\n    events per second: {rate}\n"
        ),
    })
}

fn exited(code: i32, last: &str) -> Result<job::Output, RunError> {
    Ok(job::Output {
        code: Some(code),
        text: format!("{last}\n"),
    })
}

fn timed_out() -> Result<job::Output, RunError> {
    Err(RunError::Fault(job::Fault::TimedOut(
        measure::ATTEMPT_LIMIT,
    )))
}

fn no_rate() -> Result<job::Output, RunError> {
    Ok(job::Output {
        code: Some(0),
        text: "sysbench 1.0.20\n".to_owned(),
    })
}

fn exit(code: i32, last: &str) -> Fault {
    Fault::Tool(job::Fault::Exit {
        code,
        last: Some(last.to_owned()),
    })
}

fn run(threads: usize, rate: &str, retries: Vec<Fault>) -> Run {
    Run {
        threads,
        rate: Rate {
            text: rate.to_owned(),
            value: rate.parse().expect("a number"),
        },
        retries,
    }
}

fn single(rate: &str, retries: Vec<Fault>) -> Output {
    Output {
        version: Some("1.0.20".to_owned()),
        single: run(1, rate, retries),
        multi: None,
    }
}

#[tokio::test]
async fn both_runs_are_measured_and_the_first_is_published() {
    let outcome = measure(4, vec![ran("12842.07"), ran("49847.67")]).await;
    let first = single("12842.07", Vec::new());
    let both = Output {
        multi: Some(run(4, "49847.67", Vec::new())),
        ..first.clone()
    };
    assert_eq!(outcome.result, Ok(both));
    assert_eq!(outcome.published, Some(first));
    assert_eq!(outcome.progress, ["单线程测试", "多线程测试（4 线程）"]);
    assert_eq!(outcome.threads(), [1, 4]);
}

#[tokio::test]
async fn one_thread_runs_once() {
    let outcome = measure(1, vec![ran("1234.56")]).await;
    assert_eq!(outcome.result, Ok(single("1234.56", Vec::new())));
    assert_eq!(outcome.published, None, "the result is returned at once");
    assert_eq!(outcome.progress, ["单线程测试"]);
    assert_eq!(outcome.threads(), [1]);
}

#[tokio::test]
async fn the_faults_of_retried_attempts_are_kept() {
    let outcome = measure(
        2,
        vec![
            exited(1, "x"),
            ran("100.00"),
            timed_out(),
            no_rate(),
            ran("150.00"),
        ],
    )
    .await;
    let retried = Output {
        multi: Some(run(
            2,
            "150.00",
            vec![
                Fault::Tool(job::Fault::TimedOut(Duration::from_secs(60))),
                Fault::NoRate,
            ],
        )),
        ..single("100.00", vec![exit(1, "x")])
    };
    assert_eq!(outcome.result, Ok(retried));
    assert_eq!(outcome.threads(), [1, 1, 2, 2, 2]);
}

#[tokio::test]
async fn a_single_thread_run_that_always_fails_fails_without_a_result() {
    let outcome = measure(4, vec![exited(1, "boom"); 4]).await;
    assert_eq!(
        outcome.result,
        Err(Stop::Failed(Failure::Single(vec![exit(1, "boom"); 4])))
    );
    assert_eq!(outcome.published, None);
    assert_eq!(
        outcome.threads(),
        [1, 1, 1, 1],
        "the multi-thread run does not start"
    );
}

#[tokio::test]
async fn a_multi_thread_run_that_always_fails_leaves_the_single_thread_run() {
    let mut script = vec![ran("12842.07")];
    script.extend((0..4).map(|_| timed_out()));
    let outcome = measure(4, script).await;
    let timed_out = Fault::Tool(job::Fault::TimedOut(Duration::from_secs(60)));
    assert_eq!(
        outcome.result,
        Err(Stop::Failed(Failure::Multi(vec![timed_out; 4])))
    );
    assert_eq!(outcome.published, Some(single("12842.07", Vec::new())));
}

#[tokio::test]
async fn a_cancellation_ends_the_measurement_without_retries() {
    let outcome = measure(4, vec![Err(RunError::Cancelled)]).await;
    assert_eq!(outcome.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(outcome.threads(), [1]);

    let outcome = measure(4, vec![ran("12842.07"), Err(RunError::Cancelled)]).await;
    assert_eq!(outcome.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(outcome.published, Some(single("12842.07", Vec::new())));
}
