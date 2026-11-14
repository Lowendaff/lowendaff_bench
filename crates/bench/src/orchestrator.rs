//! The orchestrator: plans the selected plugins, checks each one's requirements, runs it with
//! its timeout and the interrupt in view, and keeps what it left in the result enum, with the
//! run's status. Plugins of both contracts run; those of the first contract leave their records.
//! User-facing text is the reporter's.

use std::any::Any;
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::DynPlugin;
use bench_api::Env;
use bench_api::Net;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::typed;
use bench_api::typed::Progress;
use bench_api::typed::Stop;
use bench_api::typed::ToolRun;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::platform::Platform;
use bench_host::profile::HostProfile;
use tokio::task::JoinError;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::registry::Entry;
use crate::results::Measured;
use crate::results::Report;

/// What became of one plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The plugin returned its result.
    Completed,
    /// Skipped by the plugin itself, whose reason is in its result, or for a requirement.
    Skipped(Option<Unmet>),
    /// Failed in the plugin, whose reason is in its result, or by a fault around it.
    Failed(Option<Fault>),
    /// Stopped at its deadline.
    TimedOut,
    /// Interrupted while running, or never started because the run was interrupted.
    Cancelled,
}

/// A requirement that the host does not meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unmet {
    /// glibc older than `needed`, or none at all when `found` is `None`.
    Glibc {
        /// The oldest glibc the plugin's tools run on.
        needed: GlibcVersion,
        /// The host's glibc.
        found: Option<GlibcVersion>,
    },
    /// The network the plugin needs is not reachable.
    Network(Net),
    /// A tool that could not be provisioned, and why; empty when it was not provisioned at all.
    Tool {
        /// The tool's name.
        name: &'static str,
        /// The provisioning's reason.
        why: String,
    },
}

/// A fault around a plugin rather than in its measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The plugin panicked with this message.
    Panic(String),
    /// Its work directory could not be created.
    WorkDir(String),
    /// Its task ended otherwise, with this error.
    Aborted(String),
}

/// One plugin's run, as the reporter reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Run<R> {
    /// The plugin's id.
    pub id: &'static str,
    /// The id's first segment, such as `cpu` for `cpu.sysbench`.
    pub group: &'static str,
    /// The plugin's name: its title in the first contract, or the reporter's name for it.
    pub title: Option<&'static str>,
    /// When it started, counted from the start of the run; `None` when it never started.
    pub started: Option<Duration>,
    /// How long it ran; `None` when it never started.
    pub elapsed: Option<Duration>,
    /// What became of it.
    pub status: Status,
    /// What it left; `None` when it never started or left nothing.
    pub result: Option<R>,
}

/// The id's first dot-separated segment.
#[must_use]
pub fn group_of(id: &str) -> &str {
    id.split('.').next().unwrap_or(id)
}

/// A selected plugin, built from its configuration and ready to run.
pub struct Planned<R> {
    id: &'static str,
    pub(crate) title: Option<&'static str>,
    requires: Requirements,
    timeout: Duration,
    runner: Arc<dyn Runner<R>>,
}

impl<R: Send + 'static> Planned<R> {
    /// A plugin of the first contract, whose records `wrap` turns into a result.
    #[must_use]
    pub fn legacy(
        descriptor: bench_api::Descriptor,
        plugin: Arc<dyn DynPlugin>,
        wrap: fn(Report<Vec<Record>, String>) -> R,
    ) -> Self {
        Self {
            id: descriptor.id,
            title: Some(descriptor.title),
            requires: descriptor.requires,
            timeout: descriptor.timeout,
            runner: Arc::new(LegacyRunner { plugin, wrap }),
        }
    }

    /// A plugin of the typed contract, whose report `wrap` turns into a result.
    #[must_use]
    pub fn typed<P: typed::Plugin>(
        plugin: P,
        wrap: fn(Report<P::Output, P::Failure>) -> R,
    ) -> Self {
        Self {
            id: P::DESCRIPTOR.id,
            title: None,
            requires: P::DESCRIPTOR.requires,
            timeout: P::DESCRIPTOR.timeout,
            runner: Arc::new(TypedRunner {
                plugin: Arc::new(plugin),
                wrap,
            }),
        }
    }
}

impl<R> Planned<R> {
    /// The plugin's id.
    #[must_use]
    pub fn id(&self) -> &'static str {
        self.id
    }

    /// The plugin's name: its title in the first contract, or the reporter's name for it.
    #[must_use]
    pub fn title(&self) -> Option<&'static str> {
        self.title
    }

    /// What the plugin needs.
    #[must_use]
    pub fn requires(&self) -> &Requirements {
        &self.requires
    }
}

/// Hears about each plugin as the run goes: the progress log, or a recorder in tests.
pub trait Observer<R>: Send {
    /// A plugin starts.
    fn on_start(&mut self, _id: &'static str) {}
    /// A plugin reports progress.
    fn on_progress(&mut self, _id: &'static str, _line: &str) {}
    /// A plugin ran a tool.
    fn on_tool(&mut self, _id: &'static str, _run: &ToolRun) {}
    /// A plugin's run is over, or it never started.
    fn on_finish(&mut self, _run: &Run<R>) {}
}

/// An observer that ignores everything.
pub struct Quiet;

impl<R> Observer<R> for Quiet {}

/// How plugins run.
pub struct Settings {
    /// Time a plugin gets to wind down after its cancel token fires, before it is aborted.
    pub grace: Duration,
    /// Parent of the per-plugin work directories; on the tested disk, not on `/tmp`.
    pub work_root: PathBuf,
    /// The start of the run, from which [`Run::started`] counts.
    pub origin: Instant,
    /// Why each tool that could not be provisioned failed.
    pub failed_tools: BTreeMap<String, String>,
    /// The host, as the run read it, for the plugins of the typed contract.
    pub host: Arc<HostProfile>,
}

/// Resolves the selection and builds every selected plugin, so config errors surface before
/// anything runs. All errors are returned together.
pub fn plan(
    registry: &[Entry],
    selection: &[&str],
    config: &BTreeMap<String, serde_json::Value>,
) -> Result<Vec<Planned<Measured>>, Vec<String>> {
    let mut errors: Vec<String> = config
        .keys()
        .filter(|key| !registry.iter().any(|entry| entry.id == key.as_str()))
        .map(|key| format!("配置文件中有未知测试项 `{key}`"))
        .collect();
    let mut planned = Vec::new();
    for id in selection {
        let Some(entry) = registry.iter().find(|entry| entry.id == *id) else {
            errors.push(format!("未知测试项 `{id}`"));
            continue;
        };
        match entry.build(config.get(*id)) {
            Ok(plugin) => planned.push(plugin),
            Err(err) => errors.push(format!("测试项 {id} 的配置无效：{err}")),
        }
    }
    if errors.is_empty() {
        Ok(planned)
    } else {
        Err(errors)
    }
}

/// Splits the plan by platform: the plugins that apply here, and the titles of the test items
/// that run here in no variant, for the 本机不适用 line.
#[must_use]
pub fn for_platform<R>(
    plan: Vec<Planned<R>>,
    platform: Platform,
) -> (Vec<Planned<R>>, Vec<&'static str>) {
    let (applicable, elsewhere): (Vec<Planned<R>>, Vec<Planned<R>>) = plan
        .into_iter()
        .partition(|planned| planned.requires.applies_to(platform));
    let mut titles = Vec::new();
    for planned in &elsewhere {
        let title = planned.title.unwrap_or(planned.id);
        let runs_here = applicable
            .iter()
            .any(|other| other.title.unwrap_or(other.id) == title);
        if !runs_here && !titles.contains(&title) {
            titles.push(title);
        }
    }
    (applicable, titles)
}

/// What the host must offer a plugin besides its tools, checked before any download: its
/// glibc, then its network.
pub fn check_host(requires: &Requirements, env: &Env) -> Result<(), Unmet> {
    if let Some(needed) = requires.glibc {
        match env.libc {
            Libc::Glibc(found) if found < needed => {
                return Err(Unmet::Glibc {
                    needed,
                    found: Some(found),
                });
            }
            Libc::NoGlibc => {
                return Err(Unmet::Glibc {
                    needed,
                    found: None,
                });
            }
            Libc::Glibc(_) | Libc::GlibcUnknown => {}
        }
    }
    let reachable = match requires.net {
        Net::None => true,
        Net::Any => env.ipv4 || env.ipv6,
        Net::Ipv4 => env.ipv4,
        Net::Ipv6 => env.ipv6,
    };
    if reachable {
        Ok(())
    } else {
        Err(Unmet::Network(requires.net))
    }
}

/// Everything a plugin needs before it runs: [`check_host`], then its tools.
pub fn preflight(
    requires: &Requirements,
    env: &Env,
    failed_tools: &BTreeMap<String, String>,
) -> Result<(), Unmet> {
    check_host(requires, env)?;
    match requires
        .tools
        .iter()
        .find(|tool| !env.tools.contains_key(tool.name))
    {
        Some(tool) => Err(Unmet::Tool {
            name: tool.name,
            why: failed_tools.get(tool.name).cloned().unwrap_or_default(),
        }),
        None => Ok(()),
    }
}

/// Runs the plan in order and returns every plugin's run.
pub async fn run_plan<R: Send + 'static>(
    plan: Vec<Planned<R>>,
    env: Arc<Env>,
    cancel: &CancellationToken,
    settings: &Settings,
    observer: &mut dyn Observer<R>,
) -> Vec<Run<R>> {
    let mut runs = Vec::with_capacity(plan.len());
    for planned in plan {
        let mut run = Run {
            id: planned.id,
            group: group_of(planned.id),
            title: planned.title,
            started: None,
            elapsed: None,
            status: Status::Cancelled,
            result: None,
        };
        if !cancel.is_cancelled() {
            match preflight(&planned.requires, &env, &settings.failed_tools) {
                Err(unmet) => run.status = Status::Skipped(Some(unmet)),
                Ok(()) => {
                    observer.on_start(planned.id);
                    let begun = Instant::now();
                    let (status, result) =
                        run_one(&planned, env.clone(), cancel, settings, observer).await;
                    run.started = Some(begun.duration_since(settings.origin));
                    run.elapsed = Some(begun.elapsed());
                    run.status = status;
                    run.result = result;
                }
            }
        }
        observer.on_finish(&run);
        runs.push(run);
    }
    runs
}

async fn run_one<R: Send + 'static>(
    planned: &Planned<R>,
    env: Arc<Env>,
    global: &CancellationToken,
    settings: &Settings,
    observer: &mut dyn Observer<R>,
) -> (Status, Option<R>) {
    let work = match tempfile::Builder::new()
        .prefix(planned.id)
        .tempdir_in(&settings.work_root)
    {
        Ok(dir) => dir,
        Err(err) => return (Status::Failed(Some(Fault::WorkDir(err.to_string()))), None),
    };
    let launch = Launch {
        id: planned.id,
        env,
        host: Arc::clone(&settings.host),
        deadline: Instant::now() + planned.timeout,
        work_dir: work.path().to_path_buf(),
        token: global.child_token(),
        global,
        grace: settings.grace,
        observer,
    };
    let ended = planned.runner.run(launch).await;
    drop(work);
    ended
}

/// Everything a runner needs for one run.
struct Launch<'a, R> {
    id: &'static str,
    env: Arc<Env>,
    host: Arc<HostProfile>,
    deadline: Instant,
    work_dir: PathBuf,
    /// The plugin's own token, a child of `global`.
    token: CancellationToken,
    global: &'a CancellationToken,
    grace: Duration,
    observer: &'a mut dyn Observer<R>,
}

type Running<'a, R> = Pin<Box<dyn Future<Output = (Status, Option<R>)> + Send + 'a>>;

/// Runs one plugin of one contract to its end: completion, a stop, its deadline or the
/// interrupt.
trait Runner<R>: Send + Sync {
    fn run<'a>(&'a self, launch: Launch<'a, R>) -> Running<'a, R>;
}

/// How a plugin's task ended, before the result is put together.
enum Ending<T> {
    Joined(Result<T, JoinError>),
    TimedOut,
    Cancelled,
}

struct LegacyRunner<R> {
    plugin: Arc<dyn DynPlugin>,
    wrap: fn(Report<Vec<Record>, String>) -> R,
}

impl<R: Send + 'static> Runner<R> for LegacyRunner<R> {
    fn run<'a>(&'a self, launch: Launch<'a, R>) -> Running<'a, R> {
        Box::pin(async move {
            let (ctx, mut receiver) = Context::new(
                launch.env,
                launch.deadline,
                launch.work_dir,
                launch.token.clone(),
            );
            let plugin = self.plugin.clone();
            let mut task = tokio::spawn(async move { plugin.run_boxed(&ctx).await });
            let mut records = Vec::new();
            // In this order, so that a plugin that has already ended is taken at its word, and
            // a stream of records cannot hold off its deadline or the interrupt.
            let ending = loop {
                tokio::select! {
                    biased;
                    joined = &mut task => break Ending::Joined(joined),
                    () = tokio::time::sleep_until(launch.deadline) => {
                        stop(&mut task, &launch.token, launch.grace).await;
                        break Ending::TimedOut;
                    }
                    () = launch.global.cancelled() => {
                        stop(&mut task, &launch.token, launch.grace).await;
                        break Ending::Cancelled;
                    }
                    Some(record) = receiver.recv() => records.push(record),
                }
            };
            // Always drain: `select!` may see the task finish before its last records, and
            // records emitted during the grace period arrive only here.
            while let Ok(record) = receiver.try_recv() {
                records.push(record);
            }
            // A plugin that stops short once the run is interrupted stops because of it.
            let interrupted = launch.global.is_cancelled();
            let (status, stop) = match ending {
                Ending::Joined(Ok(Ok(()))) => (Status::Completed, None),
                Ending::Joined(Ok(Err(_))) if interrupted => (Status::Cancelled, None),
                Ending::Joined(Ok(Err(PluginError::Skip(reason)))) => {
                    (Status::Skipped(None), Some(Stop::Skipped(reason)))
                }
                Ending::Joined(Ok(Err(PluginError::Fail(reason)))) => {
                    (Status::Failed(None), Some(Stop::Failed(reason)))
                }
                Ending::Joined(Err(err)) => (Status::Failed(Some(fault(err))), None),
                Ending::TimedOut => (Status::TimedOut, None),
                Ending::Cancelled => (Status::Cancelled, None),
            };
            let report = Report {
                output: Some(records),
                stop,
            };
            (status, Some((self.wrap)(report)))
        })
    }
}

struct TypedRunner<P: typed::Plugin, R> {
    plugin: Arc<P>,
    wrap: fn(Report<P::Output, P::Failure>) -> R,
}

impl<P: typed::Plugin, R: Send + 'static> Runner<R> for TypedRunner<P, R> {
    fn run<'a>(&'a self, launch: Launch<'a, R>) -> Running<'a, R> {
        Box::pin(async move {
            let tools = Arc::new(launch.env.tools.clone());
            let (ctx, mut channels) = typed::Context::<P::Output>::new(
                launch.host,
                tools,
                launch.deadline,
                launch.work_dir,
                launch.token.clone(),
            );
            let plugin = self.plugin.clone();
            let mut task = tokio::spawn(async move { plugin.run(&ctx).await });
            // In the legacy runner's order, for the same reasons.
            let ending = loop {
                tokio::select! {
                    biased;
                    joined = &mut task => break Ending::Joined(joined),
                    () = tokio::time::sleep_until(launch.deadline) => {
                        stop(&mut task, &launch.token, launch.grace).await;
                        break Ending::TimedOut;
                    }
                    () = launch.global.cancelled() => {
                        stop(&mut task, &launch.token, launch.grace).await;
                        break Ending::Cancelled;
                    }
                    Some(progress) = channels.progress.recv() => {
                        tell(launch.observer, launch.id, progress);
                    }
                }
            };
            while let Ok(progress) = channels.progress.try_recv() {
                tell(launch.observer, launch.id, progress);
            }
            let published = channels.published.borrow().clone();
            // A plugin that stops short once the run is interrupted stops because of it.
            let interrupted = launch.global.is_cancelled();
            let (status, report) = match ending {
                Ending::Joined(Ok(Ok(output))) => (
                    Status::Completed,
                    Report {
                        output: Some(output),
                        stop: None,
                    },
                ),
                Ending::Joined(Ok(Err(_))) if interrupted => (
                    Status::Cancelled,
                    Report {
                        output: published,
                        stop: None,
                    },
                ),
                Ending::Joined(Ok(Err(stop))) => {
                    let status = match stop {
                        Stop::Skipped(_) => Status::Skipped(None),
                        Stop::Failed(_) => Status::Failed(None),
                    };
                    let report = Report {
                        output: published,
                        stop: Some(stop),
                    };
                    (status, report)
                }
                Ending::Joined(Err(err)) => (
                    Status::Failed(Some(fault(err))),
                    Report {
                        output: published,
                        stop: None,
                    },
                ),
                Ending::TimedOut => (
                    Status::TimedOut,
                    Report {
                        output: published,
                        stop: None,
                    },
                ),
                Ending::Cancelled => (
                    Status::Cancelled,
                    Report {
                        output: published,
                        stop: None,
                    },
                ),
            };
            (status, Some((self.wrap)(report)))
        })
    }
}

/// Hands a plugin's progress to the observer.
fn tell<R>(observer: &mut dyn Observer<R>, id: &'static str, progress: Progress) {
    match progress {
        Progress::Line(line) => observer.on_progress(id, &line),
        Progress::Tool(run) => observer.on_tool(id, &run),
    }
}

/// Cooperative stop, then abort. A plugin that blocks its thread cannot be aborted; after a
/// second grace period it is detached and keeps running in the background.
async fn stop<T>(task: &mut JoinHandle<T>, token: &CancellationToken, grace: Duration) {
    token.cancel();
    if tokio::time::timeout(grace, &mut *task).await.is_ok() {
        return;
    }
    task.abort();
    let _ = tokio::time::timeout(grace, &mut *task).await;
}

fn fault(err: JoinError) -> Fault {
    if err.is_panic() {
        Fault::Panic(panic_message(err.into_panic()))
    } else {
        Fault::Aborted(err.to_string())
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "（非文本的 panic 信息）".into())
}
