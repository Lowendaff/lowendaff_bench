//! The typed plugin contract. A plugin returns a result of its own type, or its own reason to
//! skip or to fail; it publishes the result as it goes and reports its progress, and the
//! orchestrator and the reporter do the rest. Plugins move to it group by group; it stays in this
//! module, beside the first contract at the crate root, until every plugin has moved.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_host::profile::HostProfile;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::CancellationToken;
use crate::Requirements;

/// Static facts about a plugin type.
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    /// Stable id `<group>.<name>`, such as `memory.stream`.
    pub id: &'static str,
    /// Preconditions the orchestrator checks before calling `run`.
    pub requires: Requirements,
    /// Hard limit for one run.
    pub timeout: Duration,
}

/// Why a plugin stopped short of a complete result, with the plugin's own reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Stop<F> {
    /// The measurement does not apply here.
    Skipped(F),
    /// The measurement went wrong.
    Failed(F),
}

/// A tool that a result came from, with the version the tool reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolVersion {
    /// The tool's name, as in its pin.
    pub name: &'static str,
    /// The version the tool reported about itself.
    pub version: String,
}

/// What every typed result tells besides its own data.
pub trait Measurement: Serialize + Clone + Send + Sync + 'static {
    /// The tools used, with the versions they reported.
    fn tools(&self) -> Vec<ToolVersion>;
}

/// A test item whose result has a type of its own.
pub trait Plugin: DeserializeOwned + Default + Send + Sync + 'static {
    /// The plugin's result.
    type Output: Measurement;
    /// Why the plugin skipped or failed.
    type Failure: Serialize + Send + 'static;
    /// Static facts about the plugin.
    const DESCRIPTOR: Descriptor;
    /// Runs the measurement: the complete result, or why there is none.
    fn run(
        &self,
        ctx: &Context<Self::Output>,
    ) -> impl Future<Output = Result<Self::Output, Stop<Self::Failure>>> + Send;
}

/// What a plugin gets for one run.
pub struct Context<O> {
    host: Arc<HostProfile>,
    tools: Arc<BTreeMap<String, PathBuf>>,
    cancel: CancellationToken,
    deadline: Instant,
    work_dir: PathBuf,
    progress: UnboundedSender<String>,
    published: watch::Sender<Option<O>>,
}

/// The orchestrator's end of a [`Context`].
pub struct Channels<O> {
    /// The plugin's progress lines, in order.
    pub progress: UnboundedReceiver<String>,
    /// The last result that the plugin published; `None` until it publishes one.
    pub published: watch::Receiver<Option<O>>,
}

impl<O: Measurement> Context<O> {
    /// A context for one run, with the tools that were provisioned, and the orchestrator's end of
    /// its channels.
    #[must_use]
    pub fn new(
        host: Arc<HostProfile>,
        tools: Arc<BTreeMap<String, PathBuf>>,
        deadline: Instant,
        work_dir: PathBuf,
        cancel: CancellationToken,
    ) -> (Self, Channels<O>) {
        let (progress, progress_receiver) = mpsc::unbounded_channel();
        let (published, published_receiver) = watch::channel(None);
        let ctx = Self {
            host,
            tools,
            cancel,
            deadline,
            work_dir,
            progress,
            published,
        };
        let channels = Channels {
            progress: progress_receiver,
            published: published_receiver,
        };
        (ctx, channels)
    }

    /// The host, as the run read it before any plugin ran.
    #[must_use]
    pub fn host(&self) -> &HostProfile {
        &self.host
    }

    /// The path of a provisioned tool; `None` for a tool the plugin did not declare, since the
    /// orchestrator runs a plugin only when every declared tool is ready.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&Path> {
        self.tools.get(name).map(PathBuf::as_path)
    }

    /// A directory of the plugin's own, removed after the run.
    #[must_use]
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Fires on a timeout and on an interrupt; a plugin stops soon after.
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// When the orchestrator stops waiting for the plugin.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// One line of progress, in Chinese, for the log on stderr. A line that arrives after the
    /// orchestrator stopped listening is dropped.
    pub fn progress(&self, text: impl Into<String>) {
        let _ = self.progress.send(text.into());
    }

    /// Replaces the last partial result. The report shows the last one published when the
    /// plugin skips, fails, times out or is interrupted.
    pub fn publish(&self, output: &O) {
        self.published.send_replace(Some(output.clone()));
    }
}
