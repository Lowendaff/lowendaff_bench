//! Plugin contract for lowendaff-bench. Test-item crates depend on this crate only.
//!
//! User-facing strings are Chinese; ids,
//! identifiers and comments are English.

#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error;
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;

pub use tokio_util::sync::CancellationToken;

/// Architectures every pinned tool must cover.
pub const RELEASE_ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// Static facts about a plugin type, checked by [`validate_registry`].
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    /// Stable id: lowercase dotted segments such as `disk.fio`.
    pub id: &'static str,
    /// Section title shown in the terminal.
    pub title: &'static str,
    /// Plugin version; bump it when the measurement method changes.
    pub version: &'static str,
    /// Preconditions the orchestrator checks before calling `run`.
    pub requires: Requirements,
    /// Hard limit for one run.
    pub timeout: Duration,
}

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
}

impl Requirements {
    /// No tools, no network, every platform, any C library.
    pub const NONE: Self = Self {
        tools: &[],
        net: Net::None,
        platforms: &[],
        glibc: None,
    };

    /// Whether the plugin runs on `platform`.
    #[must_use]
    pub fn applies_to(&self, platform: Platform) -> bool {
        self.platforms.is_empty() || self.platforms.contains(&platform.kind())
    }
}

/// Where the program runs, detected once by the orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Platform {
    /// No virtualization was detected; most likely a dedicated server.
    #[default]
    Dedicated,
    /// A virtual machine.
    Vm(Hypervisor),
    /// A container, sharing the kernel of its host.
    Container(ContainerKind),
}

impl Platform {
    /// The kind plugins declare in [`Requirements::platforms`].
    #[must_use]
    pub fn kind(self) -> PlatformKind {
        match self {
            Self::Dedicated => PlatformKind::Dedicated,
            Self::Vm(_) => PlatformKind::Vm,
            Self::Container(_) => PlatformKind::Container,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dedicated => f.write_str("未检测到虚拟化"),
            Self::Vm(Hypervisor::Qemu) => f.write_str("QEMU 虚拟机（纯软件模拟）"),
            Self::Vm(Hypervisor::Other) => f.write_str("虚拟机（类型未知）"),
            Self::Vm(hypervisor) => write!(f, "{hypervisor} 虚拟机"),
            Self::Container(ContainerKind::Wsl) => f.write_str("WSL"),
            Self::Container(ContainerKind::Other) => f.write_str("容器（类型未知）"),
            Self::Container(container) => write!(f, "{container} 容器"),
        }
    }
}

/// Kind of platform, as declared by plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    /// A machine without detected virtualization.
    Dedicated,
    /// Any virtual machine.
    Vm,
    /// Any container.
    Container,
}

impl fmt::Display for PlatformKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dedicated => "独立服务器",
            Self::Vm => "虚拟机",
            Self::Container => "容器",
        })
    }
}

/// Hypervisor of a virtual machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hypervisor {
    /// QEMU with KVM acceleration.
    Kvm,
    /// QEMU without KVM, emulating the CPU in software.
    Qemu,
    /// Xen.
    Xen,
    /// VMware.
    Vmware,
    /// Microsoft Hyper-V.
    HyperV,
    /// Oracle VirtualBox.
    VirtualBox,
    /// Parallels.
    Parallels,
    /// Amazon EC2 (Nitro or Xen based).
    AmazonEc2,
    /// Google Compute Engine.
    GoogleCompute,
    /// FreeBSD bhyve.
    Bhyve,
    /// A hypervisor that was detected but not recognized.
    Other,
}

impl fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Kvm => "KVM",
            Self::Qemu => "QEMU",
            Self::Xen => "Xen",
            Self::Vmware => "VMware",
            Self::HyperV => "Hyper-V",
            Self::VirtualBox => "VirtualBox",
            Self::Parallels => "Parallels",
            Self::AmazonEc2 => "Amazon EC2",
            Self::GoogleCompute => "Google Compute Engine",
            Self::Bhyve => "bhyve",
            Self::Other => "未知",
        })
    }
}

/// Kind of container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// LXC, including LXD and Incus.
    Lxc,
    /// OpenVZ or Virtuozzo.
    Openvz,
    /// Docker.
    Docker,
    /// Podman.
    Podman,
    /// systemd-nspawn.
    SystemdNspawn,
    /// Windows Subsystem for Linux.
    Wsl,
    /// A container that was detected but not recognized.
    Other,
}

impl fmt::Display for ContainerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Lxc => "LXC",
            Self::Openvz => "OpenVZ",
            Self::Docker => "Docker",
            Self::Podman => "Podman",
            Self::SystemdNspawn => "systemd-nspawn",
            Self::Wsl => "WSL",
            Self::Other => "未知",
        })
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

/// A glibc version, such as 2.35. Versions compare by major, then minor number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlibcVersion {
    /// Major number, 2 for every glibc since 1997.
    pub major: u32,
    /// Minor number.
    pub minor: u32,
}

impl fmt::Display for GlibcVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// The host's C library, as far as programs built for glibc are concerned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Libc {
    /// glibc of this version.
    Glibc(GlibcVersion),
    /// glibc whose version could not be read. Plugins that need glibc still run.
    #[default]
    GlibcUnknown,
    /// No glibc loader for this architecture, as on musl systems such as Alpine.
    NoGlibc,
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

/// Host facts probed once by the orchestrator and shared read-only with every plugin.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// Value of `std::env::consts::ARCH`.
    pub arch: String,
    /// Whether an IPv4 handshake with a public address succeeded.
    pub ipv4: bool,
    /// Whether an IPv6 handshake with a public address succeeded.
    pub ipv6: bool,
    /// Tools that were downloaded and verified, by name.
    pub tools: BTreeMap<String, PathBuf>,
    /// The platform the program runs on.
    pub platform: Platform,
    /// The host's C library.
    pub libc: Libc,
}

/// A test item. The implementing struct is also its configuration: `Default` gives the
/// defaults and serde parses the user's table for this plugin id.
pub trait Plugin: DeserializeOwned + Default + Send + Sync + 'static {
    /// Static facts about this plugin.
    const DESCRIPTOR: Descriptor;

    /// Runs the test and streams results through `ctx.emit`.
    ///
    /// Must not block the async executor: CPU-bound work belongs in `spawn_blocking` and
    /// must poll `ctx.cancel_token()`; child processes are spawned with `kill_on_drop(true)`.
    fn run(&self, ctx: &Context) -> impl Future<Output = Result<(), PluginError>> + Send;
}

/// Per-run handle given to a plugin.
pub struct Context {
    // Shared with the orchestrator and every plugin task. Tasks are spawned (so they need
    // `'static` data) and a detached task may outlive the run, so this cannot be a borrow.
    env: Arc<Env>,
    records: UnboundedSender<Record>,
    cancel: CancellationToken,
    deadline: Instant,
    work_dir: PathBuf,
}

impl Context {
    /// Builds a context and the receiver for the records it emits. Used by the
    /// orchestrator, and by plugin unit tests to drive `run` directly.
    #[must_use]
    pub fn new(
        env: Arc<Env>,
        deadline: Instant,
        work_dir: PathBuf,
        cancel: CancellationToken,
    ) -> (Self, UnboundedReceiver<Record>) {
        let (records, receiver) = mpsc::unbounded_channel();
        let ctx = Self {
            env,
            records,
            cancel,
            deadline,
            work_dir,
        };
        (ctx, receiver)
    }

    /// Host facts shared by all plugins.
    #[must_use]
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Streams one result. Records survive a later timeout, interrupt, failure or panic.
    pub fn emit(&self, record: Record) {
        // The receiver only goes away after the orchestrator gave up on this run.
        let _ = self.records.send(record);
    }

    /// Fires on timeout (before the grace period) and on interrupt.
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// When the orchestrator stops waiting for this run.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Private directory on the run's disk (not `/tmp`), removed after the run.
    #[must_use]
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Path of a declared tool; the orchestrator only runs a plugin whose tools verified.
    pub fn tool(&self, name: &str) -> Result<&Path, PluginError> {
        self.env
            .tools
            .get(name)
            .map(PathBuf::as_path)
            .ok_or_else(|| PluginError::fail(format!("工具 {name} 未声明或不可用")))
    }
}

/// Why a run did not complete. Any `std::error::Error` converts into `Fail` with `?`.
#[derive(Debug)]
pub enum PluginError {
    /// A precondition discovered at run time; reported as skipped.
    Skip(String),
    /// The test could not produce its result; reported as failed.
    Fail(String),
}

impl PluginError {
    /// A skip with a user-facing reason.
    #[must_use]
    pub fn skip(reason: impl Into<String>) -> Self {
        Self::Skip(reason.into())
    }

    /// A failure with a user-facing reason.
    #[must_use]
    pub fn fail(reason: impl Into<String>) -> Self {
        Self::Fail(reason.into())
    }
}

/// Lets plugins use `?` on any std error; the cause chain is kept in the message.
impl<E: error::Error + Send + Sync + 'static> From<E> for PluginError {
    fn from(err: E) -> Self {
        use std::fmt::Write;

        let mut message = err.to_string();
        let mut source = err.source();
        while let Some(cause) = source {
            let _ = write!(message, ": {cause}");
            source = cause.source();
        }
        Self::Fail(message)
    }
}

/// Result primitive. Renderers handle every kind, so a new kind is a compile error in
/// each renderer rather than a silently dropped record.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// Text fact, e.g. CPU model or virtualization type.
    Fact {
        /// Stable id within the plugin.
        id: String,
        /// Chinese label.
        label: String,
        /// Text as shown.
        value: String,
    },
    /// Number with unit, or the reason it could not be measured.
    Metric {
        /// Stable id within the plugin.
        id: String,
        /// Chinese label.
        label: String,
        /// The value or why it is missing.
        result: Measurement,
    },
    /// Rectangular table of display cells.
    Table(Table),
}

impl Record {
    /// A text fact.
    #[must_use]
    pub fn fact(id: &str, label: &str, value: impl Into<String>) -> Self {
        Self::Fact {
            id: id.into(),
            label: label.into(),
            value: value.into(),
        }
    }

    /// A measured or missing number.
    #[must_use]
    pub fn metric(id: &str, label: &str, result: Measurement) -> Self {
        Self::Metric {
            id: id.into(),
            label: label.into(),
            result,
        }
    }

    /// The record's stable id.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Fact { id, .. } | Self::Metric { id, .. } => id,
            Self::Table(table) => &table.id,
        }
    }
}

/// A table whose rows all have as many cells as there are headers.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    id: String,
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// Builds a table, rejecting rows whose width differs from the header.
    pub fn try_new(
        id: &str,
        title: &str,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    ) -> Result<Self, RecordError> {
        if let Some(index) = rows.iter().position(|row| row.len() != headers.len()) {
            let cells = rows[index].len();
            let expected = headers.len();
            return Err(RecordError(format!(
                "表格 {id} 第 {index} 行有 {cells} 列，应为 {expected} 列"
            )));
        }
        Ok(Self {
            id: id.into(),
            title: title.into(),
            headers,
            rows,
        })
    }

    /// Chinese title shown above the table.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Column headers.
    #[must_use]
    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    /// Rows of display cells.
    #[must_use]
    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }
}

/// Why a metric has no value. Shown instead of a fake number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// The service or resource is not available.
    Unavailable,
    /// The remote side was busy.
    Busy,
    /// The measurement did not finish in time.
    Timeout,
    /// Not supported on this host.
    Unsupported,
    /// The measurement failed.
    Error,
    /// The cause is unknown.
    Unknown,
}

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "不可用",
            Self::Busy => "繁忙",
            Self::Timeout => "超时",
            Self::Unsupported => "不支持",
            Self::Error => "出错",
            Self::Unknown => "未知",
        })
    }
}

/// A finite value with a unit, or the reason it is missing; nothing in between.
///
/// Displays as `171.01 MB/s`, `940 Mbit/s`, or the reason (`繁忙`).
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement(Reading);

#[derive(Debug, Clone, PartialEq)]
enum Reading {
    Measured { value: f64, unit: String },
    Missing(Missing),
}

impl Measurement {
    /// A measured value; rejects non-finite values and empty units.
    pub fn try_new(value: f64, unit: &str) -> Result<Self, RecordError> {
        if !value.is_finite() {
            return Err(RecordError(format!("测量值必须是有限数，实际为 {value}")));
        }
        if unit.is_empty() {
            return Err(RecordError("测量值缺少单位".into()));
        }
        Ok(Self(Reading::Measured {
            value,
            unit: unit.to_owned(),
        }))
    }

    /// A value that could not be measured, with the reason.
    #[must_use]
    pub fn new_missing(reason: Missing) -> Self {
        Self(Reading::Missing(reason))
    }
}

impl fmt::Display for Measurement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Reading::Measured { value, unit } if value.fract() == 0.0 => {
                write!(f, "{value:.0} {unit}")
            }
            Reading::Measured { value, unit } => write!(f, "{value:.2} {unit}"),
            Reading::Missing(reason) => reason.fmt(f),
        }
    }
}

/// A record that violates its invariant; the message is user-facing.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordError(pub String);

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl error::Error for RecordError {}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe view of a configured plugin. Implemented for every `Plugin`; never by hand.
pub trait DynPlugin: Send + Sync {
    /// Static facts about the plugin.
    fn descriptor(&self) -> Descriptor;

    /// [`Plugin::run`] with the future boxed, so it can be called through `dyn`.
    fn run_boxed<'a>(&'a self, ctx: &'a Context) -> BoxFuture<'a, Result<(), PluginError>>;
}

impl<P: Plugin> DynPlugin for P {
    fn descriptor(&self) -> Descriptor {
        P::DESCRIPTOR
    }

    fn run_boxed<'a>(&'a self, ctx: &'a Context) -> BoxFuture<'a, Result<(), PluginError>> {
        Box::pin(self.run(ctx))
    }
}

// `Arc` because the orchestrator keeps the plugin while a spawned (`'static`) task runs
// it, and a detached task may outlive the run.
type BuildFn = fn(Option<&serde_json::Value>) -> Result<Arc<dyn DynPlugin>, String>;

/// One entry of the compile-time registry.
#[derive(Debug, Clone, Copy)]
pub struct Registration {
    /// Static facts about the plugin.
    pub descriptor: Descriptor,
    build: BuildFn,
}

impl Registration {
    /// Builds the plugin from its config table; `None` uses the plugin's `Default`.
    pub fn build(&self, config: Option<&serde_json::Value>) -> Result<Arc<dyn DynPlugin>, String> {
        (self.build)(config)
    }
}

/// The registry entry for plugin `P`.
#[must_use]
pub const fn register<P: Plugin>() -> Registration {
    Registration {
        descriptor: P::DESCRIPTOR,
        build: build_plugin::<P>,
    }
}

fn build_plugin<P: Plugin>(
    config: Option<&serde_json::Value>,
) -> Result<Arc<dyn DynPlugin>, String> {
    let plugin = match config {
        None => P::default(),
        Some(value) => P::deserialize(value).map_err(|err| err.to_string())?,
    };
    Ok(Arc::new(plugin))
}

/// Contract checks every registry must pass; the binary crate runs them as one test.
/// Messages are for developers, so they are English.
#[must_use]
pub fn validate_registry(registry: &[Registration]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut tools: BTreeMap<&str, ToolSpec> = BTreeMap::new();
    let mut by_title: BTreeMap<&str, Vec<(&str, &[PlatformKind])>> = BTreeMap::new();
    for registration in registry {
        let descriptor = registration.descriptor;
        let id = descriptor.id;
        if !valid_id(id) {
            problems.push(format!(
                "{id}: id must be lowercase dotted segments of [a-z0-9-]"
            ));
        }
        if !ids.insert(id) {
            problems.push(format!("{id}: duplicate id"));
        }
        if descriptor.title.is_empty() || descriptor.version.is_empty() {
            problems.push(format!("{id}: title and version must be set"));
        }
        if descriptor.timeout.is_zero() {
            problems.push(format!("{id}: timeout must be positive"));
        }
        // Variants of one test item share its title; exactly one of them may run anywhere.
        let platforms = descriptor.requires.platforms;
        let variants = by_title.entry(descriptor.title).or_default();
        for (other, other_platforms) in variants.iter() {
            if *other == id {
                // A duplicate registration; the duplicate-id check reports it.
                continue;
            }
            let overlap = platforms.is_empty()
                || other_platforms.is_empty()
                || platforms.iter().any(|kind| other_platforms.contains(kind));
            if overlap {
                problems.push(format!(
                    "{id}: shares its title with {other}, so their platforms must not overlap"
                ));
            }
        }
        variants.push((id, platforms));
        if let Err(err) = registration.build(Some(&serde_json::json!({}))) {
            problems.push(format!(
                "{id}: empty config table must build (missing #[serde(default)]?): {err}"
            ));
        }
        if registration
            .build(Some(&serde_json::json!({ "__unknown_field__": true })))
            .is_ok()
        {
            problems.push(format!(
                "{id}: config accepts unknown fields (missing #[serde(deny_unknown_fields)]?)"
            ));
        }
        for tool in descriptor.requires.tools {
            check_tool(id, tool, &mut problems);
            match tools.get(tool.name) {
                Some(seen) if seen != tool => {
                    problems.push(format!(
                        "{id}: tool {} differs from another plugin's declaration",
                        tool.name
                    ));
                }
                Some(_) => {}
                None => {
                    tools.insert(tool.name, *tool);
                }
            }
        }
    }
    problems
}

fn check_tool(owner: &str, tool: &ToolSpec, problems: &mut Vec<String>) {
    let name = tool.name;
    if name.is_empty() || tool.version.is_empty() {
        problems.push(format!("{owner}: tool name and version must be set"));
    }
    for arch in RELEASE_ARCHES {
        if tool.asset_for(arch).is_none() {
            problems.push(format!("{owner}: tool {name} has no {arch} build"));
        }
    }
    for asset in tool.assets {
        if !asset.url.starts_with("https://") {
            problems.push(format!(
                "{owner}: tool {name} must be downloaded over https"
            ));
        }
        let is_hex = asset
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if asset.sha256.len() != 64 || !is_hex {
            problems.push(format!(
                "{owner}: tool {name} sha256 must be 64 lowercase hex digits"
            ));
        }
        if asset.size == 0 {
            problems.push(format!("{owner}: tool {name} size must be positive"));
        }
    }
    if let ToolForm::TarGz { executable } = tool.form {
        let plain = !executable.is_empty()
            && executable != "."
            && executable != ".."
            && !executable.contains('/');
        if !plain {
            problems.push(format!(
                "{owner}: tool {name} must name its executable in the archive by file name"
            ));
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('.').all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('-')
                && !segment.ends_with('-')
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
