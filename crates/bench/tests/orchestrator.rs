//! Every outcome the orchestrator must produce: what a plugin returns, what it last published
//! when it stops short (also during the grace period after a timeout), its progress lines, the
//! requirements checked before it runs, its work directory, and the plan built from the registry
//! and the config.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Fault;
use bench::orchestrator::Observer;
use bench::orchestrator::Planned;
use bench::orchestrator::Quiet;
use bench::orchestrator::Run;
use bench::orchestrator::Settings;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::registry;
use bench::registry::Entry;
use bench::results::Measured;
use bench::results::Report;
use bench_api::CancellationToken;
use bench_api::Net;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed::Context;
use bench_api::typed::Descriptor;
use bench_api::typed::Measurement;
use bench_api::typed::Plugin;
use bench_api::typed::Stop;
use bench_api::typed::ToolEnd;
use bench_api::typed::ToolRun;
use bench_api::typed::ToolVersion;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Scores;
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

// ---------------------------------------------------------------- test plugins

#[derive(Default, Deserialize)]
struct Counts;

impl Plugin for Counts {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.counts", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        for count in 1..=2 {
            ctx.progress(format!("第 {count} 次"));
            ctx.ran(ToolRun {
                command: vec!["count".to_owned(), count.to_string()],
                end: ToolEnd::Exit(0),
                tail: Vec::new(),
            });
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

static HANGING_RELEASED: AtomicBool = AtomicBool::new(false);

/// Sets its flag when dropped, standing in for a resource such as a child process spawned with
/// `kill_on_drop`.
struct ReleaseFlag(&'static AtomicBool);

impl Drop for ReleaseFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Holds a resource, publishes, then never ends and ignores its cancel token, until its deadline
/// passes and the abort drops it.
#[derive(Default, Deserialize)]
struct Hanging;

impl Plugin for Hanging {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.hanging", 5_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        let _child_process = ReleaseFlag(&HANGING_RELEASED);
        ctx.publish(&Count(1));
        std::future::pending().await
    }
}

/// Loops on a blocking thread until its cancel token fires, then publishes how far it got.
#[derive(Default, Deserialize)]
struct CpuBound;

impl Plugin for CpuBound {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.cpu-bound", 200);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        let stop = ctx.cancel_token();
        let spun = tokio::task::spawn_blocking(move || {
            let started = std::time::Instant::now();
            let mut n: u64 = 0;
            // The time cap only keeps a broken orchestrator from hanging the test binary.
            while !stop.is_cancelled() && started.elapsed() < Duration::from_secs(5) {
                n = std::hint::black_box(n.wrapping_add(1));
            }
            n > 0
        })
        .await
        .expect("the loop does not panic");
        ctx.publish(&Count(u32::from(spun)));
        Ok(Count(2))
    }
}

/// Breaks the contract: blocks an executor thread, so it cannot be aborted.
#[derive(Default, Deserialize)]
struct BlocksThread;

impl Plugin for BlocksThread {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.blocks-thread", 100);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        ctx.publish(&Count(1));
        std::thread::sleep(Duration::from_millis(1500));
        Ok(Count(2))
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

static WORK_DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Writes into its work directory, and notes where it was.
#[derive(Default, Deserialize)]
struct UsesWorkDir;

impl Plugin for UsesWorkDir {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = descriptor("test.work-dir", 10_000);

    async fn run(&self, ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        tokio::fs::write(ctx.work_dir().join("scratch"), b"data")
            .await
            .map_err(|_| Stop::Failed(Why::Broken))?;
        WORK_DIRS
            .lock()
            .expect("no test panics while holding WORK_DIRS")
            .push(ctx.work_dir().to_path_buf());
        Ok(Count(1))
    }
}

const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.39",
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/fio",
        sha256: "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8",
        size: 24,
    }],
};

static DEMANDING_RAN: AtomicBool = AtomicBool::new(false);

/// Needs glibc 2.35, IPv6 and fio, so that a test can see the order of the checks.
#[derive(Default, Deserialize)]
struct Demanding;

impl Plugin for Demanding {
    type Output = Count;
    type Failure = Why;
    const DESCRIPTOR: Descriptor = Descriptor {
        requires: Requirements {
            tools: &[FIO],
            net: Net::Ipv6,
            platforms: &[],
            glibc: Some(GlibcVersion {
                major: 2,
                minor: 35,
            }),
            numa: false,
        },
        ..descriptor("test.needs", 10_000)
    };

    async fn run(&self, _ctx: &Context<Count>) -> Result<Count, Stop<Why>> {
        DEMANDING_RAN.store(true, Ordering::SeqCst);
        Ok(Count(1))
    }
}

/// Stands in for a plugin of the program, for the tests of the plan that the registry and the
/// config give: its result has the type of Geekbench 7's, the simplest to build, with the scores
/// that its config names.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Good {
    samples: u32,
}

impl Plugin for Good {
    type Output = geekbench::Output;
    type Failure = geekbench::Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.good", 10_000);

    async fn run(
        &self,
        _ctx: &Context<geekbench::Output>,
    ) -> Result<geekbench::Output, Stop<geekbench::Failure>> {
        Ok(scores(self.samples))
    }
}

/// A result of Geekbench 7's type with these scores.
fn scores(score: u32) -> geekbench::Output {
    geekbench::Output {
        scores: Some(Scores {
            version: None,
            single: score,
            multi: score,
            url: None,
        }),
        ..geekbench::Output::default()
    }
}

fn plugins() -> Vec<Entry> {
    vec![registry::entry::<Good>(Measured::CpuGeekbench)]
}

// ---------------------------------------------------------------- harness

#[derive(Default)]
struct Recorder(Vec<String>);

impl Observer<Left> for Recorder {
    fn on_start(&mut self, id: &'static str) {
        self.0.push(format!("start {id}"));
    }

    fn on_progress(&mut self, id: &'static str, line: &str) {
        self.0.push(format!("{id} {line}"));
    }

    fn on_tool(&mut self, id: &'static str, run: &ToolRun) {
        self.0.push(format!("{id} ran {}", run.command.join(" ")));
    }

    fn on_finish(&mut self, run: &Run<Left>) {
        self.0.push(format!("finish {} {:?}", run.id, run.status));
    }
}

/// Shows a progress line by cancelling its token.
struct Shows(CancellationToken);

impl Observer<Left> for Shows {
    fn on_progress(&mut self, _id: &'static str, _line: &str) {
        self.0.cancel();
    }
}

/// Runs the plan on `host` with the provisioned `tools`, with a grace period of `grace_ms` after
/// a deadline or an interrupt.
async fn run_on<R: Send + 'static>(
    plan: Vec<Planned<R>>,
    host: HostProfile,
    tools: BTreeMap<String, PathBuf>,
    grace_ms: u64,
    cancel: &CancellationToken,
    observer: &mut dyn Observer<R>,
) -> Vec<Run<R>> {
    let work_root = tempfile::tempdir().expect("create a temporary work root");
    let settings = Settings {
        grace: Duration::from_millis(grace_ms),
        work_root: work_root.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        tools: Arc::new(tools),
        failed_tools: BTreeMap::new(),
        host: Arc::new(host),
    };
    orchestrator::run_plan(plan, cancel, &settings, observer).await
}

/// [`run_on`] a host of which nothing is known, without tools.
async fn run_with<R: Send + 'static>(
    plan: Vec<Planned<R>>,
    grace_ms: u64,
    cancel: &CancellationToken,
    observer: &mut dyn Observer<R>,
) -> Vec<Run<R>> {
    let host = HostProfile::default();
    run_on(plan, host, BTreeMap::new(), grace_ms, cancel, observer).await
}

async fn run(plan: Vec<Planned<Left>>, cancel: &CancellationToken) -> (Vec<Run<Left>>, Recorder) {
    let mut recorder = Recorder::default();
    let runs = run_with(plan, 1_000, cancel, &mut recorder).await;
    (runs, recorder)
}

fn left(output: Option<u32>, stop: Option<Stop<Why>>) -> Option<Left> {
    Some(Left::Counter(Report {
        output: output.map(Count),
        stop,
    }))
}

/// Interrupts the run after a second.
fn interrupted_after_a_second() -> CancellationToken {
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    cancel
}

// ---------------------------------------------------------------- tests

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
            "test.counts ran count 1",
            "test.counts 第 2 次",
            "test.counts ran count 2",
            "finish test.counts Completed"
        ],
        "progress lines and tool runs in the order the plugin sent them"
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
async fn a_timeout_aborts_a_plugin_that_ignores_it_and_releases_its_resources() {
    let (runs, _) = run(
        vec![Planned::typed(Hanging, Left::Counter)],
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(runs[0].result, left(Some(1), None));
    assert!(runs[0].elapsed.expect("it started") >= Duration::from_secs(5));
    assert!(
        HANGING_RELEASED.load(Ordering::SeqCst),
        "an aborted plugin must drop its resources"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cpu_bound_plugin_stops_on_its_cancel_token() {
    let runs = run_with(
        vec![Planned::typed(CpuBound, Left::Counter)],
        500,
        &CancellationToken::new(),
        &mut Recorder::default(),
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(
        runs[0].result,
        left(Some(1), None),
        "it published after its loop saw the cancel token, inside the grace period"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plugin_blocking_its_thread_is_detached_not_awaited() {
    let runs = run_with(
        vec![Planned::typed(BlocksThread, Left::Counter)],
        100,
        &CancellationToken::new(),
        &mut Recorder::default(),
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(runs[0].result, left(Some(1), None));
    assert!(
        runs[0].elapsed.expect("it started") < Duration::from_secs(1),
        "deadline, grace and abort wait (100 ms each) must end the run while the plugin still \
         blocks its thread for 1500 ms; took {:?}",
        runs[0].elapsed
    );
}

#[tokio::test(start_paused = true)]
async fn an_interrupt_keeps_the_last_publish_and_cancels_the_plugins_after_it() {
    let (runs, _) = run(
        vec![
            Planned::typed(Slow, Left::Counter),
            Planned::typed(Counts, Left::Counter),
        ],
        &interrupted_after_a_second(),
    )
    .await;

    assert_eq!(runs[0].status, Status::Cancelled);
    assert_eq!(runs[0].result, left(Some(2), None));
    assert_eq!(runs[1].status, Status::Cancelled);
    assert_eq!(runs[1].result, None, "never started");
    assert_eq!(runs[1].started, None);
    assert_eq!(runs[1].elapsed, None);
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
    let (runs, _) = run(
        vec![Planned::typed(Stubborn, Left::Counter)],
        &interrupted_after_a_second(),
    )
    .await;

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
    let runs = run_with(
        vec![Planned::typed(plugin, Left::Counter)],
        1_000,
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

#[tokio::test]
async fn the_work_directory_lives_under_the_run_root_and_is_removed() {
    let work_root = tempfile::tempdir().unwrap();
    let settings = Settings {
        grace: Duration::from_secs(1),
        work_root: work_root.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        tools: Arc::default(),
        failed_tools: BTreeMap::new(),
        host: Arc::default(),
    };
    let runs = orchestrator::run_plan(
        vec![Planned::typed(UsesWorkDir, Left::Counter)],
        &CancellationToken::new(),
        &settings,
        &mut Recorder::default(),
    )
    .await;

    assert_eq!(runs[0].status, Status::Completed);
    let used = WORK_DIRS.lock().unwrap().pop().unwrap();
    assert_eq!(used.parent(), Some(work_root.path()));
    assert!(!used.exists());
}

/// A host with this C library, whose IPv6 reaches the Internet when `ipv6` is set.
fn host(libc: Libc, ipv6: bool) -> HostProfile {
    let target = "[2606:4700:4700::1111]:443".parse().expect("an address");
    let ipv6 = Family {
        targets: if ipv6 {
            vec![(target, Reach::Connected)]
        } else {
            vec![(target, Reach::NoRoute)]
        },
        address: Address::NotAsked,
    };
    HostProfile {
        libc,
        network: Network {
            ipv4: Family::default(),
            ipv6,
        },
        ..HostProfile::default()
    }
}

/// The requirement a run of `Demanding` on `host`, with `tools`, is skipped for, and how long it
/// ran.
async fn skip_reason(
    host: HostProfile,
    tools: BTreeMap<String, PathBuf>,
) -> (Unmet, Option<Duration>) {
    let runs = run_on(
        vec![Planned::typed(Demanding, Left::Counter)],
        host,
        tools,
        1_000,
        &CancellationToken::new(),
        &mut Recorder::default(),
    )
    .await;
    let Status::Skipped(Some(unmet)) = &runs[0].status else {
        panic!("{:?}", runs[0].status)
    };
    (unmet.clone(), runs[0].elapsed)
}

#[tokio::test]
async fn unmet_requirements_skip_without_running_glibc_first_then_network_then_tools() {
    let glibc = |minor| Libc::Glibc(GlibcVersion { major: 2, minor });
    let fio = || BTreeMap::from([("fio".to_owned(), PathBuf::from("/x/fio"))]);
    let needed = GlibcVersion {
        major: 2,
        minor: 35,
    };
    let found = Some(GlibcVersion {
        major: 2,
        minor: 34,
    });
    assert_eq!(
        skip_reason(host(glibc(34), false), BTreeMap::new()).await,
        (Unmet::Glibc { needed, found }, None)
    );
    assert_eq!(
        skip_reason(host(Libc::NoGlibc, true), fio()).await.0,
        Unmet::Glibc {
            needed,
            found: None
        }
    );
    assert_eq!(
        skip_reason(host(Libc::GlibcUnknown, false), BTreeMap::new())
            .await
            .0,
        Unmet::Network(Net::Ipv6),
        "an unknown glibc passes, and the network comes before the tools"
    );
    assert_eq!(
        skip_reason(host(glibc(35), true), BTreeMap::new()).await.0,
        Unmet::Tool {
            name: "fio",
            why: String::new()
        }
    );
    assert!(!DEMANDING_RAN.load(Ordering::SeqCst));
}

#[test]
fn the_host_check_accepts_the_minimum_glibc_and_newer() {
    let requires = Demanding::DESCRIPTOR.requires;
    let on = |major, minor| host(Libc::Glibc(GlibcVersion { major, minor }), true);
    assert_eq!(orchestrator::check_host(&requires, &on(2, 41)), Ok(()));
    assert_eq!(orchestrator::check_host(&requires, &on(3, 0)), Ok(()));
    assert!(orchestrator::check_host(&requires, &on(2, 34)).is_err());
    assert_eq!(
        orchestrator::check_host(&requires, &on(2, 35)),
        Ok(()),
        "tools are not the host check's business"
    );
}

#[test]
fn the_host_check_reads_each_family_from_the_profile() {
    let family = |target: &str, reach| Family {
        targets: vec![(target.parse().expect("an address"), reach)],
        address: Address::NotAsked,
    };
    let on = |ipv4, ipv6| HostProfile {
        network: Network {
            ipv4: family("1.1.1.1:443", ipv4),
            ipv6: family("[2606:4700:4700::1111]:443", ipv6),
        },
        ..HostProfile::default()
    };
    let needs = |net| Requirements {
        net,
        ..Requirements::NONE
    };
    let (up, down) = (Reach::Connected, Reach::Refused);
    // Whether a plugin that needs no network, IPv4, IPv6 or either runs on each host.
    let hosts = [
        (on(down, down), [true, false, false, false]),
        (on(up, down), [true, true, false, true]),
        (on(down, up), [true, false, true, true]),
        (on(up, up), [true, true, true, true]),
    ];
    for (host, expected) in hosts {
        let passed = [Net::None, Net::Ipv4, Net::Ipv6, Net::Any]
            .map(|net| orchestrator::check_host(&needs(net), &host).is_ok());
        assert_eq!(passed, expected, "{:?}", host.network);
    }
}

#[tokio::test]
async fn a_plugin_is_built_from_its_table_in_the_config() {
    let config = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    let planned = orchestrator::plan(&plugins(), &["test.good"], &config).expect("a valid plan");

    let runs = run_with(planned, 1_000, &CancellationToken::new(), &mut Quiet).await;

    assert_eq!(runs[0].status, Status::Completed);
    assert_eq!(
        runs[0].result,
        Some(Measured::CpuGeekbench(Report {
            output: Some(scores(2)),
            stop: None,
        }))
    );
}

#[test]
fn config_errors_are_reported_together_before_anything_runs() {
    let config = BTreeMap::from([
        (
            "test.good".to_owned(),
            serde_json::json!({ "samples": "many" }),
        ),
        ("test.goood".to_owned(), serde_json::json!({})),
    ]);
    let Err(errors) = orchestrator::plan(&plugins(), &["test.good", "test.nope"], &config) else {
        panic!("plan should fail")
    };

    assert_eq!(errors.len(), 3, "{errors:#?}");
    assert!(
        errors
            .iter()
            .any(|e| e == "配置文件中有未知测试项 `test.goood`")
    );
    assert!(errors.iter().any(|e| e == "未知测试项 `test.nope`"));
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("测试项 test.good 的配置无效：invalid type: string \"many\""))
    );
}
