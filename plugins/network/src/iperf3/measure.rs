//! The measurement as YABS v2026-10-31 runs iperf3: over IPv4, then over IPv6 when the host has
//! it, an upload to each server, a second's pause, then a download from it. A run that fails is
//! tried again on another port of the server, 2 seconds later, up to three times. The result is
//! published after each server, so that a timeout or an interrupt keeps what was measured.

use std::borrow::Cow;
use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Step;
use bench_api::Stop;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::job::User;

use super::parse;
use super::parse::Attempt;
use super::result::Failure;
use super::result::Output;
use super::result::Settings;
use super::result::Speed;
use super::result::Tested;
use super::result::Way;
use super::servers::Chosen;
use super::servers::Server;

/// The tries that one direction gets, each on a port not tried yet, as YABS's.
pub const TRIES: u32 = 3;

/// The wait before another try, as YABS's.
pub const WAIT: Duration = Duration::from_secs(2);

/// The pause between a server's upload and its download, as YABS's.
pub const PAUSE: Duration = Duration::from_secs(1);

/// How long iperf3 may take to connect, in milliseconds.
pub const CONNECT_TIMEOUT: &str = "5000";

/// What a run may take beyond its seconds: connecting, and the exchange of the results.
pub const MARGIN: Duration = Duration::from_secs(10);

/// A family of the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// IPv4.
    Ipv4,
    /// IPv6.
    Ipv6,
}

/// The arguments of one run: an upload to `server` at `port` over `family`, or a download when
/// `download` is set, as `settings` say, with its result in JSON.
#[must_use]
pub fn args(
    server: &Server,
    family: Family,
    port: u16,
    download: bool,
    settings: Settings,
) -> Vec<OsString> {
    let family = match family {
        Family::Ipv4 => "-4",
        Family::Ipv6 => "-6",
    };
    let mut args: Vec<String> = vec![
        family.to_owned(),
        "-c".to_owned(),
        server.host.clone(),
        "-p".to_owned(),
        port.to_string(),
        "-P".to_owned(),
        settings.streams.to_string(),
        "-t".to_owned(),
        settings.seconds.to_string(),
        "-J".to_owned(),
        "--connect-timeout".to_owned(),
        CONNECT_TIMEOUT.to_owned(),
    ];
    if let Some(rate) = settings.rate {
        args.push("-b".to_owned());
        args.push(format!("{rate}M"));
    }
    if download {
        args.push("-R".to_owned());
    }
    args.into_iter().map(OsString::from).collect()
}

/// One run of `program`: [`args`], as the unprivileged user from `/`, since the servers are
/// third parties, and never quoted in the log, since iperf3's JSON names the host's own address.
#[must_use]
pub fn job(
    program: &Path,
    server: &Server,
    family: Family,
    port: u16,
    download: bool,
    settings: Settings,
) -> Job {
    let limit = Duration::from_secs(u64::from(settings.seconds)) + MARGIN;
    let mut job = Job::new(
        program,
        args(server, family, port, download, settings),
        limit,
    );
    job.dir = Some("/".into());
    job.quotable = false;
    job.user = Some(User::NOBODY);
    job
}

/// Tests each of the `chosen` servers over each family of `families`, IPv4's and IPv6's, that the
/// host reaches and the server answers over, as `settings` say. `pick` chooses the next port
/// among those not tried yet, by its index; `run` runs a job: the shared runner, or a recording in
/// tests.
pub async fn measure<F, Fut, P>(
    ctx: &Context<Output>,
    program: &Path,
    chosen: Chosen,
    families: (bool, bool),
    settings: Settings,
    mut pick: P,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
    P: FnMut(usize) -> usize,
{
    let Chosen { servers, unknown } = chosen;
    let (ipv4, ipv6) = families;
    let plan: Vec<(Family, Vec<&Server>)> = [(Family::Ipv4, ipv4), (Family::Ipv6, ipv6)]
        .into_iter()
        .filter(|(_, reached)| *reached)
        .map(|(family, _)| {
            let answer = servers
                .iter()
                .filter(|server| match family {
                    Family::Ipv4 => server.ipv4,
                    Family::Ipv6 => server.ipv6,
                })
                .collect();
            (family, answer)
        })
        .filter(|(_, servers): &(Family, Vec<&Server>)| !servers.is_empty())
        .collect();
    if plan.is_empty() {
        return Err(Stop::Skipped(Failure::NoFamily));
    }
    let mut output = Output {
        version: None,
        settings,
        unknown,
        ipv4: Vec::new(),
        ipv6: Vec::new(),
    };
    ctx.publish(&output);
    let cancel = ctx.cancel_token();
    let tries = Tries {
        program,
        settings,
        cancel: &cancel,
        run: &run,
    };
    for (family, servers) in &plan {
        for server in servers {
            let ipv6 = *family == Family::Ipv6;
            ctx.progress(Step::Upload {
                location: Cow::Owned(server.location.clone()),
                country: Cow::Owned(server.country.clone()),
                ipv6,
            });
            let upload = tries
                .direction(server, *family, false, &mut output.version, &mut pick)
                .await?;
            let download = if upload == Way::Unreachable {
                Way::Skipped
            } else {
                sleep_cancellable(PAUSE, &cancel).await?;
                ctx.progress(Step::Download {
                    location: Cow::Owned(server.location.clone()),
                    country: Cow::Owned(server.country.clone()),
                    ipv6,
                });
                tries
                    .direction(server, *family, true, &mut output.version, &mut pick)
                    .await?
            };
            let tested = Tested {
                location: server.location.clone(),
                country: server.country.clone(),
                provider: server.provider.clone(),
                upload,
                download,
            };
            match family {
                Family::Ipv4 => output.ipv4.push(tested),
                Family::Ipv6 => output.ipv6.push(tested),
            }
            ctx.publish(&output);
        }
    }
    let measured = output.ipv4.iter().chain(&output.ipv6).any(|tested| {
        matches!(tested.upload, Way::Measured(_)) || matches!(tested.download, Way::Measured(_))
    });
    if !measured {
        return Err(Stop::Failed(Failure::NoResult));
    }
    Ok(output)
}

/// What every try needs.
struct Tries<'a, F> {
    program: &'a Path,
    settings: Settings,
    cancel: &'a CancellationToken,
    run: &'a F,
}

async fn sleep_cancellable(
    duration: Duration,
    cancel: &CancellationToken,
) -> Result<(), Stop<Failure>> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(Stop::Failed(Failure::Cancelled)),
        () = tokio::time::sleep(duration) => Ok(()),
    }
}

impl<F, Fut> Tries<'_, F>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    /// The upload to `server` over `family`, or the download when `download` is set: up to
    /// [`TRIES`] tries, each on a port not tried yet, [`WAIT`] apart, until one measures a
    /// speed. Without a speed, the last try's reason, unless the server could not be reached
    /// on any try. Notes the version that iperf3 reported, when there is none yet.
    async fn direction(
        &self,
        server: &Server,
        family: Family,
        download: bool,
        version: &mut Option<String>,
        pick: &mut impl FnMut(usize) -> usize,
    ) -> Result<Way, Stop<Failure>> {
        let mut ports = server.port_list();
        let mut reached = None;
        for tries in 1..=TRIES {
            if ports.is_empty() {
                break;
            }
            if tries > 1 {
                sleep_cancellable(WAIT, self.cancel).await?;
            }
            let index = pick(ports.len()).min(ports.len() - 1);
            let port = ports.remove(index);
            let job = job(self.program, server, family, port, download, self.settings);
            let way = match (self.run)(job).await {
                Ok(finished) => {
                    if version.is_none() {
                        *version = parse::version(&finished);
                    }
                    match parse::attempt(&finished, download) {
                        Attempt::Measured {
                            bits,
                            rtt,
                            retransmits,
                        } => {
                            return Ok(Way::Measured(Speed {
                                bits,
                                tries,
                                rtt,
                                retransmits,
                            }));
                        }
                        Attempt::Busy => Way::Busy,
                        Attempt::Unreachable => Way::Unreachable,
                        Attempt::Error(error) => Way::Error(error),
                        Attempt::Nothing => Way::Nothing,
                        Attempt::Fault(fault) => Way::Fault(fault),
                    }
                }
                Err(RunError::Fault(fault)) => Way::Fault(fault),
                Err(RunError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            };
            if way != Way::Unreachable {
                reached = Some(way);
            }
        }
        Ok(reached.unwrap_or(Way::Unreachable))
    }
}
