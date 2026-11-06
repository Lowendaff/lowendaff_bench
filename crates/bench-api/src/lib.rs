//! Plugin contract for lowendaff-bench. Test-item crates depend on this crate only.
//!
//! Design: `docs/design/plugin-system.md`. User-facing strings are Chinese; ids,
//! identifiers and comments are English.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::time::Instant;

pub use tokio_util::sync::CancellationToken;

/// Architectures every pinned tool must cover (design P-02).
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
    pub net: Net,
}

impl Requirements {
    pub const NONE: Self = Self {
        tools: &[],
        net: Net::None,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    None,
    Any,
    Ipv4,
    Ipv6,
}

/// A pinned external tool. Plugins that share a tool must declare identical specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Executable name, e.g. `fio`.
    pub name: &'static str,
    pub version: &'static str,
    /// One static build per architecture in [`RELEASE_ARCHES`].
    pub assets: &'static [ToolAsset],
}

impl ToolSpec {
    pub fn asset_for(&self, arch: &str) -> Option<&ToolAsset> {
        self.assets.iter().find(|asset| asset.arch == arch)
    }
}

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
    pub arch: String,
    pub ipv4: bool,
    pub ipv6: bool,
    /// Tools that were downloaded and verified, by name.
    pub tools: BTreeMap<String, PathBuf>,
}

/// A test item. The implementing struct is also its configuration: `Default` gives the
/// defaults and serde parses the user's table for this plugin id.
pub trait Plugin: DeserializeOwned + Default + Send + Sync + 'static {
    const DESCRIPTOR: Descriptor;

    /// Runs the test and streams results through `ctx.emit`.
    ///
    /// Must not block the async executor: CPU-bound work belongs in `spawn_blocking` and
    /// must poll `ctx.cancel_token()`; child processes are spawned with `kill_on_drop(true)`.
    fn run(&self, ctx: &Context) -> impl Future<Output = Result<(), PluginError>> + Send;
}

/// Per-run handle given to a plugin.
pub struct Context {
    env: Arc<Env>,
    records: mpsc::UnboundedSender<Record>,
    cancel: CancellationToken,
    deadline: Instant,
    work_dir: PathBuf,
}

impl Context {
    /// Used by the orchestrator, and by plugin unit tests to drive `run` directly.
    pub fn new(
        env: Arc<Env>,
        deadline: Instant,
        work_dir: PathBuf,
        cancel: CancellationToken,
    ) -> (Self, mpsc::UnboundedReceiver<Record>) {
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

    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Records survive a later timeout, interrupt, failure or panic.
    pub fn emit(&self, record: Record) {
        // The receiver only goes away after the orchestrator gave up on this run.
        let _ = self.records.send(record);
    }

    /// Fires on timeout (before the grace period) and on interrupt.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Private directory on the run's disk (not `/tmp`), removed after the run.
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

#[derive(Debug)]
pub enum PluginError {
    /// A precondition discovered at run time; reported as skipped.
    Skip(String),
    /// The test could not produce its result; reported as failed.
    Fail(String),
}

impl PluginError {
    pub fn skip(reason: impl Into<String>) -> Self {
        Self::Skip(reason.into())
    }

    pub fn fail(reason: impl Into<String>) -> Self {
        Self::Fail(reason.into())
    }
}

/// Lets plugins use `?` on any std error; the cause chain is kept in the message.
impl<E: std::error::Error + Send + Sync + 'static> From<E> for PluginError {
    fn from(err: E) -> Self {
        let mut message = err.to_string();
        let mut source = err.source();
        while let Some(cause) = source {
            message.push_str(": ");
            message.push_str(&cause.to_string());
            source = cause.source();
        }
        Self::Fail(message)
    }
}

/// Result primitive. Renderers understand only these kinds, so a new test item never
/// needs renderer changes. New kinds arrive with the first plugin that needs them.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Record {
    /// Text fact, e.g. CPU model or virtualization type.
    Fact {
        id: String,
        label: String,
        value: String,
    },
    /// Number with unit, or the reason it could not be measured.
    Metric {
        id: String,
        label: String,
        result: Measurement,
    },
    /// Rectangular table of display cells.
    Table(Table),
}

impl Record {
    pub fn fact(id: &str, label: &str, value: impl Into<String>) -> Self {
        Self::Fact {
            id: id.into(),
            label: label.into(),
            value: value.into(),
        }
    }

    pub fn metric(id: &str, label: &str, result: Measurement) -> Self {
        Self::Metric {
            id: id.into(),
            label: label.into(),
            result,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Fact { id, .. } | Self::Metric { id, .. } => id,
            Self::Table(table) => &table.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    id: String,
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(
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

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }
}

/// Why a metric has no value. Shown instead of a fake number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    Unavailable,
    Busy,
    Timeout,
    Unsupported,
    Error,
    Unknown,
}

impl Missing {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "不可用",
            Self::Busy => "繁忙",
            Self::Timeout => "超时",
            Self::Unsupported => "不支持",
            Self::Error => "出错",
            Self::Unknown => "未知",
        }
    }
}

/// A finite value with a unit, or the reason it is missing; nothing in between.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement(Result<(f64, String), Missing>);

impl Measurement {
    pub fn measured(value: f64, unit: &str) -> Result<Self, RecordError> {
        if !value.is_finite() {
            return Err(RecordError(format!("测量值必须是有限数，实际为 {value}")));
        }
        if unit.is_empty() {
            return Err(RecordError("测量值缺少单位".into()));
        }
        Ok(Self(Ok((value, unit.to_owned()))))
    }

    pub fn missing(status: Missing) -> Self {
        Self(Err(status))
    }

    /// `Ok((value, unit))` when measured, otherwise why it is missing.
    pub fn value(&self) -> Result<(f64, &str), Missing> {
        match &self.0 {
            Ok((value, unit)) => Ok((*value, unit)),
            Err(missing) => Err(*missing),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordError(pub String);

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RecordError {}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe view of a configured plugin. Implemented for every `Plugin`; never by hand.
pub trait DynPlugin: Send + Sync {
    fn descriptor(&self) -> Descriptor;
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

type BuildFn = fn(Option<&serde_json::Value>) -> Result<Arc<dyn DynPlugin>, String>;

/// One entry of the compile-time registry.
#[derive(Debug, Clone, Copy)]
pub struct Registration {
    pub descriptor: Descriptor,
    build: BuildFn,
}

impl Registration {
    /// `None` means no config table was given: the plugin's `Default` is used.
    pub fn build(&self, config: Option<&serde_json::Value>) -> Result<Arc<dyn DynPlugin>, String> {
        (self.build)(config)
    }
}

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
pub fn validate_registry(registry: &[Registration]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut tools: BTreeMap<&str, ToolSpec> = BTreeMap::new();
    for reg in registry {
        let d = reg.descriptor;
        if !valid_id(d.id) {
            problems.push(format!(
                "{}: id must be lowercase dotted segments of [a-z0-9-]",
                d.id
            ));
        }
        if !ids.insert(d.id) {
            problems.push(format!("{}: duplicate id", d.id));
        }
        if d.title.is_empty() || d.version.is_empty() {
            problems.push(format!("{}: title and version must be set", d.id));
        }
        if d.timeout.is_zero() {
            problems.push(format!("{}: timeout must be positive", d.id));
        }
        if let Err(err) = reg.build(Some(&serde_json::json!({}))) {
            problems.push(format!(
                "{}: empty config table must build (missing #[serde(default)]?): {err}",
                d.id
            ));
        }
        if reg
            .build(Some(&serde_json::json!({ "__unknown_field__": true })))
            .is_ok()
        {
            problems.push(format!(
                "{}: config accepts unknown fields (missing #[serde(deny_unknown_fields)]?)",
                d.id
            ));
        }
        for tool in d.requires.tools {
            check_tool(d.id, tool, &mut problems);
            match tools.get(tool.name) {
                Some(seen) if seen != tool => {
                    problems.push(format!(
                        "{}: tool {} differs from another plugin's declaration",
                        d.id, tool.name
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
    if tool.name.is_empty() || tool.version.is_empty() {
        problems.push(format!("{owner}: tool name and version must be set"));
    }
    for arch in RELEASE_ARCHES {
        if tool.asset_for(arch).is_none() {
            problems.push(format!("{owner}: tool {} has no {arch} build", tool.name));
        }
    }
    for asset in tool.assets {
        if !asset.url.starts_with("https://") {
            problems.push(format!(
                "{owner}: tool {} must be downloaded over https",
                tool.name
            ));
        }
        let is_hex = asset
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if asset.sha256.len() != 64 || !is_hex {
            problems.push(format!(
                "{owner}: tool {} sha256 must be 64 lowercase hex digits",
                tool.name
            ));
        }
        if asset.size == 0 {
            problems.push(format!("{owner}: tool {} size must be positive", tool.name));
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
