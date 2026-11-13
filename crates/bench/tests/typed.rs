//! Plugins of the typed contract in the orchestrator: what they return, what they last published
//! when they stop short (also during the grace period after a timeout), and their progress lines.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Fault;
use bench::orchestrator::Observer;
use bench::orchestrator::Planned;
use bench::orchestrator::Run;
use bench::orchestrator::Settings;
use bench::orchestrator::Status;
use bench::results::Report;
use bench_api::CancellationToken;
use bench_api::Env;
use bench_api::Requirements;
use bench_api::typed::Context;
use bench_api::typed::Descriptor;
use bench_api::typed::Measurement;
use bench_api::typed::Plugin;
use bench_api::typed::Stop;
use bench_api::typed::ToolVersion;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Count(u32);

impl Measurement for Count {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum Why {
    NoDisk,
    Broken,
}

/// The tests' own result enum, as `Measured` is the program's.
#[derive(Debug, Clone, PartialEq)]
enum Left {
    Counter(Report<Count, Why>),
}

const fn descriptor(id: &'static str, timeout_ms: u64) -> Descriptor {
    Descriptor {
        id,
        requires: Requirements::NONE,
        timeout: Duration::from_millis(timeout_ms),
    }
}

#[derive(Default, Deserialize)]
struct Counts;

impl Plugin for Counts {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.counts", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        for count in 1..=2 {
            ctx.progress(format!("第 {count} 次"));
            ctx.publish(&Count(count));
        }
        Ok(Count(3))
    }
}

#[derive(Default, Deserialize)]
struct GivesUp;

impl Plugin for GivesUp {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.gives-up", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.publish(&Count(1));
        Err(Stop::Failed(Why::Broken))
    }
}

#[derive(Default, Deserialize)]
struct Skips;

impl Plugin for Skips {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.skips", 10_000);

    async fn run(&self, _ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        Err(Stop::Skipped(Why::NoDisk))
    }
}

/// Publishes once, waits for its cancel token, then publishes again inside the grace period and
/// stops, as the contract asks.
#[derive(Default, Deserialize)]
struct Slow;

impl Plugin for Slow {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.slow", 5_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.publish(&Count(1));
        ctx.cancel_token().cancelled().await;
        ctx.publish(&Count(2));
        ctx.progress("收到停止信号");
        Err(Stop::Failed(Why::Broken))
    }
}

#[derive(Default, Deserialize)]
struct Panics;

impl Plugin for Panics {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.panics", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.publish(&Count(1));
        panic!("boom");
    }
}

/// Publishes, then never ends by itself, as a plugin stuck in a call would: only the abort ends
/// it.
#[derive(Default, Deserialize)]
struct Stubborn;

impl Plugin for Stubborn {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.stubborn", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.publish(&Count(1));
        std::future::pending().await
    }
}

/// Interrupts the whole run, as Ctrl-C would, and gives up or completes in the same breath.
#[derive(Default, Deserialize)]
struct Interrupts {
    #[serde(skip)]
    interrupt: CancellationToken,
    #[serde(skip)]
    gives_up: bool,
}

impl Plugin for Interrupts {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.interrupts", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.publish(&Count(1));
        self.interrupt.cancel();
        if self.gives_up {
            Err(Stop::Failed(Why::Broken))
        } else {
            Ok(Count(2))
        }
    }
}

/// Says one line, and ends only once the line has been shown.
#[derive(Default, Deserialize)]
struct WaitsToBeSeen {
    #[serde(skip)]
    shown: CancellationToken,
}

impl Plugin for WaitsToBeSeen {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.waits", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.progress("等待显示");
        self.shown.cancelled().await;
        Ok(Count(1))
    }
}

/// Shows a progress line by cancelling its token.
struct Shows(CancellationToken);

impl Observer<Left> for Shows {
    fn on_progress(&mut self, _id: &'static str, _line: &str) {
        self.0.cancel();
    }
}

#[derive(Default)]
struct Recorder(Vec<String>);

impl Observer<Left> for Recorder {
    fn on_start(&mut self, id: &'static str) {
        self.0.push(format!("start {id}"));
    }

    fn on_progress(&mut self, id: &'static str, line: &str) {
        self.0.push(format!("{id} {line}"));
    }

    fn on_finish(&mut self, run: &Run<Left>) {
        self.0.push(format!("finish {} {:?}", run.id, run.status));
    }
}

async fn run(plan: Vec<Planned<Left>>, cancel: &CancellationToken) -> (Vec<Run<Left>>, Recorder) {
    let mut recorder = Recorder::default();
    let runs = run_observed(plan, cancel, &mut recorder).await;
    (runs, recorder)
}

async fn run_observed(
    plan: Vec<Planned<Left>>,
    cancel: &CancellationToken,
    observer: &mut dyn Observer<Left>,
) -> Vec<Run<Left>> {
    let work_root = tempfile::tempdir().expect("create a temporary work root");
    let settings = Settings {
        grace: Duration::from_millis(1_000),
        work_root: work_root.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        failed_tools: BTreeMap::new(),
    };
    orchestrator::run_plan(plan, Arc::new(Env::default()), cancel, &settings, observer).await
}

fn left(output: Option<u32>, stop: Option<Stop<Why>>) -> Option<Left> {
    Some(Left::Counter(Report {
        output: output.map(Count),
        stop,
    }))
}

#[tokio::test]
async fn a_returned_result_replaces_what_was_published() {
    let (runs, recorder) = run(
        vec![Planned::typed(Counts, Left::Counter)],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(runs[0].status, Status::Completed);
    assert_eq!(runs[0].result, left(Some(3), None));
    assert_eq!(
        (runs[0].id, runs[0].group, runs[0].title),
        ("test.counts", "test", None)
    );
    assert_eq!(
        recorder.0,
        [
            "start test.counts",
            "test.counts 第 1 次",
            "test.counts 第 2 次",
            "finish test.counts Completed"
        ]
    );
}

#[tokio::test]
async fn a_stop_keeps_the_last_publish_and_the_plugins_reason() {
    let (runs, _) = run(
        vec![
            Planned::typed(GivesUp, Left::Counter),
            Planned::typed(Skips, Left::Counter),
        ],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(runs[0].status, Status::Failed(None));
    assert_eq!(
        runs[0].result,
        left(Some(1), Some(Stop::Failed(Why::Broken)))
    );
    assert_eq!(runs[1].status, Status::Skipped(None));
    assert_eq!(runs[1].result, left(None, Some(Stop::Skipped(Why::NoDisk))));
}

#[tokio::test(start_paused = true)]
async fn a_timeout_keeps_what_was_published_during_the_grace_period() {
    let (runs, recorder) = run(
        vec![Planned::typed(Slow, Left::Counter)],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(
        runs[0].result,
        left(Some(2), None),
        "the second publish came after the deadline, before the grace period ended"
    );
    assert!(
        recorder.0.contains(&"test.slow 收到停止信号".to_owned()),
        "progress from the grace period is drained: {:?}",
        recorder.0
    );
    assert!(runs[0].elapsed.expect("it started") >= Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn an_interrupt_keeps_the_last_publish() {
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    let (runs, _) = run(
        vec![
            Planned::typed(Slow, Left::Counter),
            Planned::typed(Counts, Left::Counter),
        ],
        &cancel,
    )
    .await;

    assert_eq!(runs[0].status, Status::Cancelled);
    assert_eq!(runs[0].result, left(Some(2), None));
    assert_eq!(runs[1].status, Status::Cancelled);
    assert_eq!(runs[1].result, None, "never started");
    assert_eq!(runs[1].started, None);
}

#[tokio::test]
async fn a_panic_keeps_the_last_publish_and_the_next_plugin_runs() {
    let (runs, _) = run(
        vec![
            Planned::typed(Panics, Left::Counter),
            Planned::typed(Counts, Left::Counter),
        ],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        runs[0].status,
        Status::Failed(Some(Fault::Panic("boom".into())))
    );
    assert_eq!(runs[0].result, left(Some(1), None));
    assert_eq!(runs[1].status, Status::Completed);
    assert!(
        runs[1].started.expect("it started") >= runs[0].started.expect("it started"),
        "plugins run in order"
    );
}

#[tokio::test(start_paused = true)]
async fn an_interrupt_keeps_the_last_publish_of_a_plugin_that_must_be_aborted() {
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    let (runs, _) = run(vec![Planned::typed(Stubborn, Left::Counter)], &cancel).await;

    assert_eq!(runs[0].status, Status::Cancelled);
    assert_eq!(runs[0].result, left(Some(1), None));
}

#[tokio::test]
async fn a_plugin_that_stops_short_once_the_run_is_interrupted_counts_as_interrupted() {
    let cancel = CancellationToken::new();
    let plugin = Interrupts {
        interrupt: cancel.clone(),
        gives_up: true,
    };
    let (runs, _) = run(
        vec![
            Planned::typed(plugin, Left::Counter),
            Planned::typed(Counts, Left::Counter),
        ],
        &cancel,
    )
    .await;

    assert_eq!(runs[0].status, Status::Cancelled);
    assert_eq!(
        runs[0].result,
        left(Some(1), None),
        "the publish stays; the reason is the interrupt's, not the plugin's"
    );
    assert_eq!(runs[1].status, Status::Cancelled);
}

#[tokio::test]
async fn a_plugin_that_has_ended_when_the_interrupt_comes_is_taken_at_its_word() {
    // Its end and the interrupt are both there when the orchestrator looks, and no run may pick
    // one of them by chance: hence many runs.
    for _ in 0..20 {
        let cancel = CancellationToken::new();
        let plugin = Interrupts {
            interrupt: cancel.clone(),
            gives_up: false,
        };
        let (runs, _) = run(
            vec![
                Planned::typed(plugin, Left::Counter),
                Planned::typed(Counts, Left::Counter),
            ],
            &cancel,
        )
        .await;

        assert_eq!(runs[0].status, Status::Completed);
        assert_eq!(runs[0].result, left(Some(2), None));
        assert_eq!(runs[1].status, Status::Cancelled);
    }
}

#[tokio::test(start_paused = true)]
async fn progress_lines_are_shown_while_the_plugin_runs() {
    let shown = CancellationToken::new();
    let plugin = WaitsToBeSeen {
        shown: shown.clone(),
    };
    let runs = run_observed(
        vec![Planned::typed(plugin, Left::Counter)],
        &CancellationToken::new(),
        &mut Shows(shown),
    )
    .await;

    assert_eq!(
        runs[0].status,
        Status::Completed,
        "the plugin waited for its line to be shown"
    );
}
