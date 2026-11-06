//! Every outcome the orchestrator must produce (design §8), ported from the spike.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Observer;
use bench::orchestrator::Outcome;
use bench::orchestrator::Run;
use bench::orchestrator::Settings;
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
use bench_api::ToolSpec;
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
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/fio",
        sha256: "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8",
        size: 24,
    }],
};

static NEEDS_TOOL_RAN: AtomicBool = AtomicBool::new(false);

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NeedsToolAndIpv6 {}

impl Plugin for NeedsToolAndIpv6 {
    const DESCRIPTOR: Descriptor = Descriptor {
        requires: Requirements {
            tools: &[FIO],
            net: Net::Ipv6,
            platforms: &[],
        },
        ..descriptor("test.needs", 10_000)
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        NEEDS_TOOL_RAN.store(true, Ordering::SeqCst);
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

// ---------------------------------------------------------------- harness

fn registry() -> Vec<Registration> {
    vec![
        bench_api::register::<Good>(),
        bench_api::register::<UsesWorkDir>(),
        bench_api::register::<Failing>(),
        bench_api::register::<Panicking>(),
        bench_api::register::<NeedsToolAndIpv6>(),
        bench_api::register::<Hanging>(),
        bench_api::register::<Cooperative>(),
        bench_api::register::<CpuBound>(),
        bench_api::register::<BlocksThread>(),
        bench_api::register::<Sleeper>(),
    ]
}

#[derive(Default)]
struct Recorder(Vec<String>);

impl Observer for Recorder {
    fn on_start(&mut self, p: &Descriptor) {
        self.0.push(format!("start {}", p.id));
    }

    fn on_record(&mut self, p: &Descriptor, r: &Record) {
        self.0.push(format!("record {} {}", p.id, r.id()));
    }

    fn on_finish(&mut self, p: &Descriptor, o: &Outcome) {
        self.0.push(format!("finish {} {o:?}", p.id));
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
) -> (Vec<Run>, Recorder) {
    let work_root = tempfile::tempdir().expect("create a temporary work root");
    let planned = orchestrator::plan(&registry(), selection, &config).expect("valid plan");
    let mut recorder = Recorder::default();
    let settings = Settings {
        grace: Duration::from_millis(grace_ms),
        work_root: work_root.path().to_path_buf(),
    };
    let runs =
        orchestrator::run_plan(planned, Arc::new(env), cancel, &settings, &mut recorder).await;
    (runs, recorder)
}

fn ids(run: &Run) -> Vec<&str> {
    run.records.iter().map(Record::id).collect()
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn completed_run_streams_records_built_from_config() {
    let config = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    let (runs, recorder) = run(
        &["test.good"],
        config,
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;

    assert_eq!(runs[0].outcome, Outcome::Completed);
    assert_eq!(
        runs[0].records[1],
        Record::metric("rate", "速率", Measurement::try_new(3.0, "MB/s").unwrap())
    );
    assert_eq!(
        recorder.0,
        [
            "start test.good",
            "record test.good arch",
            "record test.good rate",
            "finish test.good Completed"
        ]
    );
}

#[tokio::test]
async fn work_dir_lives_under_the_run_root_and_is_removed() {
    let work_root = tempfile::tempdir().unwrap();
    let planned = orchestrator::plan(&registry(), &["test.work-dir"], &BTreeMap::new()).unwrap();
    let settings = Settings {
        grace: Duration::from_secs(1),
        work_root: work_root.path().to_path_buf(),
    };
    let runs = orchestrator::run_plan(
        planned,
        Arc::new(env()),
        &CancellationToken::new(),
        &settings,
        &mut Recorder::default(),
    )
    .await;

    assert_eq!(runs[0].outcome, Outcome::Completed);
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

    let Outcome::Failed { error } = &runs[0].outcome else {
        panic!("{:?}", runs[0].outcome)
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
        runs[0].outcome,
        Outcome::Failed {
            error: "插件崩溃：boom".into()
        }
    );
    assert_eq!(ids(&runs[0]), ["before"]);
    assert_eq!(runs[1].outcome, Outcome::Completed);
}

#[tokio::test]
async fn unmet_requirements_skip_without_running() {
    let (runs, _) = run(
        &["test.needs"],
        BTreeMap::new(),
        env(),
        &CancellationToken::new(),
        1_000,
    )
    .await;
    assert_eq!(
        runs[0].outcome,
        Outcome::Skipped {
            reason: "工具 fio 不可用".into()
        }
    );

    let with_fio = Env {
        tools: BTreeMap::from([("fio".to_owned(), PathBuf::from("/x/fio"))]),
        ..env()
    };
    let (runs, _) = run(
        &["test.needs"],
        BTreeMap::new(),
        with_fio,
        &CancellationToken::new(),
        1_000,
    )
    .await;
    assert_eq!(
        runs[0].outcome,
        Outcome::Skipped {
            reason: "需要 IPv6 网络连接".into()
        }
    );
    assert_eq!(runs[0].elapsed, None);
    assert!(!NEEDS_TOOL_RAN.load(Ordering::SeqCst));
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

    assert_eq!(runs[0].outcome, Outcome::TimedOut);
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

    assert_eq!(runs[0].outcome, Outcome::TimedOut);
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

    assert_eq!(runs[0].outcome, Outcome::TimedOut);
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

    assert_eq!(runs[0].outcome, Outcome::TimedOut);
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

    assert_eq!(runs[0].outcome, Outcome::Cancelled);
    assert_eq!(ids(&runs[0]), ["before"]);
    assert_eq!(runs[1].outcome, Outcome::Cancelled);
    assert_eq!(runs[1].elapsed, None, "never started");
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
