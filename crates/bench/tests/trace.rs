//! Route tracing's part of the report, from constructed runs: NextTrace's method, each target's
//! line and the table of its hops, the host's own address masked, and each way the item ends.
//! Compared with snapshots and with the words themselves.

use std::net::IpAddr;
use std::time::Duration;

use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Stop;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use bench_tool::job;
use plugin_network::trace;
use plugin_network::trace::Choice;
use plugin_network::trace::lists::ListError;
use plugin_network::trace::lists::Rule;
use plugin_network::trace::lookup::LookupError;
use plugin_network::trace::result::Answer;
use plugin_network::trace::result::ExchangeList;
use plugin_network::trace::result::Hop;
use plugin_network::trace::result::Place;
use plugin_network::trace::result::Route;
use plugin_network::trace::result::Settings;
use plugin_network::trace::result::Target;
use plugin_network::trace::result::Trace;
use plugin_network::trace::result::TraceError;
use plugin_network::trace::result::Traced;
use plugin_network::trace::result::Unavailable;
use plugin_network::trace::targets::Group;

/// The host's own public address over IPv4, which a hop of a trace may be.
const OWN: &str = "198.51.100.23";

fn run(
    status: Status,
    output: Option<trace::Output>,
    stop: Option<Stop<trace::Failure>>,
) -> Run<Measured> {
    Run {
        id: "network.trace",
        group: "network",
        title: Some("Route tracing"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_secs(721)),
        status,
        result: Some(Measured::NetworkTrace(Report { output, stop })),
    }
}

/// A host whose public IPv4 address is [`OWN`].
fn host() -> HostProfile {
    HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![(
                    "192.0.2.1:443".parse().expect("an address"),
                    Reach::Connected,
                )],
                address: Address::Found {
                    address: OWN.parse().expect("an address"),
                    source: "test",
                },
            },
            ipv6: Family::default(),
        },
        ..HostProfile::default()
    }
}

/// The report of `runs` from its network section on, with the summary.
fn section(runs: &[Run<Measured>], show_ip: bool) -> String {
    let host = host();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip,
        not_applicable: &[],
        runs,
        total: Duration::from_secs(722),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("[Network]").expect("a network section");
    text[start..].to_owned()
}

/// Whether `text` holds `line` as a line of its own.
fn shows(text: &str, line: &str) -> bool {
    text.lines().any(|shown| shown == line)
}

fn answer(
    address: &str,
    asn: Option<&str>,
    network: Option<&str>,
    place: Option<&str>,
    latency: u64,
) -> Answer {
    Answer {
        address: address.parse::<IpAddr>().expect("an address"),
        asn: asn.map(str::to_owned),
        network: network.map(str::to_owned),
        exchange: None,
        place: place.map(str::to_owned),
        latency,
    }
}

fn hop(answers: Vec<Answer>) -> Hop {
    Hop { answers }
}

fn listed(id: &str, group: Group, name: &str) -> Target {
    Target::Listed {
        id: id.to_owned(),
        group,
        name: name.to_owned(),
    }
}

fn traced(target: Target, host: Option<&str>, route: Route) -> Traced {
    Traced {
        target,
        host: host.map(str::to_owned),
        route,
    }
}

fn defaults() -> Settings {
    Settings {
        groups: Vec::new(),
        targets: Vec::new(),
        list: false,
        geo: true,
    }
}

fn output() -> trace::Output {
    let mut exchange = answer(
        "206.223.116.133",
        Some("AS2906"),
        Some("Netflix"),
        Some("US"),
        1_830,
    );
    exchange.exchange = Some("Equinix San Jose".to_owned());
    let youtube = Trace {
        reached: true,
        hops: vec![
            hop(vec![answer("10.0.0.1", None, None, None, 412)]),
            hop(vec![answer(
                OWN,
                Some("AS64496"),
                Some("Example Net"),
                None,
                1_020,
            )]),
            hop(Vec::new()),
            hop(vec![
                answer(
                    "192.0.2.4",
                    Some("AS15169"),
                    Some("Google"),
                    Some("US California"),
                    8_912,
                ),
                answer(
                    "192.0.2.5",
                    Some("AS15169"),
                    Some("Google"),
                    Some("US California"),
                    9_105,
                ),
            ]),
            hop(vec![exchange]),
        ],
    };
    let silent = Trace {
        reached: false,
        hops: vec![hop(Vec::new()); 30],
    };
    let beijing = Trace {
        reached: true,
        hops: vec![
            hop(vec![answer("10.0.0.1", None, None, None, 404)]),
            hop(vec![answer(
                "219.142.211.23",
                Some("AS4847"),
                Some("China Telecom"),
                Some("China Beijing"),
                182_400,
            )]),
        ],
    };
    trace::Output {
        settings: defaults(),
        unknown: vec!["xyz".to_owned()],
        exchanges: ExchangeList::Read(408),
        ipv4: vec![
            traced(
                Target::YouTube,
                Some("r1---sn-npoldn7s.googlevideo.com"),
                Route::Traced(youtube),
            ),
            traced(
                Target::Netflix(None),
                None,
                Route::Unavailable(Unavailable::Unfetched(
                    "download failed: HTTP 404".to_owned(),
                )),
            ),
            traced(
                listed("pek-4809", Group::China, "Beijing Telecom CN2"),
                Some("ipv4.pek-4809.endpoint.nxtrace.org"),
                Route::Traced(beijing.clone()),
            ),
            traced(
                listed(
                    "cogent-fra",
                    Group::International,
                    "Cogent Communications - Frankfurt, DE",
                ),
                Some("t1.174.de.fra.jam114514.me"),
                Route::Traced(silent),
            ),
            traced(
                listed("gcp-sin", Group::International, "GCP - Singapore, SG"),
                Some("35.187.238.97"),
                Route::Failed(TraceError::Error(
                    "DNS lookup failed: lookup x on 192.0.2.53:53: no such host".to_owned(),
                )),
            ),
        ],
        ipv6: vec![
            traced(
                Target::Netflix(Some(Place {
                    city: "Frankfurt".to_owned(),
                    country: "DE".to_owned(),
                })),
                Some("ipv6-c001-fra002-ix.1.oca.nflxvideo.net"),
                Route::Failed(TraceError::Fault(job::Fault::TimedOut(
                    Duration::from_secs(60),
                ))),
            ),
            traced(
                listed("pek-4809", Group::China, "Beijing Telecom CN2"),
                Some("ipv6.pek-4809.endpoint.nxtrace.org"),
                Route::Traced(beijing),
            ),
        ],
    }
}

#[test]
fn each_target_has_its_line_and_the_table_of_its_hops() {
    let text = section(&[run(Status::Completed, Some(output()), None)], false);
    insta::assert_snapshot!(text);
    for line in [
        "  NextTrace 1.7.6: ICMP, 3 probes a hop, at most 30 hops",
        "  YouTube (r1---sn-npoldn7s.googlevideo.com): reached in 5 hops",
        "  Netflix: unavailable: download failed: HTTP 404",
        "  Beijing Telecom CN2 (ipv4.pek-4809.endpoint.nxtrace.org): reached in 2 hops",
        "  Cogent Communications - Frankfurt, DE (t1.174.de.fra.jam114514.me): not reached after \
         30 hops",
        "  GCP - Singapore, SG (35.187.238.97): DNS lookup failed: lookup x on 192.0.2.53:53: no \
         such host",
        "  Netflix, Frankfurt, DE (ipv6-c001-fra002-ix.1.oca.nflxvideo.net): still running after \
         60 s, stopped",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert!(
        !text.contains(OWN),
        "the host's own address is masked: {text}"
    );
    assert!(text.contains("198.51.*.*"), "{text}");
    assert!(
        text.contains("AS2906") && text.contains("Netflix [Equinix San Jose]"),
        "{text}"
    );

    let host = host();
    let runs = [run(Status::Completed, Some(output()), None)];
    let md = report::render(
        &Page {
            version: "0.1.0",
            host: &host,
            show_ip: false,
            not_applicable: &[],
            runs: &runs,
            total: Duration::from_secs(722),
        },
        bench::report::Format::Markdown,
        None,
    );
    assert!(md.contains("\n## Route tracing\n"), "{md}");
    assert!(md.contains("\n### Public services\n"), "{md}");
    assert!(md.contains("\n#### YouTube (IPv4)\n"), "{md}");
    assert!(
        md.contains("| Hop | Address | ASN | Network | Place | Latency |\n"),
        "{md}"
    );
    assert!(
        md.contains("| 3 | * |  |  |  |  |\n"),
        "timeout hop `*` must not be backslash-escaped: {md}"
    );
    assert!(
        md.contains(
            "| 5 | 206.223.116.133 | AS2906 | Netflix [Equinix San Jose] | US | 1.83 ms |\n"
        ),
        "ASN is separated from Network and non-link brackets stay unescaped: {md}"
    );
}

#[test]
fn the_hosts_own_address_shows_in_full_when_asked_for() {
    let text = section(&[run(Status::Completed, Some(output()), None)], true);
    assert!(text.contains(OWN), "{text}");
    assert!(!text.contains("198.51.*.*"), "{text}");
}

#[test]
fn what_the_configuration_chose_shows_in_the_method() {
    let chosen = trace::Output {
        settings: Settings {
            groups: vec![Choice::China, Choice::Services],
            targets: vec!["pek-4809".to_owned(), "youtube".to_owned()],
            list: true,
            geo: false,
        },
        ..output()
    };
    let text = section(&[run(Status::Completed, Some(chosen), None)], false);
    assert!(
        shows(
            &text,
            "  NextTrace 1.7.6: ICMP, 3 probes a hop, at most 30 hops; groups china, services; \
             targets pek-4809, youtube; another target list; without networks and places"
        ),
        "{text}"
    );
}

#[test]
fn a_run_without_exchange_names_says_why_and_a_lookup_without_a_server_says_why() {
    let lookups = [
        (LookupError::NoServer, "Google's answer names no server"),
        (
            LookupError::Id,
            "Google's answer names a server whose id cannot be written as a host name",
        ),
        (LookupError::Format, "fast.com's answer is not its API's"),
        (LookupError::NoTarget, "fast.com's answer names no server"),
        (
            LookupError::Url,
            "fast.com's answer names a server at an address that cannot be traced",
        ),
        (
            LookupError::Place,
            "fast.com's answer names a server's place that cannot be shown",
        ),
    ];
    let mut ipv4: Vec<Traced> = lookups
        .iter()
        .map(|(error, _)| {
            traced(
                Target::YouTube,
                None,
                Route::Unavailable(Unavailable::Answer(*error)),
            )
        })
        .collect();
    ipv4.push(traced(
        Target::YouTube,
        None,
        Route::Unavailable(Unavailable::Unreadable("Permission denied".to_owned())),
    ));
    ipv4.push(traced(
        listed("t", Group::China, "T"),
        Some("t.example"),
        Route::Failed(TraceError::Nothing),
    ));
    let output = |exchanges| trace::Output {
        exchanges,
        unknown: Vec::new(),
        ipv4: ipv4.clone(),
        ipv6: Vec::new(),
        settings: defaults(),
    };
    let text = section(
        &[run(
            Status::Completed,
            Some(output(ExchangeList::Missing(
                "download failed: HTTP 404".to_owned(),
            ))),
            None,
        )],
        false,
    );
    for (_, words) in lookups {
        assert!(
            shows(&text, &format!("  YouTube: unavailable: {words}")),
            "{words:?} in {text}"
        );
    }
    for line in [
        "  YouTube: unavailable: cannot read the answer: Permission denied",
        "  T (t.example): NextTrace traced nothing",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert!(
        text.contains("no exchange names: download failed: HTTP 404"),
        "{text}"
    );
    assert!(
        !text.contains("no target in the list for"),
        "no note without unknown ids: {text}"
    );
    for (exchanges, note) in [
        (
            ExchangeList::Unreadable("Is a directory".to_owned()),
            "no exchange names: cannot read the list of exchange ports: Is a directory",
        ),
        (
            ExchangeList::Unusable(ListError::Entry(2, Rule::Address)),
            "no exchange names: port 2 of the list of exchange ports breaks a rule: its address \
             must be an IP address as it prints, an IPv6 address without an IPv4 part",
        ),
        (
            ExchangeList::Unusable(ListError::TooMany(1001)),
            "no exchange names: the list of exchange ports has 1001 ports, more than 1000",
        ),
        (
            ExchangeList::Unusable(ListError::Duplicate("192.0.2.1".to_owned())),
            "no exchange names: two ports of the list of exchange ports have the address \
             192.0.2.1",
        ),
    ] {
        let text = section(
            &[run(Status::Completed, Some(output(exchanges)), None)],
            false,
        );
        assert!(text.contains(note), "{note:?} in {text}");
    }
    let read = section(
        &[run(
            Status::Completed,
            Some(output(ExchangeList::Read(1))),
            None,
        )],
        false,
    );
    assert!(!read.contains("no exchange names"), "{read}");
}

#[test]
fn every_way_the_item_ends_shows_its_words() {
    let skipped = |failure| run(Status::Skipped(None), None, Some(Stop::Skipped(failure)));
    let failed = |failure| run(Status::Failed(None), None, Some(Stop::Failed(failure)));
    let list = |error| failed(trace::Failure::List(error));
    let rule = |position, rule| list(ListError::Entry(position, rule));
    let runs = [
        failed(trace::Failure::Unreadable(
            "No such file or directory (os error 2)".to_owned(),
        )),
        list(ListError::TooLarge),
        list(ListError::Format(
            "expected value at line 1 column 1".to_owned(),
        )),
        skipped(trace::Failure::List(ListError::Newer(2))),
        list(ListError::Version(0)),
        list(ListError::TooMany(201)),
        rule(3, Rule::Id),
        rule(1, Rule::Group),
        rule(2, Rule::Name),
        rule(4, Rule::Asn),
        rule(5, Rule::Ipv4),
        rule(6, Rule::Ipv6),
        list(ListError::Duplicate("pek-4809".to_owned())),
        skipped(trace::Failure::NoTarget(Vec::new())),
        skipped(trace::Failure::NoTarget(vec![
            "xyz".to_owned(),
            "abc".to_owned(),
        ])),
        skipped(trace::Failure::NoFamily),
        failed(trace::Failure::NoRoute),
        failed(trace::Failure::Cancelled),
    ];
    let text = section(&runs, false);
    insta::assert_snapshot!(text);
    for line in [
        "    failed: cannot read the target list: No such file or directory (os error 2)",
        "    failed: the target list is larger than 65536 bytes",
        "    failed: the target list is not in its format: expected value at line 1 column 1",
        "    skipped: the target list needs a newer lowendaff-bench (format 2)",
        "    failed: the target list names a format that does not exist: 0",
        "    failed: the target list has 201 targets, more than 200",
        "    failed: target 3 of the target list breaks a rule: its id must be 1 to 40 characters \
         of a to z, digits and -, other than youtube and netflix",
        "    failed: target 1 of the target list breaks a rule: its group must be china or \
         international",
        "    failed: target 2 of the target list breaks a rule: its name must be 1 to 40 \
         characters that print",
        "    failed: target 4 of the target list breaks a rule: its AS number must be from 1 to \
         4294967295",
        "    failed: target 5 of the target list breaks a rule: its ipv4 must be a DNS name or an \
         IPv4 address",
        "    failed: target 6 of the target list breaks a rule: its ipv6 must be a DNS name or an \
         IPv6 address",
        "    failed: two targets of the target list have the id pek-4809",
        "    skipped: no target of the list is chosen",
        "    skipped: no target in the list for xyz, abc",
        "    skipped: no chosen target has a name of a network family of this host",
        "    failed: no target got a route",
        "    failed: interrupted",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}

#[test]
fn a_port_rule_of_the_exchange_list_names_its_exchange() {
    let output = trace::Output {
        exchanges: ExchangeList::Unusable(ListError::Entry(7, Rule::Exchange)),
        ..output()
    };
    let text = section(&[run(Status::Completed, Some(output), None)], false);
    assert!(
        text.contains(
            "no exchange names: port 7 of the list of exchange ports breaks a rule: its \
             exchange must be 1 to 60 characters that print"
        ),
        "{text}"
    );
    assert!(
        text.contains("no target in the list for xyz"),
        "the note on unknown ids: {text}"
    );
}
