//! Network speed: iperf3 measures the TCP throughput each way and the latency between the host and
//! the servers of a list, over IPv4 and IPv6, as YABS does. The list is a file of this repository,
//! which the program downloads for the item, so that it changes without a release.

pub mod measure;
pub mod parse;
pub mod result;
pub mod servers;

use std::collections::hash_map::RandomState;
use std::io;
use std::num::NonZeroU8;
use std::num::NonZeroU16;
use std::num::NonZeroU32;
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
use bench_tool::IPERF3;
use serde::Deserialize;
use tokio::fs::File;

use result::Settings;
use servers::ListError;

pub use result::Failure;
pub use result::Output;

/// Where the program downloads the server list from, unless the configuration names another.
pub const SERVERS_URL: &str =
    "https://raw.githubusercontent.com/Lowendaff/bench_data/main/servers/iperf3.json";

/// The name that the item asks for the server list by.
pub const SERVERS: &str = "servers";

/// Network speed, as its configuration sets it.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Iperf3 {
    /// The countries whose servers to test, by ISO 3166 code in any case, or `all`; none for the
    /// list's defaults. The command line's `--countries` sets it.
    pub countries: Vec<String>,
    /// The servers to test by their ids, which win over the countries.
    pub servers: Vec<String>,
    /// Another list: a path on the host, or an HTTPS address; none for this repository's.
    pub list: Option<String>,
    /// The seconds each way.
    pub seconds: NonZeroU16,
    /// The parallel streams.
    pub streams: NonZeroU8,
    /// The most Mbit/s that a stream sends; none for no limit.
    pub rate: Option<NonZeroU32>,
}

impl Default for Iperf3 {
    /// YABS's way, with this repository's list and its default servers.
    fn default() -> Self {
        let Settings {
            seconds, streams, ..
        } = Settings::YABS;
        Self {
            countries: Vec::new(),
            servers: Vec::new(),
            list: None,
            seconds: NonZeroU16::new(seconds).expect("YABS's seconds are not 0"),
            streams: NonZeroU8::new(streams).expect("YABS's streams are not 0"),
            rate: None,
        }
    }
}

impl Iperf3 {
    /// How each test runs.
    #[must_use]
    pub fn settings(&self) -> Settings {
        Settings {
            seconds: self.seconds.get(),
            streams: self.streams.get(),
            rate: self.rate.map(NonZeroU32::get),
        }
    }
}

impl Plugin for Iperf3 {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.iperf3",
        requires: Requirements {
            tools: &[IPERF3],
            net: Net::Any,
            ..Requirements::NONE
        },
        // The worst case of ten servers over both families, with every try running into its
        // limit; the default five servers take about 5 minutes when they answer.
        timeout: Duration::from_secs(45 * 60),
    };

    fn files(&self) -> Vec<FileSpec> {
        let source = match &self.list {
            None => FileSource::Url(SERVERS_URL.to_owned()),
            // An address names its scheme; the program downloads HTTPS addresses only.
            Some(list) if list.contains("://") => FileSource::Url(list.clone()),
            Some(list) => FileSource::Path(PathBuf::from(list)),
        };
        vec![FileSpec::new(SERVERS, source)]
    }

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(IPERF3.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let path = ctx
            .file(SERVERS)
            .expect("the orchestrator runs a plugin only when it has its files");
        let list = read(path).await?;
        let list = servers::parse(&list).map_err(|error| match error {
            ListError::Newer(_) => Stop::Skipped(Failure::List(error)),
            _ => Stop::Failed(Failure::List(error)),
        })?;
        let chosen = servers::choose(&list, &self.countries, &self.servers);
        if chosen.servers.is_empty() {
            return Err(Stop::Skipped(Failure::NoServer(chosen.unknown)));
        }
        let network = &ctx.host().network;
        let families = (network.ipv4.reachable(), network.ipv6.reachable());
        measure::measure(
            ctx,
            &program,
            chosen,
            families,
            self.settings(),
            random_index,
            |job| async move { job.run(ctx).await.ended },
        )
        .await
    }
}

/// The server list at `path`, up to a byte more than a list may hold, so that a larger one shows.
async fn read(path: &Path) -> Result<Vec<u8>, Stop<Failure>> {
    use tokio::io::AsyncReadExt;

    let unreadable = |err: io::Error| Stop::Failed(Failure::Unreadable(err.to_string()));
    let file = File::open(path).await.map_err(unreadable)?;
    let mut list = Vec::new();
    file.take(servers::LIMIT as u64 + 1)
        .read_to_end(&mut list)
        .await
        .map_err(unreadable)?;
    Ok(list)
}

/// An index below `count`, at random: the ports of a server are tried in a random order, as YABS
/// picks a random port, so that the clients of a server spread over its ports.
fn random_index(count: usize) -> usize {
    use std::hash::BuildHasher;

    // Each `RandomState` hashes with keys of its own, which the system's randomness seeds.
    let random = RandomState::new().hash_one(count);
    usize::try_from(random % count.max(1) as u64).unwrap_or(0)
}
