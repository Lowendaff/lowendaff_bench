//! Route tracing: NextTrace traces the route from the host to the targets of a list and to the
//! nearest YouTube and Netflix cache servers, over IPv4 and IPv6: every hop with the addresses
//! that answered, their networks and places, and their latency, as the original project's route
//! tracing showed them. The lists are files of this repository, which the program downloads for
//! the item, so that they change without a release.

pub mod exchanges;
pub mod lists;
pub mod lookup;
pub mod measure;
pub mod parse;
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
use bench_api::IpFamily;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_tool::NEXTTRACE;
use serde::Deserialize;
use tokio::fs::File;

use exchanges::Exchanges;
use lists::ListError;
use measure::About;
use measure::Plan;
use measure::Planned;
use result::ExchangeList;
use result::Place;
use result::Settings;
use result::Target;
use result::Unavailable;
use targets::Chosen;

pub use result::Failure;
pub use result::Output;
pub use targets::Choice;

/// Where the program downloads the target list from, unless the configuration names another.
pub const TARGETS_URL: &str =
    "https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/traceroute.json";

/// Where the program downloads the list of Netflix's exchange ports from, unless the
/// configuration names another.
pub const EXCHANGES_URL: &str =
    "https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/netflix-ix.json";

/// Google's page that names the YouTube cache server that serves the host, in one line.
pub const YOUTUBE_URL: &str = "https://redirector.googlevideo.com/report_mapping?di=no";

/// fast.com's API, with the token that fast.com's own page uses, which names the Netflix cache
/// servers that would serve the host.
pub const NETFLIX_URL: &str = concat!(
    "https://api.fast.com/netflix/speedtest/v2?https=true&",
    "token=YXNkZmFzZGxmbnNkYWZoYXNkZmhrYWxm&urlCount=5"
);

/// The name that the item asks for the target list by.
pub const TARGETS: &str = "targets";

/// The name that the item asks for the list of exchange ports by.
pub const EXCHANGES: &str = "exchanges";

/// The names that the item asks for Google's answer by, over IPv4 and over IPv6.
pub const YOUTUBE: [&str; 2] = ["youtube-ipv4", "youtube-ipv6"];

/// The names that the item asks for fast.com's answer by, over IPv4 and over IPv6.
pub const NETFLIX: [&str; 2] = ["netflix-ipv4", "netflix-ipv6"];

/// The families in the order that the item traces them, with the index of each one's answers in
/// [`YOUTUBE`] and [`NETFLIX`].
const FAMILIES: [(IpFamily, usize); 2] = [(IpFamily::Ipv4, 0), (IpFamily::Ipv6, 1)];

/// Route tracing, as its configuration sets it.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Trace {
    /// The groups whose targets to trace: `services`, `china`, `international` or `all`; none
    /// for both cache servers and the list's defaults.
    pub groups: Vec<Choice>,
    /// The targets to trace by their ids, `youtube` and `netflix` among them, which win over the
    /// groups.
    pub targets: Vec<String>,
    /// Another target list: a path on the host, or an HTTPS address; none for this repository's.
    pub list: Option<String>,
    /// Another list of exchange ports: a path or an HTTPS address; none for this repository's.
    pub exchanges: Option<String>,
    /// Another page in place of Google's `report_mapping`: a path or an HTTPS address.
    pub youtube_api: Option<String>,
    /// Another address in place of fast.com's API, should its token stop working: a path or an
    /// HTTPS address.
    pub netflix_api: Option<String>,
    /// Whether NextTrace looks up each hop's network and place with its service.
    pub geo: bool,
}

impl Default for Trace {
    /// Both cache servers and the default targets of this repository's list, with each hop's
    /// network and place.
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            targets: Vec::new(),
            list: None,
            exchanges: None,
            youtube_api: None,
            netflix_api: None,
            geo: true,
        }
    }
}

impl Plugin for Trace {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "network.trace",
        requires: Requirements {
            tools: &[NEXTTRACE],
            net: Net::Any,
            ..Requirements::NONE
        },
        // The 73 traces over IPv4 take 6 to 18 minutes when they answer, and a host with IPv6
        // adds up to 50 more; past the limit, the traces done are kept.
        timeout: Duration::from_secs(60 * 60),
    };

    fn files(&self) -> Vec<FileSpec> {
        let optional = |name, source, family| FileSpec {
            family,
            optional: true,
            ..FileSpec::new(name, source)
        };
        let mut files = vec![
            FileSpec::new(TARGETS, source(self.list.as_deref(), TARGETS_URL)),
            optional(
                EXCHANGES,
                source(self.exchanges.as_deref(), EXCHANGES_URL),
                None,
            ),
        ];
        let (youtube, netflix) = targets::services(&self.groups, &self.targets);
        let lookups = [
            (youtube, YOUTUBE, self.youtube_api.as_deref(), YOUTUBE_URL),
            (netflix, NETFLIX, self.netflix_api.as_deref(), NETFLIX_URL),
        ];
        for (chosen, names, given, default) in lookups {
            if !chosen {
                continue;
            }
            for (family, index) in FAMILIES {
                files.push(optional(names[index], source(given, default), Some(family)));
            }
        }
        files
    }

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(NEXTTRACE.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let path = ctx
            .file(TARGETS)
            .expect("the orchestrator runs a plugin only when it has its files");
        let list = read(path)
            .await
            .map_err(|err| Stop::Failed(Failure::Unreadable(err)))?;
        let list = targets::parse(&list).map_err(|error| match error {
            ListError::Newer(_) => Stop::Skipped(Failure::List(error)),
            _ => Stop::Failed(Failure::List(error)),
        })?;
        let chosen = targets::choose(&list, &self.groups, &self.targets);
        if !chosen.youtube && !chosen.netflix && chosen.targets.is_empty() {
            return Err(Stop::Skipped(Failure::NoTarget(chosen.unknown)));
        }
        let (exchanges, served) = exchange_ports(ctx).await;
        let network = &ctx.host().network;
        let reached = [network.ipv4.reachable(), network.ipv6.reachable()];
        let mut plan = Plan {
            ipv4: Vec::new(),
            ipv6: Vec::new(),
        };
        for (family, index) in FAMILIES {
            if !reached[index] {
                continue;
            }
            let planned = planned(ctx, &chosen, family, index).await;
            match family {
                IpFamily::Ipv4 => plan.ipv4 = planned,
                IpFamily::Ipv6 => plan.ipv6 = planned,
            }
        }
        let about = About {
            settings: Settings {
                groups: self.groups.clone(),
                targets: self.targets.clone(),
                list: self.list.is_some(),
                geo: self.geo,
            },
            unknown: chosen.unknown,
            exchanges: served,
        };
        measure::measure(ctx, &program, plan, about, &exchanges, |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}

/// A file's source: the one given, an HTTPS address when it names its scheme, else a path on the
/// host; else the `default` address.
fn source(given: Option<&str>, default: &str) -> FileSource {
    match given {
        None => FileSource::Url(default.to_owned()),
        // An address names its scheme; the program downloads HTTPS addresses only.
        Some(given) if given.contains("://") => FileSource::Url(given.to_owned()),
        Some(given) => FileSource::Path(PathBuf::from(given)),
    }
}

/// The chosen targets over `family`, whose answers are at `index`: the cache servers, by the
/// answers that the program fetched over that family, then the list's targets that have a name of
/// the family.
async fn planned(
    ctx: &Context<Output>,
    chosen: &Chosen,
    family: IpFamily,
    index: usize,
) -> Vec<Planned> {
    let mut planned = Vec::new();
    if chosen.youtube {
        let host = answer(ctx, YOUTUBE[index])
            .await
            .and_then(|answer| lookup::youtube(&answer).map_err(Unavailable::Answer));
        planned.push(Planned {
            target: Target::YouTube,
            host,
        });
    }
    if chosen.netflix {
        let cache = answer(ctx, NETFLIX[index])
            .await
            .and_then(|answer| lookup::netflix(&answer).map_err(Unavailable::Answer));
        let (target, host) = match cache {
            Ok(cache) => (
                Target::Netflix(Some(Place {
                    city: cache.city,
                    country: cache.country,
                })),
                Ok(cache.host),
            ),
            Err(unavailable) => (Target::Netflix(None), Err(unavailable)),
        };
        planned.push(Planned { target, host });
    }
    for target in &chosen.targets {
        let host = match family {
            IpFamily::Ipv4 => Some(&target.ipv4),
            IpFamily::Ipv6 => target.ipv6.as_ref(),
        };
        if let Some(host) = host {
            planned.push(Planned {
                target: Target::Listed {
                    id: target.id.clone(),
                    group: target.group,
                    name: target.name.clone(),
                },
                host: Ok(host.clone()),
            });
        }
    }
    planned
}

/// The answer that the program fetched as the file `name`, or why there is none.
async fn answer(ctx: &Context<Output>, name: &str) -> Result<Vec<u8>, Unavailable> {
    match ctx.file(name) {
        Some(path) => read(path).await.map_err(Unavailable::Unreadable),
        None => Err(Unavailable::Unfetched(
            ctx.missing(name).unwrap_or_default().to_owned(),
        )),
    }
}

/// The list of exchange ports, and how it served: empty when the program could not get it, or it
/// is not a list that the item can use, as it only names exchanges.
async fn exchange_ports(ctx: &Context<Output>) -> (Exchanges, ExchangeList) {
    let Some(path) = ctx.file(EXCHANGES) else {
        let why = ctx.missing(EXCHANGES).unwrap_or_default().to_owned();
        return (Exchanges::new(), ExchangeList::Missing(why));
    };
    let bytes = match read(path).await {
        Ok(bytes) => bytes,
        Err(err) => return (Exchanges::new(), ExchangeList::Unreadable(err)),
    };
    match exchanges::parse(&bytes) {
        Ok(exchanges) => {
            let ports = exchanges.len();
            (exchanges, ExchangeList::Read(ports))
        }
        Err(error) => (Exchanges::new(), ExchangeList::Unusable(error)),
    }
}

/// The file at `path`, up to a byte more than a list may hold, so that a larger one shows.
async fn read(path: &Path) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;

    let unreadable = |err: io::Error| err.to_string();
    let file = File::open(path).await.map_err(unreadable)?;
    let mut bytes = Vec::new();
    file.take(lists::LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(unreadable)?;
    Ok(bytes)
}
