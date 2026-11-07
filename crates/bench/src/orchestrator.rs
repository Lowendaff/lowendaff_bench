//! Sequential orchestrator: plan, preflight, then run each plugin with timeout and
//! interrupt handling. User-facing messages are Chinese.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::DynPlugin;
use bench_api::Env;
use bench_api::Net;
use bench_api::Platform;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Registration;
use bench_api::Requirements;
use tokio::task::JoinError;
use tokio::task::JoinHandle;
use tokio::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Completed,
    Skipped {
        reason: String,
    },
    Failed {
        error: String,
    },
    TimedOut,
    /// Interrupted while running, or never started because the run was interrupted.
    Cancelled,
}

#[derive(Debug)]
pub struct Run {
    pub plugin: &'static str,
    pub outcome: Outcome,
    /// `None` when the plugin never started.
    pub elapsed: Option<Duration>,
    pub records: Vec<Record>,
}

/// A selected plugin, already built from its configuration.
pub struct Planned {
    descriptor: Descriptor,
    plugin: Arc<dyn DynPlugin>,
}

impl Planned {
    /// Static facts about the planned plugin.
    #[must_use]
    pub fn descriptor(&self) -> Descriptor {
        self.descriptor
    }
}

/// Live progress sink (terminal renderer, test recorder).
pub trait Observer {
    fn on_start(&mut self, _plugin: &Descriptor) {}
    fn on_record(&mut self, _plugin: &Descriptor, _record: &Record) {}
    fn on_finish(&mut self, _plugin: &Descriptor, _outcome: &Outcome) {}
}

pub struct Settings {
    /// Time a plugin gets to wind down after its cancel token fires, before it is aborted.
    pub grace: Duration,
    /// Parent of the per-plugin work directories; on the tested disk, not on `/tmp`.
    pub work_root: PathBuf,
}

/// Resolves the selection and builds every plugin, so config errors surface before
/// anything runs. All errors are returned together.
pub fn plan(
    registry: &[Registration],
    selection: &[&str],
    config: &BTreeMap<String, serde_json::Value>,
) -> Result<Vec<Planned>, Vec<String>> {
    let mut errors: Vec<String> = config
        .keys()
        .filter(|key| {
            !registry
                .iter()
                .any(|registration| registration.descriptor.id == key.as_str())
        })
        .map(|key| format!("配置文件中有未知测试项 `{key}`"))
        .collect();
    let mut planned = Vec::new();
    for id in selection {
        let Some(registration) = registry
            .iter()
            .find(|registration| registration.descriptor.id == *id)
        else {
            errors.push(format!("未知测试项 `{id}`"));
            continue;
        };
        match registration.build(config.get(*id)) {
            Ok(plugin) => planned.push(Planned {
                descriptor: registration.descriptor,
                plugin,
            }),
            Err(err) => errors.push(format!("测试项 {id} 的配置无效：{err}")),
        }
    }
    if errors.is_empty() {
        Ok(planned)
    } else {
        Err(errors)
    }
}

/// Splits the plan by platform (design D-16): the plugins that apply here, and the titles
/// of the test items that run here in no variant, for the 本机不适用 line.
#[must_use]
pub fn for_platform(plan: Vec<Planned>, platform: Platform) -> (Vec<Planned>, Vec<&'static str>) {
    let (applicable, elsewhere): (Vec<Planned>, Vec<Planned>) = plan
        .into_iter()
        .partition(|planned| planned.descriptor.requires.applies_to(platform));
    let mut titles = Vec::new();
    for planned in &elsewhere {
        let title = planned.descriptor.title;
        let runs_here = applicable
            .iter()
            .any(|other| other.descriptor.title == title);
        if !runs_here && !titles.contains(&title) {
            titles.push(title);
        }
    }
    (applicable, titles)
}

pub fn preflight(requires: &Requirements, env: &Env) -> Result<(), String> {
    if let Some(tool) = requires
        .tools
        .iter()
        .find(|tool| !env.tools.contains_key(tool.name))
    {
        return Err(format!("工具 {} 不可用", tool.name));
    }
    match requires.net {
        Net::Any if !(env.ipv4 || env.ipv6) => Err("需要网络连接".into()),
        Net::Ipv4 if !env.ipv4 => Err("需要 IPv4 网络连接".into()),
        Net::Ipv6 if !env.ipv6 => Err("需要 IPv6 网络连接".into()),
        _ => Ok(()),
    }
}

pub async fn run_plan(
    plan: Vec<Planned>,
    env: Arc<Env>,
    cancel: &CancellationToken,
    settings: &Settings,
    observer: &mut dyn Observer,
) -> Vec<Run> {
    let mut runs = Vec::with_capacity(plan.len());
    for planned in plan {
        let descriptor = planned.descriptor;
        let (outcome, records, elapsed) = if cancel.is_cancelled() {
            (Outcome::Cancelled, Vec::new(), None)
        } else if let Err(reason) = preflight(&descriptor.requires, &env) {
            (Outcome::Skipped { reason }, Vec::new(), None)
        } else {
            observer.on_start(&descriptor);
            let started = Instant::now();
            let (outcome, records) =
                run_one(&planned, env.clone(), cancel, settings, observer).await;
            (outcome, records, Some(started.elapsed()))
        };
        observer.on_finish(&descriptor, &outcome);
        runs.push(Run {
            plugin: descriptor.id,
            outcome,
            elapsed,
            records,
        });
    }
    runs
}

async fn run_one(
    planned: &Planned,
    env: Arc<Env>,
    global: &CancellationToken,
    settings: &Settings,
    observer: &mut dyn Observer,
) -> (Outcome, Vec<Record>) {
    let descriptor = planned.descriptor;
    let work = match tempfile::Builder::new()
        .prefix(descriptor.id)
        .tempdir_in(&settings.work_root)
    {
        Ok(dir) => dir,
        Err(err) => {
            return (
                Outcome::Failed {
                    error: format!("无法创建工作目录：{err}"),
                },
                Vec::new(),
            );
        }
    };
    let token = global.child_token();
    let deadline = Instant::now() + descriptor.timeout;
    let (ctx, mut receiver) = Context::new(env, deadline, work.path().to_path_buf(), token.clone());
    let plugin = planned.plugin.clone();
    let mut task = tokio::spawn(async move { plugin.run_boxed(&ctx).await });

    let mut records = Vec::new();
    let outcome = loop {
        tokio::select! {
            Some(record) = receiver.recv() => {
                observer.on_record(&descriptor, &record);
                records.push(record);
            }
            joined = &mut task => break outcome_of(joined),
            _ = tokio::time::sleep_until(deadline) => {
                stop(&mut task, &token, settings.grace).await;
                break Outcome::TimedOut;
            }
            _ = global.cancelled() => {
                stop(&mut task, &token, settings.grace).await;
                break Outcome::Cancelled;
            }
        }
    };
    // Always drain: `select!` may see the task finish before its last records, and
    // records emitted during the grace period arrive only here.
    while let Ok(record) = receiver.try_recv() {
        observer.on_record(&descriptor, &record);
        records.push(record);
    }
    drop(work);
    (outcome, records)
}

/// Cooperative stop, then abort. A plugin that blocks its thread cannot be aborted;
/// after a second grace period it is detached and keeps running in the background.
async fn stop(
    task: &mut JoinHandle<Result<(), PluginError>>,
    token: &CancellationToken,
    grace: Duration,
) {
    token.cancel();
    if tokio::time::timeout(grace, &mut *task).await.is_ok() {
        return;
    }
    task.abort();
    let _ = tokio::time::timeout(grace, &mut *task).await;
}

fn outcome_of(joined: Result<Result<(), PluginError>, JoinError>) -> Outcome {
    match joined {
        Ok(Ok(())) => Outcome::Completed,
        Ok(Err(PluginError::Skip(reason))) => Outcome::Skipped { reason },
        Ok(Err(PluginError::Fail(error))) => Outcome::Failed { error },
        Err(err) if err.is_panic() => Outcome::Failed {
            error: format!("插件崩溃：{}", panic_message(err.into_panic())),
        },
        Err(err) => Outcome::Failed {
            error: format!("任务异常结束：{err}"),
        },
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "（非文本的 panic 信息）".into())
}
