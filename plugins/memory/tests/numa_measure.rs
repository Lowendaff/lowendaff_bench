//! The NUMA chapter with recorded runs in place of multiload and multichase: the runs of each
//! cell on its row's CPUs with memory bound to its column, what it returns, publishes and logs,
//! a refused binding, retries, failures and a cancellation.

mod common;

use std::path::Path;
use std::path::PathBuf;

use bench_api::CancellationToken;
use bench_api::typed::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_memory::numa::Failure;
use plugin_memory::numa::Fault;
use plugin_memory::numa::Output;
use plugin_memory::numa::measure;
use plugin_memory::numa::result::Kind;
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

fn multiload(job: &Job) -> bool {
    job.program.ends_with("multiload")
}

/// What the tools print when a run works: near memory is faster than far memory.
fn fine(job: &Job) -> Result<job::Output, RunError> {
    let near = matches!(
        (common::arg(job, "-W"), job.cpus.as_deref()),
        (Some("0:1"), Some([0, 1])) | (Some("1:1"), Some([2, 3]))
    );
    if multiload(job) {
        let mibs = if near { "52000" } else { "31000" };
        common::exited(
            0,
            &format!(
                "Samples\t, Byte/thd\t, LoadThds\t, LdMaxMibs\t, LdAvgMibs\n3     \t, 1   \t, \
                 2   \t, {mibs}   \t, 1\n"
            ),
        )
    } else {
        common::exited(0, if near { " 95.000\n" } else { " 140.1\n" })
    }
}

/// A table of the output, by kind: the measured text of each cell, row by row.
fn table(output: &Output, kind: Kind) -> Vec<Vec<Option<&str>>> {
    output
        .cells
        .iter()
        .map(|cells| {
            cells
                .iter()
                .map(|cell| {
                    let measured = match kind {
                        Kind::Bandwidth => cell.bandwidth.as_ref(),
                        Kind::Latency => cell.latency.as_ref(),
                    };
                    measured.map(|measured| measured.value.text.as_str())
                })
                .collect()
        })
        .collect()
}

fn refused() -> Result<job::Output, RunError> {
    common::exited(1, "mbind: Operation not permitted\n")
}

#[tokio::test]
async fn each_cell_runs_on_its_rows_cpus_with_memory_bound_to_its_column() {
    let plan = Plan::new(&common::two_nodes());
    let measured = measure(Some(&plan), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.nodes, 2);
    assert_eq!(
        (output.rows.as_slice(), output.columns.as_slice()),
        (&[0, 1][..], &[0, 1][..])
    );
    assert_eq!(
        table(output, Kind::Bandwidth),
        [
            [Some("52000"), Some("31000")],
            [Some("31000"), Some("52000")]
        ]
    );
    assert_eq!(
        table(output, Kind::Latency),
        [
            [Some("95.000"), Some("140.1")],
            [Some("140.1"), Some("95.000")]
        ]
    );
    assert!(!output.cache_unknown);
    assert_eq!(measured.published.as_ref(), Some(output));
    assert_eq!(measured.progress.len(), 8);
    assert_eq!(
        measured.progress[..2],
        [
            "线程在节点 0、内存在节点 0 的带宽测试",
            "线程在节点 0、内存在节点 0 的延迟测试"
        ]
    );

    let placed: Vec<(bool, &[usize], Vec<String>)> = measured
        .jobs
        .iter()
        .map(|job| {
            (
                multiload(job),
                job.cpus.as_deref().expect("every run is placed"),
                common::args(job),
            )
        })
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
            (true, &[0, 1][..], bandwidth("0:1")),
            (false, &[0, 1][..], latency("0:1")),
            (true, &[0, 1][..], bandwidth("1:1")),
            (false, &[0, 1][..], latency("1:1")),
        ]
    );
    assert_eq!(placed[4].1, [2, 3], "the second row's threads");
    assert_eq!(
        measured.jobs[0].program,
        PathBuf::from("/t/multichase/multiload")
    );
    assert_eq!(measured.jobs[1].program, PathBuf::from(MULTICHASE));
    let matrix = plan.matrix.as_ref().expect("two nodes");
    assert_eq!(
        measured.jobs[1],
        measure::latency(
            Path::new(MULTICHASE),
            &matrix.rows[0],
            0,
            matrix.cells[0][0]
        )
    );
}

#[tokio::test]
async fn threads_on_one_node_still_reach_the_memory_of_both() {
    let mut host = common::two_nodes();
    host.layout.cpus = Some(vec![0, 1]);
    let measured = measure(Some(&Plan::new(&host)), |_, _| None).await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    assert_eq!(output.nodes, 2, "the nodes of the rows and the columns");
    assert_eq!(
        (output.rows.as_slice(), output.columns.as_slice()),
        (&[0][..], &[0, 1][..])
    );
    assert_eq!(
        table(output, Kind::Bandwidth),
        [[Some("52000"), Some("31000")]]
    );
    assert_eq!(measured.jobs.len(), 4);
}

#[tokio::test]
async fn a_refused_binding_skips_the_chapter_without_trying_again() {
    let measured = measure(Some(&Plan::new(&common::two_nodes())), |job, _| {
        multiload(job).then(refused)
    })
    .await;
    assert_eq!(measured.result, Err(Stop::Skipped(Failure::Refused)));
    assert_eq!(measured.jobs.len(), 1, "a refusal is not retried");
}

#[tokio::test]
async fn a_refusal_after_measured_cells_still_skips_the_chapter() {
    let measured = measure(Some(&Plan::new(&common::two_nodes())), |job, _| {
        (!multiload(job) && common::arg(job, "-W") == Some("1:1")).then(refused)
    })
    .await;
    assert_eq!(measured.result, Err(Stop::Skipped(Failure::Refused)));
    assert_eq!(measured.jobs.len(), 4);
}

#[tokio::test]
async fn a_run_that_fails_every_attempt_fails_the_chapter_and_names_its_cell() {
    let measured = measure(Some(&Plan::new(&common::two_nodes())), |job, _| {
        let far = common::arg(job, "-W") == Some("1:1") && job.cpus.as_deref() == Some(&[0, 1]);
        (multiload(job) && far).then(|| common::exited(0, "no header\n"))
    })
    .await;
    assert_eq!(
        measured.result,
        Err(Stop::Failed(Failure::Run {
            row: 0,
            column: 1,
            kind: Kind::Bandwidth,
            faults: vec![Fault::NoBandwidth; 4],
        }))
    );
    let published = measured.published.expect("the first cell was published");
    assert_eq!(
        table(&published, Kind::Latency),
        [[Some("95.000"), None], [None, None]]
    );
}

#[tokio::test]
async fn a_run_that_works_after_failed_attempts_keeps_their_faults() {
    let measured = measure(Some(&Plan::new(&common::two_nodes())), |job, before| {
        let first = common::arg(job, "-W") == Some("0:1") && job.cpus.as_deref() == Some(&[0, 1]);
        (!multiload(job) && first && before == 0).then(|| common::exited(0, "\n"))
    })
    .await;
    let Ok(output) = &measured.result else {
        panic!("the chapter completes: {:?}", measured.result);
    };
    let latency = output.cells[0][0].latency.as_ref().expect("measured");
    assert_eq!(latency.retries, [Fault::NoLatency]);
    let bandwidth = output.cells[0][0].bandwidth.as_ref().expect("measured");
    assert!(bandwidth.retries.is_empty());
}

#[tokio::test]
async fn a_cancellation_ends_the_chapter_at_once() {
    let measured = measure(Some(&Plan::new(&common::two_nodes())), |job, _| {
        (!multiload(job)).then_some(Err(RunError::Cancelled))
    })
    .await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(measured.jobs.len(), 2);
    let published = measured
        .published
        .expect("the first bandwidth was published");
    assert_eq!(
        table(&published, Kind::Bandwidth),
        [[Some("52000"), None], [None, None]]
    );
}

#[tokio::test]
async fn one_node_has_nothing_to_measure() {
    let measured = measure(Some(&Plan::new(&common::host())), |_, _| None).await;
    assert_eq!(measured.result, Err(Stop::Skipped(Failure::OneNode)));
    assert!(measured.jobs.is_empty());
    let measured = measure(None, |_, _| None).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoMemoryInfo)));
}
