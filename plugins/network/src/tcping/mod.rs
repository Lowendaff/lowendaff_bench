//! TCP handshake latency (`network.tcping`): unprivileged dual-stack TCP `SYN -> SYN-ACK` / `RST`
//! round-trip latency, jitter, P95, and packet-loss probing across mainland China's 31 provinces,
//! China premium international backbones (`CN2`, `9929`, `CMIN2`), and 17 global hubs, with 800 ms
//! primary-to-backup endpoint failover.

pub mod measure;
pub mod result;
pub mod targets;

use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::FileSource;
use bench_api::FileSpec;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use serde::Deserialize;
use tokio::fs::File;

use targets::ListError;

pub use result::Failure;
pub use result::Group;
pub use result::GroupChoice;
pub use result::LatencyMeasured;
pub use result::Operator;
pub use result::Output;
pub use result::ProbeCount;
pub use result::Settings;
pub use result::TargetOutcome;
pub use result::TestedTarget;

/// Default URL where `lowendaff-bench` fetches the dual-stack TCP latency target catalog.
pub const TARGETS_URL: &str =
    "https://raw.githubusercontent.com/Lowendaff/bench_data/main/servers/tcping.json";

/// File handle name requested from the orchestrator.
pub const TARGETS: &str = "tcping-targets";

/// Plugin configuration for `network.tcping`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tcping {
    /// Optional target list override (`https://` URL or local file path).
    pub targets: Option<String>,
    /// Selected target groups (`china`, `china-premium`, `global`, or `all`).
    pub groups: Vec<GroupChoice>,
    /// Probe every catalog target in the selected groups (`--tcping-all`), expanding all 31
    /// mainland China provinces and secondary global hubs.
    pub all_targets: bool,
    /// Handshake probes sent per reachable endpoint (`5..=50`, default `10`).
    pub count: ProbeCount,
}

impl Plugin for Tcping {
    type Output = Output;
    type Failure = Failure;

    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.tcping",
        requires: Requirements {
            tools: &[],
            net: Net::Any,
            platforms: &[],
            glibc: None,
            numa: false,
        },
        timeout: Duration::from_secs(600),
    };

    fn files(&self) -> Vec<FileSpec> {
        let source = match self.targets.as_deref() {
            None => FileSource::Url(TARGETS_URL.to_owned()),
            Some(s) if is_url(s) => FileSource::Url(s.to_owned()),
            Some(s) => FileSource::Path(PathBuf::from(s)),
        };
        vec![FileSpec::new(TARGETS, source)]
    }

    async fn run(&self, ctx: &Context<Self::Output>) -> Result<Self::Output, Stop<Self::Failure>> {
        let allow_local = self.targets.as_deref().is_some_and(|s| !is_url(s));
        let Some(path) = ctx.file(TARGETS) else {
            return match ctx.missing(TARGETS) {
                Some(why) => Err(Stop::Failed(Failure::Unreadable(why.to_owned()))),
                None => Err(Stop::Failed(Failure::MissingList)),
            };
        };
        let raw = read_bounded(path).await?;
        let catalog = targets::parse_for_source(&raw, allow_local).map_err(list_stop)?;
        let selected = targets::choose(&catalog, &self.groups, self.all_targets);
        if selected.is_empty() {
            return Err(Stop::Skipped(Failure::NoTargets));
        }
        let settings = Settings {
            groups: self.groups.clone(),
            all_targets: self.all_targets,
            count: self.count.get(),
        };
        measure::run(ctx, &selected, settings, allow_local).await
    }
}

fn is_url(value: &str) -> bool {
    value
        .split_once("://")
        .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.bytes().all(is_scheme_byte))
}

const fn is_scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
}

fn list_stop(error: ListError) -> Stop<Failure> {
    match error {
        ListError::TooLarge => Stop::Failed(Failure::TooLarge),
        ListError::Format(why) => Stop::Failed(Failure::InvalidJson(why)),
        ListError::Version(ver) => Stop::Failed(Failure::UnsupportedVersion(ver)),
        ListError::Count(n) => Stop::Failed(Failure::TargetCount(n)),
        ListError::Entry(entry, rule) => Stop::Failed(Failure::InvalidEntry {
            entry,
            field: rule.as_field(),
        }),
        ListError::Duplicate(id) => Stop::Failed(Failure::DuplicateId(id)),
    }
}

async fn read_bounded(path: &Path) -> Result<Vec<u8>, Stop<Failure>> {
    use tokio::io::AsyncReadExt;

    let file = File::open(path)
        .await
        .map_err(|err| Stop::Failed(Failure::Unreadable(io_reason(&err))))?;
    let cap = u64::try_from(targets::LIMIT).unwrap_or(u64::MAX);
    let mut raw = Vec::new();
    file.take(cap.saturating_add(1))
        .read_to_end(&mut raw)
        .await
        .map_err(|err| Stop::Failed(Failure::Unreadable(io_reason(&err))))?;
    if raw.len() > targets::LIMIT {
        return Err(Stop::Failed(Failure::TooLarge));
    }
    Ok(raw)
}

fn io_reason(error: &io::Error) -> String {
    error.to_string()
}
