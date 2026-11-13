//! Every outcome the orchestrator must produce, ported from the spike.

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
use bench::orchestrator::Run;
use bench::orchestrator::Settings;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::results::Measured;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Env;
use bench_api::Measurement;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Registration;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed::Stop;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use serde::Deserialize;

const fn descriptor(id: &'static str, timeout_ms: u64) -> Descriptor {
    Descriptor {
        id,
        title: "测试项",
        version: "0.1.0",
        requires: Requirements::NONE,
        timeout: Duration::from_millis(timeout_ms),
    }
}

// ---------------------------------------------------------------- test plugins

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Good {
    samples: u32,
}

impl Plugin for Good {
    const DESCRIPTOR: Descriptor = descriptor("test.good", 10_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("arch", "架构", ctx.env().arch.clone()));
        ctx.emit(Record::metric(
            "rate",
            "速率",
            Measurement::try_new(f64::from(self.samples) * 1.5, "MB/s")?,
        ));
        Ok(())
    }
}

static WORK_DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UsesWorkDir {}

impl Plugin for UsesWorkDir {
    const DESCRIPTOR: Descriptor = descriptor("test.work-dir", 10_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        tokio::fs::write(ctx.work_dir().join("scratch"), b"data").await?;
        WORK_DIRS
            .lock()
            .expect("no test panics while holding WORK_DIRS")
            .push(ctx.work_dir().to_path_buf());
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Failing {}

impl Plugin for Failing {
    const DESCRIPTOR: Descriptor = descriptor("test.failing", 10_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("before", "失败前的结果", "保留"));
        std::fs::read_to_string("/definitely/not/here")?;
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Panicking {}

impl Plugin for Panicking {
    const DESCRIPTOR: Descriptor = descriptor("test.panicking", 10_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("before", "崩溃前的结果", "保留"));
        panic!("boom");
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
#[serde(default, deny_unknown_fields)]
struct Demanding {}

impl Plugin for Demanding {
    const DESCRIPTOR: Descriptor = Descriptor {
        requires: Requirements {
            tools: &[FIO],
            net: Net::Ipv6,
            platforms: &[],
            glibc: Some(GlibcVersion {
                major: 2,
                minor: 35,
            }),
        },
        ..descriptor("test.needs", 10_000)
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        DEMANDING_RAN.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Emits one result, then never finishes and ignores cancellation. The guard stands in
/// for resources such as a child process spawned with `kill_on_drop`.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Hanging {}

static HANGING_RELEASED: AtomicBool = AtomicBool::new(false);

struct ReleaseFlag(&'static AtomicBool);

impl Drop for ReleaseFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl Plugin for Hanging {
    const DESCRIPTOR: Descriptor = descriptor("test.hanging", 5_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let _child_process = ReleaseFlag(&HANGING_RELEASED);
        ctx.emit(Record::metric(
            "node-1",
            "节点 1",
            Measurement::try_new(940.0, "Mbit/s")?,
        ));
        std::future::pending::<()>().await;
        Ok(())
    }
}

/// Works until cancelled, then reports how far it got (inside the grace period).
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Cooperative {}

impl Plugin for Cooperative {
    const DESCRIPTOR: Descriptor = descriptor("test.cooperative", 5_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let stop = ctx.cancel_token();
        let mut done = 0_u32;
        while !stop.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            done += 1;
        }
        ctx.emit(Record::metric(
            "nodes",
            "已完成节点",
            Measurement::try_new(f64::from(done), "个")?,
        ));
        Ok(())
    }
}

/// CPU-bound loop on a blocking thread that honours the cancel token.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CpuBound {}

impl Plugin for CpuBound {
    const DESCRIPTOR: Descriptor = descriptor("test.cpu-bound", 200);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let stop = ctx.cancel_token();
        let spins = tokio::task::spawn_blocking(move || {
            let started = std::time::Instant::now();
            let mut n: u64 = 0;
            // The time cap only keeps a broken orchestrator from hanging the test binary.
            while !stop.is_cancelled() && started.elapsed() < Duration::from_secs(5) {
                n = std::hint::black_box(n.wrapping_add(1));
            }
            n
        })
        .await?;
        ctx.emit(Record::fact("spins", "循环次数", spins.to_string()));
        Ok(())
    }
}

/// Violates the contract: blocks an executor thread, so it cannot be aborted.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BlocksThread {}

impl Plugin for BlocksThread {
    const DESCRIPTOR: Descriptor = descriptor("test.blocks-thread", 100);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("started", "已开始", "是"));
        std::thread::sleep(Duration::from_millis(1500));
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Sleeper {}

impl Plugin for Sleeper {
    const DESCRIPTOR: Descriptor = descriptor("test.sleeper", 60_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("before", "中断前的结果", "保留"));
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(())
    }
}

/// Interrupts the whole run, as Ctrl-C would, and fails or completes in the same breath. Not
/// registered: the test hands it the run's token.
#[derive(Default, Deserialize)]
struct Interrupts {
    #[serde(skip)]
    interrupt: CancellationToken,
    #[serde(skip)]
    fails: bool,
}

impl Plugin for Interrupts {
    const DESCRIPTOR: Descriptor = descriptor("test.interrupts", 10_000);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("before", "之前", "1"));
        self.interrupt.cancel();
        if self.fails {
            Err(PluginError::Fail("被中断".into()))
        } else {
            Ok(())
        }
    }
}

// ---------------------------------------------------------------- harness

fn registry() -> Vec<Registration> {
    vec![
        bench_api::register::<Good>(),
        bench_api::register::<UsesWorkDir>(),
        bench_api::register::<Failing>(),
        bench_api::register::<Panicking>(),
        bench_api::register::<Demanding>(),
        bench_api::register::<Hanging>(),
        bench_api::register::<Cooperative>(),
        bench_api::register::<CpuBound>(),
        bench_api::register::<BlocksThread>(),
        bench_api::register::<Sleeper>(),
    ]
}

#[derive(Default)]
struct Recorder(Vec<String>);

impl Observer<Measured> for Recorder {
    fn on_start(&mut self, id: &'static str) {
        self.0.push(format!("start {id}"));
    }

    fn on_finish(&mut self, run: &Run<Measured>) {
        self.0.push(format!("finish {} {:?}", run.id, run.status));
    }
}

fn env() -> Env {
    Env {
        arch: "x86_64".into(),
        ..Env::default()
    }
}

async fn run(
    selection: &[&str],
    config: BTreeMap<String, serde_json::Value>,
    env: Env,
    cancel: &CancellationToken,
    grace_ms: u64,
) -> (Vec<Run<Measured>>, Recorder) {
    let work_root = tempfile::tempdir().expect("create a temporary work root");
    let planned = orchestrator::plan(&registry(), selection, &config).expect("valid plan");
    let mut recorder = Recorder::default();
    let settings = Settings {
        grace: Duration::from_millis(grace_ms),
        work_root: work_root.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        failed_tools: BTreeMap::new(),
        host: Arc::default(),
    };
    let runs =
        orchestrator::run_plan(planned, Arc::new(env), cancel, &settings, &mut recorder).await;
    (runs, recorder)
}

/// The records a plugin of the first contract left.
fn records(run: &Run<Measured>) -> &[Record] {
    match &run.result {
        Some(Measured::Legacy(report)) => report.output.as_deref().unwrap_or_default(),
        None => &[],
    }
}

fn ids(run: &Run<Measured>) -> Vec<&str> {
    records(run).iter().map(Record::id).collect()
}

/// The plugin's own reason, when it stopped short.
fn stop(run: &Run<Measured>) -> Option<&Stop<String>> {
    match &run.result {
        Some(Measured::Legacy(report)) => report.stop.as_ref(),
        None => None,
    }
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn a_completed_run_keeps_the_records_built_from_config() {
    let config = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    let (runs, recorder) = run(
        &["test.good"],
        config,
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;

    assert_eq!(runs[0].status, Status::Completed);
    assert_eq!(runs[0].group, "test");
    assert_eq!(
        records(&runs[0])[1],
        Record::metric("rate", "速率", Measurement::try_new(3.0, "MB/s").unwrap())
    );
    assert_eq!(
        recorder.0,
        ["start test.good", "finish test.good Completed"]
    );
}

#[tokio::test]
async fn work_dir_lives_under_the_run_root_and_is_removed() {
    let work_root = tempfile::tempdir().unwrap();
    let planned = orchestrator::plan(&registry(), &["test.work-dir"], &BTreeMap::new()).unwrap();
    let settings = Settings {
        grace: Duration::from_secs(1),
        work_root: work_root.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        failed_tools: BTreeMap::new(),
        host: Arc::default(),
    };
    let runs = orchestrator::run_plan(
        planned,
        Arc::new(env()),
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

#[tokio::test]
async fn failure_keeps_records_emitted_before_it() {
    let (runs, _) = run(
        &["test.failing"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;

    assert_eq!(runs[0].status, Status::Failed(None));
    let Some(Stop::Failed(error)) = stop(&runs[0]) else {
        panic!("{:?}", runs[0].result)
    };
    assert!(error.contains("No such file or directory"), "{error}");
    assert_eq!(ids(&runs[0]), ["before"]);
}

#[tokio::test]
async fn panic_is_contained_and_the_run_continues() {
    let (runs, _) = run(
        &["test.panicking", "test.good"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;

    assert_eq!(
        runs[0].status,
        Status::Failed(Some(Fault::Panic("boom".into())))
    );
    assert_eq!(ids(&runs[0]), ["before"]);
    assert_eq!(runs[1].status, Status::Completed);
}

/// The requirement a run of `Demanding` on `env` is skipped for, and how long it ran.
async fn skip_reason(env: Env) -> (Unmet, Option<Duration>) {
    let (runs, _) = run(
        &["test.needs"],
        BTreeMap::new(),
        env,
        &CancellationToken::new(),
        1_000,
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
    let old_glibc = Env {
        libc: glibc(34),
        ..env()
    };
    let needed = GlibcVersion {
        major: 2,
        minor: 35,
    };
    let found = Some(GlibcVersion {
        major: 2,
        minor: 34,
    });
    assert_eq!(
        skip_reason(old_glibc).await,
        (Unmet::Glibc { needed, found }, None)
    );
    let musl = Env {
        libc: Libc::NoGlibc,
        tools: fio(),
        ipv6: true,
        ..env()
    };
    assert_eq!(
        skip_reason(musl).await.0,
        Unmet::Glibc {
            needed,
            found: None
        }
    );
    assert_eq!(
        skip_reason(env()).await.0,
        Unmet::Network(Net::Ipv6),
        "an unknown glibc passes, and the network comes before the tools"
    );
    let without_fio = Env {
        libc: glibc(35),
        ipv6: true,
        ..env()
    };
    assert_eq!(
        skip_reason(without_fio).await.0,
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
    let on = |major, minor| Env {
        libc: Libc::Glibc(GlibcVersion { major, minor }),
        ipv6: true,
        ..env()
    };
    assert_eq!(orchestrator::check_host(&requires, &on(2, 41)), Ok(()));
    assert_eq!(orchestrator::check_host(&requires, &on(3, 0)), Ok(()));
    assert!(orchestrator::check_host(&requires, &on(2, 34)).is_err());
    assert_eq!(
        orchestrator::check_host(&requires, &on(2, 35)),
        Ok(()),
        "tools are not the host check's business"
    );
}

#[tokio::test(start_paused = true)]
async fn timeout_keeps_partial_records_and_releases_resources() {
    let (runs, _) = run(
        &["test.hanging"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(ids(&runs[0]), ["node-1"]);
    assert!(runs[0].elapsed.unwrap() >= Duration::from_secs(5));
    assert!(
        HANGING_RELEASED.load(Ordering::SeqCst),
        "aborted plugin must drop its resources"
    );
}

#[tokio::test(start_paused = true)]
async fn cooperative_plugin_reports_inside_the_grace_period() {
    let (runs, _) = run(
        &["test.cooperative"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(ids(&runs[0]), ["nodes"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cpu_bound_plugin_stops_on_its_cancel_token() {
    let (runs, _) = run(
        &["test.cpu-bound"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        500,
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(ids(&runs[0]), ["spins"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugin_blocking_its_thread_is_detached_not_awaited() {
    let (runs, _) = run(
        &["test.blocks-thread"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        100,
    )
    .await;

    assert_eq!(runs[0].status, Status::TimedOut);
    assert_eq!(ids(&runs[0]), ["started"]);
    assert!(
        runs[0].elapsed.unwrap() < Duration::from_secs(1),
        "deadline, grace and abort wait (100 ms each) must end the run while the plugin still \
         blocks its thread for 1500 ms; took {:?}",
        runs[0].elapsed
    );
}

#[tokio::test(start_paused = true)]
async fn interrupt_cancels_the_current_and_remaining_plugins() {
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    let (runs, _) = run(
        &["test.sleeper", "test.good"],
        BTreeMap::new(),
        env(),
        &cancel,
        1_000,
    )
    .await;

    assert_eq!(runs[0].status, Status::Cancelled);
    assert_eq!(ids(&runs[0]), ["before"]);
    assert_eq!(runs[1].status, Status::Cancelled);
    assert_eq!(runs[1].elapsed, None, "never started");
}

/// Runs [`Interrupts`] alone.
async fn interrupt_and(fails: bool) -> Vec<Run<Measured>> {
    let cancel = CancellationToken::new();
    let plugin = Interrupts {
        interrupt: cancel.clone(),
        fails,
    };
    let planned = vec![Planned::legacy(
        Interrupts::DESCRIPTOR,
        Arc::new(plugin),
        Measured::Legacy,
    )];
    let work_root = tempfile::tempdir().expect("create a temporary work root");
    let settings = Settings {
        grace: Duration::from_millis(1_000),
        work_root: work_root.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        failed_tools: BTreeMap::new(),
        host: Arc::default(),
    };
    orchestrator::run_plan(
        planned,
        Arc::new(env()),
        &cancel,
        &settings,
        &mut Recorder::default(),
    )
    .await
}

#[tokio::test]
async fn a_failure_once_the_run_is_interrupted_counts_as_interrupted() {
    let runs = interrupt_and(true).await;

    assert_eq!(runs[0].status, Status::Cancelled);
    assert_eq!(ids(&runs[0]), ["before"]);
    assert_eq!(
        stop(&runs[0]),
        None,
        "the reason is the interrupt's, not the plugin's"
    );
}

#[tokio::test]
async fn a_plugin_that_has_ended_when_the_interrupt_comes_is_taken_at_its_word() {
    // Its end and the interrupt are both there when the orchestrator looks, and no run may pick
    // one of them by chance: hence many runs.
    for _ in 0..20 {
        let runs = interrupt_and(false).await;

        assert_eq!(runs[0].status, Status::Completed);
        assert_eq!(ids(&runs[0]), ["before"]);
    }
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
    let Err(errors) = orchestrator::plan(&registry(), &["test.good", "test.nope"], &config) else {
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
