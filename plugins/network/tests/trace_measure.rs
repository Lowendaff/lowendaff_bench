//! The measurement with recorded runs instead of NextTrace: the families and targets in order,
//! each run's command, the routes that real output gives with their exchanges named, every way a
//! trace can fail, what it publishes and logs, and a cancellation.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::IpFamily;
use bench_api::Measurement;
use bench_api::Progress;
use bench_api::Step;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use bench_tool::NEXTTRACE;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::job::User;
use plugin_network::trace::Failure;
use plugin_network::trace::Output;
use plugin_network::trace::exchanges::Exchanges;
use plugin_network::trace::lookup::LookupError;
use plugin_network::trace::measure;
use plugin_network::trace::measure::About;
use plugin_network::trace::measure::Plan;
use plugin_network::trace::measure::Planned;
use plugin_network::trace::result::ExchangeList;
use plugin_network::trace::result::Route;
use plugin_network::trace::result::Settings;
use plugin_network::trace::result::Target;
use plugin_network::trace::result::TraceError;
use plugin_network::trace::result::Unavailable;
use plugin_network::trace::targets::Group;
use tokio::time::Instant;

const NEXTTRACE_PATH: &str = "/tools/nexttrace";

/// What a measurement came to.
struct Measured {
    result: Result<Output, Stop<Failure>>,
    published: Option<Output>,
    steps: Vec<Step>,
    jobs: Vec<Job>,
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/trace/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

fn finished(code: Option<i32>, text: &str) -> Result<job::Output, RunError> {
    Ok(job::Output {
        code,
        text: text.to_owned(),
    })
}

fn listed(id: &str, name: &str) -> Target {
    Target::Listed {
        id: id.to_owned(),
        group: Group::China,
        name: name.to_owned(),
    }
}

fn planned(target: Target, host: &str) -> Planned {
    Planned {
        target,
        host: Ok(host.to_owned()),
    }
}

fn about(geo: bool) -> About {
    About {
        settings: Settings {
            groups: Vec::new(),
            targets: Vec::new(),
            list: false,
            geo,
        },
        unknown: vec!["xyz".to_owned()],
        exchanges: ExchangeList::Read(1),
    }
}

/// Measures `plan`, the runs returning `script` in order, with a cancellation when `cancel_at`
/// runs have started.
async fn measure_with(
    plan: Plan,
    about: About,
    exchanges: &Exchanges,
    script: Vec<Result<job::Output, RunError>>,
    cancel_at: Option<usize>,
) -> Measured {
    let cancel = CancellationToken::new();
    let (ctx, mut channels) = Context::<Output>::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(3600),
        "/run/work".into(),
        cancel.clone(),
    );
    let script = Mutex::new(VecDeque::from(script));
    let jobs = Mutex::new(Vec::new());
    let result = measure::measure(
        &ctx,
        Path::new(NEXTTRACE_PATH),
        plan,
        about,
        exchanges,
        |job| {
            let count = {
                let mut jobs = jobs.lock().expect("no test panics while holding the jobs");
                jobs.push(job);
                jobs.len()
            };
            if cancel_at == Some(count) {
                cancel.cancel();
            }
            let next = script
                .lock()
                .expect("no test panics while holding the script")
                .pop_front()
                .expect("the script has a run for each job");
            async move { next }
        },
    )
    .await;
    let mut steps = Vec::new();
    while let Ok(progress) = channels.progress.try_recv() {
        if let Progress::Step(step) = progress {
            steps.push(step);
        }
    }
    let published = channels.published.borrow().clone();
    Measured {
        result,
        published,
        steps,
        jobs: jobs
            .into_inner()
            .expect("no test panicked while holding the jobs"),
    }
}

fn args(job: &Job) -> Vec<String> {
    job.args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// IPv4: YouTube's cache server, Netflix's that could not be looked up, and Beijing CN2; IPv6:
/// Beijing CN2.
fn plan() -> Plan {
    let beijing = || listed("pek-4809", "Beijing Telecom CN2");
    Plan {
        ipv4: vec![
            planned(Target::YouTube, "r1---sn-npoldn7s.googlevideo.com"),
            Planned {
                target: Target::Netflix(None),
                host: Err(Unavailable::Answer(LookupError::NoTarget)),
            },
            planned(beijing(), "ipv4.pek-4809.endpoint.nxtrace.org"),
        ],
        ipv6: vec![planned(beijing(), "ipv6.pek-4809.endpoint.nxtrace.org")],
    }
}

#[tokio::test]
async fn each_target_is_traced_in_order_over_ipv4_then_ipv6_with_the_items_flags() {
    let reached = || finished(Some(0), &fixture("reached.json"));
    let measured = measure_with(
        plan(),
        about(true),
        &Exchanges::new(),
        vec![reached(), reached(), reached()],
        None,
    )
    .await;
    let flags = [
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
    let with = |family: &str, host: &str| {
        let mut args = strings(&flags);
        args.extend(strings(&[family, host]));
        args
    };
    let ran: Vec<Vec<String>> = measured.jobs.iter().map(args).collect();
    assert_eq!(
        ran,
        [
            with("-4", "r1---sn-npoldn7s.googlevideo.com"),
            with("-4", "ipv4.pek-4809.endpoint.nxtrace.org"),
            with("-6", "ipv6.pek-4809.endpoint.nxtrace.org"),
        ]
    );
    for job in &measured.jobs {
        assert_eq!(job.program, Path::new(NEXTTRACE_PATH));
        assert_eq!(job.dir.as_deref(), Some(Path::new("/")));
        assert_eq!(job.user, Some(User::NOBODY));
        assert!(!job.quotable, "its JSON names the host's own address");
        assert_eq!(job.limit, Duration::from_secs(60));
    }
    assert_eq!(
        measured.steps,
        [
            Step::Trace {
                target: "YouTube",
                ipv6: false
            },
            Step::Trace {
                target: "Beijing Telecom CN2",
                ipv6: false
            },
            Step::Trace {
                target: "Beijing Telecom CN2",
                ipv6: true
            },
        ],
        "no step for the cache server that could not be looked up"
    );
    let output = measured.result.expect("traced");
    assert_eq!(output.unknown, ["xyz"]);
    assert_eq!(output.exchanges, ExchangeList::Read(1));
    assert_eq!(output.ipv4.len(), 3);
    assert_eq!(output.ipv6.len(), 1);
    assert_eq!(
        output.ipv4[1].route,
        Route::Unavailable(Unavailable::Answer(LookupError::NoTarget))
    );
    assert_eq!(output.ipv4[1].host, None);
    assert_eq!(
        output.ipv4[2].host.as_deref(),
        Some("ipv4.pek-4809.endpoint.nxtrace.org")
    );
    let Route::Traced(trace) = &output.ipv6[0].route else {
        panic!("a trace over IPv6");
    };
    assert!(trace.reached);
    assert_eq!(trace.hops.len(), 13);
    assert_eq!(
        measured.published,
        Some(output.clone()),
        "published at the end"
    );
    assert_eq!(output.tools()[0].name, NEXTTRACE.name);
    assert_eq!(output.tools()[0].version, "1.7.6");
}

#[tokio::test]
async fn without_geo_nexttrace_looks_up_no_hop() {
    let measured = measure_with(
        Plan {
            ipv4: vec![planned(listed("lo", "Loopback"), "127.0.0.1")],
            ipv6: Vec::new(),
        },
        about(false),
        &Exchanges::new(),
        vec![finished(Some(0), &fixture("loopback.json"))],
        None,
    )
    .await;
    let ran = args(&measured.jobs[0]);
    let tail = &ran[ran.len() - 4..];
    assert_eq!(tail, ["-d", "disable-geoip", "-4", "127.0.0.1"]);
    assert!(!measured.result.expect("traced").settings.geo);
}

#[tokio::test]
async fn a_hop_at_one_of_netflixs_exchange_ports_is_named_with_its_exchange() {
    let exchanges = Exchanges::from([(
        "59.43.19.93".parse().expect("an address"),
        "Example IX".to_owned(),
    )]);
    let measured = measure_with(
        Plan {
            ipv4: vec![planned(Target::YouTube, "r1---sn-npoldn7s.googlevideo.com")],
            ipv6: Vec::new(),
        },
        about(true),
        &exchanges,
        vec![finished(Some(0), &fixture("reached.json"))],
        None,
    )
    .await;
    let output = measured.result.expect("traced");
    let Route::Traced(trace) = &output.ipv4[0].route else {
        panic!("a trace");
    };
    let named: Vec<(u8, Option<&str>)> = trace
        .hops
        .iter()
        .zip(1..)
        .flat_map(|(hop, number)| {
            hop.answers
                .iter()
                .map(move |answer| (number, answer.exchange.as_deref()))
        })
        .filter(|(_, exchange)| exchange.is_some())
        .collect();
    assert_eq!(named, [(11, Some("Example IX"))]);
}

#[tokio::test]
async fn each_way_a_trace_fails_is_kept_and_no_route_at_all_fails_the_item() {
    let targets: Vec<Planned> = (1..=6)
        .map(|index| planned(listed(&format!("t{index}"), "T"), "t.example"))
        .collect();
    let measured = measure_with(
        Plan {
            ipv4: targets,
            ipv6: Vec::new(),
        },
        about(true),
        &Exchanges::new(),
        vec![
            finished(Some(1), &fixture("unresolved.txt")),
            finished(Some(1), &fixture("no-capability.txt")),
            finished(Some(0), "{\"version\": 1}\n"),
            finished(
                Some(1),
                "{\"Hops\":[[{\"Success\":true,\"Address\":{\"IP\":\"192.0.2.1\"",
            ),
            finished(None, ""),
            Err(RunError::Fault(job::Fault::TimedOut(Duration::from_secs(
                60,
            )))),
        ],
        None,
    )
    .await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoRoute)));
    let routes: Vec<Route> = measured
        .published
        .expect("each target was published")
        .ipv4
        .into_iter()
        .map(|traced| traced.route)
        .collect();
    assert_eq!(
        routes,
        [
            Route::Failed(TraceError::Error(
                "DNS lookup failed: lookup does-not-exist.invalid on 192.0.2.53:53: no such host"
                    .to_owned()
            )),
            Route::Failed(TraceError::Error(
                "(InitICMP) ListenPacket(ip4:icmp, 127.0.0.1) failed: listen ip4:icmp \
                 127.0.0.1: socket: operation not permitted"
                    .to_owned()
            )),
            Route::Failed(TraceError::Nothing),
            // A document cut short is not quoted: it names addresses.
            Route::Failed(TraceError::Fault(job::Fault::Exit {
                code: 1,
                last: None
            })),
            Route::Failed(TraceError::Fault(job::Fault::Signal { last: None })),
            Route::Failed(TraceError::Fault(job::Fault::TimedOut(
                Duration::from_secs(60)
            ))),
        ]
    );
}

#[tokio::test]
async fn an_exit_without_a_line_to_quote_keeps_no_line_of_the_output() {
    let warning: String = fixture("warned.txt")
        .lines()
        .take(3)
        .map(|line| format!("{line}\n"))
        .collect();
    let measured = measure_with(
        Plan {
            ipv4: vec![planned(listed("t", "T"), "t.example")],
            ipv6: Vec::new(),
        },
        about(true),
        &Exchanges::new(),
        vec![finished(Some(1), &warning)],
        None,
    )
    .await;
    let output = measured.published.expect("published");
    assert_eq!(
        output.ipv4[0].route,
        Route::Failed(TraceError::Fault(job::Fault::Exit {
            code: 1,
            last: None
        }))
    );
    assert!(output.tools().is_empty(), "NextTrace traced nothing");
}

#[tokio::test]
async fn a_cancelled_trace_ends_the_item_with_what_was_traced_before() {
    let measured = measure_with(
        plan(),
        about(true),
        &Exchanges::new(),
        vec![
            finished(Some(0), &fixture("reached.json")),
            Err(RunError::Cancelled),
        ],
        Some(2),
    )
    .await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(measured.jobs.len(), 2);
    let published = measured.published.expect("published");
    assert_eq!(
        published.ipv4.len(),
        2,
        "YouTube's trace and Netflix's lookup"
    );
}

#[tokio::test]
async fn an_empty_plan_skips_the_item_as_no_family_serves() {
    let measured = measure_with(
        Plan {
            ipv4: Vec::new(),
            ipv6: Vec::new(),
        },
        about(true),
        &Exchanges::new(),
        Vec::new(),
        None,
    )
    .await;
    assert_eq!(measured.result, Err(Stop::Skipped(Failure::NoFamily)));
    assert!(measured.jobs.is_empty());
}

#[test]
fn a_trace_over_ipv6_asks_for_ipv6() {
    let args: Vec<OsString> = measure::args("a.example", IpFamily::Ipv6, true);
    assert_eq!(args[args.len() - 2], "-6");
    assert_eq!(args[args.len() - 1], "a.example");
}
