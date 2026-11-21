//! China carrier speed (`network.taier`): pure-Rust multi-stream HTTP/1.1 download and upload
//! throughput measurements against CAICT GlobalSpeed (`com.cnspeedtest.globalspeed`) carrier nodes
//! across mainland China.

pub mod measure;
pub mod nodes;
pub mod parse;
pub mod result;

use std::collections::hash_map::RandomState;
use std::io;
use std::num::NonZeroU8;
use std::num::NonZeroU16;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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

use nodes::ListError;

pub use result::Failure;
pub use result::NodeMeasured;
pub use result::NodeOutcome;
pub use result::Operator;
pub use result::OperatorChoice;
pub use result::Output;
pub use result::Settings;
pub use result::TestedNode;

/// Where the program downloads the normalized ASCII Taier node list from by default.
pub const SERVERS_URL: &str =
    "https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/taier.json";

/// File handle name requested from the orchestrator.
pub const SERVERS: &str = "taier-servers";

/// China carrier speed (`network.taier`), as configured.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Taier {
    /// Cities, provinces, location IDs, or `all`; empty tests the default locations (`Beijing`,
    /// `Shanghai`, `Guangzhou`).
    pub cities: Vec<String>,
    /// Carriers to test (`telecom`, `unicom`, `mobile`, `cernet`, `cbn`, `other`, or `all`);
    /// empty tests `telecom`, `unicom`, and `mobile`.
    pub operators: Vec<OperatorChoice>,
    /// Optional local path or HTTPS URL overriding [`SERVERS_URL`].
    pub list: Option<String>,
    /// Seconds per transfer direction (`1..=60`; `4` by default).
    pub seconds: NonZeroU16,
    /// Parallel TCP streams per direction (`1..=32`; `8` by default).
    pub streams: NonZeroU8,
}

impl Default for Taier {
    fn default() -> Self {
        Self {
            cities: Vec::new(),
            operators: Vec::new(),
            list: None,
            seconds: NonZeroU16::new(Settings::DEFAULT_SECONDS).expect("default seconds > 0"),
            streams: NonZeroU8::new(Settings::DEFAULT_STREAMS).expect("default streams > 0"),
        }
    }
}

impl Plugin for Taier {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.taier",
        requires: Requirements {
            tools: &[],
            net: Net::Ipv4,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(30 * 60),
    };

    fn files(&self) -> Vec<FileSpec> {
        let source = match &self.list {
            None => FileSource::Url(SERVERS_URL.to_owned()),
            Some(list) if list.contains("://") => FileSource::Url(list.clone()),
            Some(list) => FileSource::Path(PathBuf::from(list)),
        };
        vec![FileSpec::new(SERVERS, source)]
    }

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let path = ctx
            .file(SERVERS)
            .expect("the orchestrator runs a plugin only when it has its files");
        let raw = read(path).await?;
        let locations = nodes::parse(&raw).map_err(|error| match error {
            ListError::Newer(_) => Stop::Skipped(Failure::List(error)),
            _ => Stop::Failed(Failure::List(error)),
        })?;
        let chosen = nodes::choose(&locations, &self.cities, &self.operators);
        if chosen.slots.is_empty() {
            return Err(Stop::Skipped(Failure::NoNode(chosen.unknown)));
        }
        let settings = Settings {
            seconds: self.seconds.get().min(60),
            streams: self.streams.get().min(32),
            cities: self.cities.clone(),
            operators: chosen.operators.clone(),
            custom_list: self.list.is_some(),
        };
        measure::measure(ctx, chosen, settings, random_seed(), unix_secs).await
    }
}

async fn read(path: &Path) -> Result<Vec<u8>, Stop<Failure>> {
    use tokio::io::AsyncReadExt;

    let unreadable = |err: io::Error| Stop::Failed(Failure::Unreadable(err.to_string()));
    let file = File::open(path).await.map_err(unreadable)?;
    let mut buf = Vec::new();
    file.take(nodes::LIMIT as u64 + 1)
        .read_to_end(&mut buf)
        .await
        .map_err(unreadable)?;
    Ok(buf)
}

fn random_seed() -> u64 {
    use std::hash::BuildHasher;

    RandomState::new().hash_one(unix_secs())
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
