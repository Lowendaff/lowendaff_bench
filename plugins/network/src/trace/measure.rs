//! The measurement, as the original project traced: over IPv4, then over IPv6 when the host has
//! it, the cache servers and then the list's targets, one after another, each with a classic
//! ICMP trace of 3 probes a hop and 30 hops at most. The result is published after each target,
//! so that a timeout or an interrupt keeps what was traced.

use std::borrow::Cow;
use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::IpFamily;
use bench_api::Step;
use bench_api::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::job::User;

use super::exchanges::Exchanges;
use super::parse;
use super::result::ExchangeList;
use super::result::Failure;
use super::result::Output;
use super::result::Route;
use super::result::Settings;
use super::result::Target;
use super::result::TraceError;
use super::result::Traced;
use super::result::Unavailable;

/// How long a trace may take: 30 hops of 3 probes, which NextTrace sends a hop at a time and
/// waits a second for, with the lookups of each hop's network and place.
pub const LIMIT: Duration = Duration::from_secs(60);

/// The arguments of one trace of `host` over `family`: a classic traceroute (`--traceroute`) with
/// ICMP, 3 probes a hop (`-q 3`), 30 hops at most (`-m 30`), a second for each probe (`--timeout
/// 1000`), its result in JSON (`-j`), without colours (`-C`), with English names (`-g en`) and
/// without the upload of its map (`-M`). Without `geo`, NextTrace looks up no hop's network and
/// place (`-d disable-geoip`).
#[must_use]
pub fn args(host: &str, family: IpFamily, geo: bool) -> Vec<OsString> {
    let family = match family {
        IpFamily::Ipv4 => "-4",
        IpFamily::Ipv6 => "-6",
    };
    let mut args = vec![
        "--traceroute",
        "-j",
        "-M",
        "-C",
        "-g",
        "en",
        "-q",
        "3",
        "-m",
        "30",
        "--timeout",
        "1000",
    ];
    if !geo {
        args.extend(["-d", "disable-geoip"]);
    }
    args.push(family);
    args.push(host);
    args.into_iter().map(OsString::from).collect()
}

/// One trace by `program`: [`args`], as the unprivileged user from `/`, since the targets are third
/// parties, and never quoted in the log, since NextTrace's JSON names the host's own address.
#[must_use]
pub fn job(program: &Path, host: &str, family: IpFamily, geo: bool) -> Job {
    let mut job = Job::new(program, args(host, family, geo), LIMIT);
    job.dir = Some("/".into());
    job.quotable = false;
    job.user = Some(User::NOBODY);
    job
}

/// A target to trace over one family: its name, or why the cache server's lookup gave none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    /// Which target.
    pub target: Target,
    /// The DNS name or address to trace, or why there is none.
    pub host: Result<String, Unavailable>,
}

/// What to trace, over each family, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The targets to trace over IPv4.
    pub ipv4: Vec<Planned>,
    /// The targets to trace over IPv6.
    pub ipv6: Vec<Planned>,
}

/// What the result names besides the routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// What the configuration chose.
    pub settings: Settings,
    /// The ids asked for that no target has.
    pub unknown: Vec<String>,
    /// How the list of exchange ports served.
    pub exchanges: ExchangeList,
}

/// Traces each target of `plan` with `program`, IPv4's first, and names each hop at an address of
/// `exchanges` with its exchange. `run` runs a job: the shared runner, or a recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    program: &Path,
    plan: Plan,
    about: About,
    exchanges: &Exchanges,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    if plan.ipv4.is_empty() && plan.ipv6.is_empty() {
        return Err(Stop::Skipped(Failure::NoFamily));
    }
    let geo = about.settings.geo;
    let mut output = Output {
        settings: about.settings,
        unknown: about.unknown,
        exchanges: about.exchanges,
        ipv4: Vec::new(),
        ipv6: Vec::new(),
    };
    ctx.publish(&output);
    for (family, planned) in [(IpFamily::Ipv4, plan.ipv4), (IpFamily::Ipv6, plan.ipv6)] {
        for Planned { target, host } in planned {
            let route = match &host {
                Err(unavailable) => Route::Unavailable(unavailable.clone()),
                Ok(host) => {
                    ctx.progress(Step::Trace {
                        target: name_of(&target),
                        ipv6: family == IpFamily::Ipv6,
                    });
                    match run(job(program, host, family, geo)).await {
                        Ok(finished) => route_of(&finished, exchanges),
                        Err(RunError::Fault(fault)) => Route::Failed(TraceError::Fault(fault)),
                        Err(RunError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
                    }
                }
            };
            let traced = Traced {
                target,
                host: host.ok(),
                route,
            };
            match family {
                IpFamily::Ipv4 => output.ipv4.push(traced),
                IpFamily::Ipv6 => output.ipv6.push(traced),
            }
            ctx.publish(&output);
        }
    }
    let traced = output
        .ipv4
        .iter()
        .chain(&output.ipv6)
        .any(|traced| matches!(traced.route, Route::Traced(_)));
    if !traced {
        return Err(Stop::Failed(Failure::NoRoute));
    }
    Ok(output)
}

/// What a finished run of NextTrace gave: its trace, with the exchanges named, or its error.
fn route_of(finished: &job::Output, exchanges: &Exchanges) -> Route {
    if let Some(mut trace) = parse::trace(&finished.text) {
        for hop in &mut trace.hops {
            for answer in &mut hop.answers {
                answer.exchange = exchanges.get(&answer.address).cloned();
            }
        }
        return Route::Traced(trace);
    }
    let error = match (finished.code, parse::error(&finished.text)) {
        (Some(0), _) => TraceError::Nothing,
        (_, Some(error)) => TraceError::Error(error),
        // Its output names the host's own address, so no line of it is kept.
        (Some(code), None) => TraceError::Fault(job::Fault::Exit { code, last: None }),
        (None, None) => TraceError::Fault(job::Fault::Signal { last: None }),
    };
    Route::Failed(error)
}

/// A target's name for the progress log.
fn name_of(target: &Target) -> Cow<'static, str> {
    match target {
        Target::YouTube => Cow::Borrowed("YouTube"),
        Target::Netflix(_) => Cow::Borrowed("Netflix"),
        Target::Listed { name, .. } => Cow::Owned(name.clone()),
    }
}
