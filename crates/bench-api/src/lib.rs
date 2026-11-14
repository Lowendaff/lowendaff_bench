//! Plugin contract for lowendaff-bench, which every test-item crate implements. A plugin
//! declares what it needs, and returns a result of its own type, or its own reason to skip or to
//! fail; it publishes the result as it goes and reports its progress, and the orchestrator and
//! the reporter do the rest.

#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_host::glibc::GlibcVersion;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tokio::time::Instant;

pub use tokio_util::sync::CancellationToken;

/// Architectures every pinned tool must cover.
pub const RELEASE_ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// Declarative preconditions. Root is not listed: the whole program requires root.
#[derive(Debug, Clone, Copy)]
pub struct Requirements {
    /// Pinned tools, downloaded and verified by the orchestrator before the first test.
    pub tools: &'static [ToolSpec],
    /// Network access the test needs.
    pub net: Net,
    /// Kinds of platform the test applies to; empty means every platform. On any other
    /// platform the plugin does not run.
    pub platforms: &'static [PlatformKind],
    /// The oldest glibc that the test's tools run on, or `None` when they do not need glibc.
    /// The orchestrator checks it before downloading the tools.
    pub glibc: Option<GlibcVersion>,
    /// Whether the test needs at least two NUMA nodes that it can use, as a test between them
    /// does. On any other host the plugin does not run.
    pub numa: bool,
}

impl Requirements {
    /// No tools, no network, every platform, any C library, any NUMA layout.
    pub const NONE: Self = Self {
        tools: &[],
        net: Net::None,
        platforms: &[],
        glibc: None,
        numa: false,
    };

    /// Whether the plugin runs on `host`: on one of its platforms, and with two NUMA nodes or
    /// more when it needs them. A layout that could not be read has none.
    #[must_use]
    pub fn applies_to(&self, host: &HostProfile) -> bool {
        let platform = self.platforms.is_empty() || self.platforms.contains(&host.platform.kind());
        let numa = !self.numa || matches!(&host.numa, Read::Done(layout) if layout.usable() >= 2);
        platform && numa
    }
}

/// Network access a plugin needs; an unreachable one skips the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    /// Works offline.
    None,
    /// Either IPv4 or IPv6 must be reachable.
    Any,
    /// IPv4 must be reachable.
    Ipv4,
    /// IPv6 must be reachable.
    Ipv6,
}

/// A pinned external tool. Plugins that share a tool must declare identical specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Tool name, e.g. `fio`: the executable's file name, or the directory of an archive.
    pub name: &'static str,
    /// Upstream version the builds were made from.
    pub version: &'static str,
    /// One static build per architecture in [`RELEASE_ARCHES`].
    pub assets: &'static [ToolAsset],
    /// What each asset is.
    pub form: ToolForm,
}

impl ToolSpec {
    /// The build for `arch` (a value of `std::env::consts::ARCH`), if there is one.
    #[must_use]
    pub fn asset_for(&self, arch: &str) -> Option<&ToolAsset> {
        self.assets.iter().find(|asset| asset.arch == arch)
    }
}

/// What the download of a tool is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolForm {
    /// The download is the static executable itself.
    Executable,
    /// A gzip-compressed tar archive with one top-level directory. It is unpacked into a
    /// directory named after the tool, without that top-level directory.
    TarGz {
        /// File name of the executable in the archive's top-level directory.
        executable: &'static str,
    },
}

/// One downloadable build of a [`ToolSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAsset {
    /// Value of `std::env::consts::ARCH`.
    pub arch: &'static str,
    /// HTTPS download URL.
    pub url: &'static str,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: &'static str,
    /// Exact size in bytes; a download that grows past it is aborted.
    pub size: u64,
}

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

/// What a plugin reports while it runs, for the progress log on stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// A line of the plugin's own, in Chinese.
    Line(String),
    /// A tool it ran, which the log shows with `-v`.
    Tool(ToolRun),
}

/// A tool that a plugin ran: its command, how it ended, and what it printed last when it did
/// not exit with 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRun {
    /// The program's file name, then its arguments.
    pub command: Vec<String>,
    /// How the run ended.
    pub end: ToolEnd,
    /// Its last lines with text, oldest first, when it did not exit with 0 and its output may be
    /// quoted.
    pub tail: Vec<String>,
}

/// How a tool's run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolEnd {
    /// It exited with this code.
    Exit(i32),
    /// A signal ended it.
    Signal,
    /// It ran past this limit and was stopped.
    TimedOut(Duration),
    /// The plugin was cancelled, and the tool stopped.
    Cancelled,
    /// It could not start, for the system's reason.
    Spawn(String),
    /// What it printed could not be read, for the system's reason.
    Read(String),
}

/// What every result tells besides its own data.
pub trait Measurement: Serialize + Clone + Send + Sync + 'static {
    /// The tools used, with the versions they reported.
    fn tools(&self) -> Vec<ToolVersion>;
}

/// A test item, whose result has a type of its own.
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
    progress: UnboundedSender<Progress>,
    published: watch::Sender<Option<O>>,
}

/// The orchestrator's end of a [`Context`].
pub struct Channels<O> {
    /// The plugin's progress lines and tool runs, in order.
    pub progress: UnboundedReceiver<Progress>,
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
        let _ = self.progress.send(Progress::Line(text.into()));
    }

    /// A tool that the plugin ran, for the log with `-v`; dropped like a progress line when
    /// nobody listens.
    pub fn ran(&self, run: ToolRun) {
        let _ = self.progress.send(Progress::Tool(run));
    }

    /// Replaces the last partial result. The report shows the last one published when the
    /// plugin skips, fails, times out or is interrupted.
    pub fn publish(&self, output: &O) {
        self.published.send_replace(Some(output.clone()));
    }
}
