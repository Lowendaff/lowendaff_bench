//! The network section of the report, from constructed runs: network speed's method, a table for
//! each family with its notes, iperf3's number formats, and each way the item ends. Compared with
//! snapshots and with the words themselves; and `--countries`, which sets the item's countries.

use std::collections::BTreeMap;
use std::time::Duration;

use bench::app;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::registry;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use plugin_network::iperf3;
use plugin_network::iperf3::result::Settings;
use plugin_network::iperf3::result::Speed;
use plugin_network::iperf3::result::Tested;
use plugin_network::iperf3::result::Way;
use plugin_network::iperf3::servers::ListError;
use plugin_network::iperf3::servers::Rule;
use serde_json::json;

fn run(
    status: Status,
    output: Option<iperf3::Output>,
    stop: Option<Stop<iperf3::Failure>>,
) -> Run<Measured> {
    Run {
        id: "network.iperf3",
        group: "network",
        title: Some("Network speed"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_secs(301)),
        status,
        result: Some(Measured::NetworkIperf3(Report { output, stop })),
    }
}

/// The report of `runs` from its network section on, with the summary.
fn section(runs: &[Run<Measured>]) -> String {
    let host = HostProfile::default();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs,
        total: Duration::from_secs(302),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("[Network]").expect("a network section");
    text[start..].to_owned()
}

/// Whether `text` holds `line` as a line of its own.
fn shows(text: &str, line: &str) -> bool {
    text.lines().any(|shown| shown == line)
}

fn measured(bits: u64, tries: u32, rtt: Option<u64>) -> Way {
    Way::Measured(Speed {
        bits,
        tries,
        rtt,
        retransmits: rtt.map(|_| 12),
    })
}

fn tested(location: &str, country: &str, provider: &str, upload: Way, download: Way) -> Tested {
    Tested {
        location: location.to_owned(),
        country: country.to_owned(),
        provider: provider.to_owned(),
        upload,
        download,
    }
}

/// The five default servers over IPv4, each way they can end, and one over IPv6.
fn output() -> iperf3::Output {
    iperf3::Output {
        version: Some("3.22".to_owned()),
        settings: Settings::YABS,
        unknown: vec!["BR".to_owned()],
        ipv4: vec![
            tested(
                "Los Angeles",
                "US",
                "Clouvider",
                measured(1_243_000_000, 1, Some(12_400)),
                measured(2_030_000_000, 1, None),
            ),
            tested(
                "New York",
                "US",
                "Leaseweb",
                measured(912_400_000, 2, Some(67_049)),
                measured(94_520_000, 1, None),
            ),
            tested(
                "Ashburn",
                "US",
                "Clouvider",
                Way::Busy,
                measured(8_731_000, 3, None),
            ),
            tested(
                "Tokyo",
                "JP",
                "Leaseweb",
                Way::Error("control socket has closed unexpectedly".to_owned()),
                Way::Fault(job::Fault::TimedOut(Duration::from_secs(20))),
            ),
            tested(
                "Frankfurt",
                "DE",
                "Leaseweb",
                Way::Unreachable,
                Way::Skipped,
            ),
        ],
        ipv6: vec![tested(
            "Tokyo",
            "JP",
            "Leaseweb",
            measured(410_000_000, 1, Some(104_301)),
            Way::Nothing,
        )],
    }
}

#[test]
fn each_family_has_a_table_with_the_speeds_latency_and_notes() {
    let text = section(&[run(Status::Completed, Some(output()), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  iperf3 3.22: 8 streams, 10 s each way, TCP",
        "  Note                no server in the list for BR",
        "  IPv4",
        "  IPv6",
        "  Note                New York, US: the upload took 2 tries",
        "  Note                Ashburn, US: the download took 3 tries",
        "  Note                Tokyo, JP: the upload failed: control socket has closed \
         unexpectedly",
        "  Note                Tokyo, JP: the download failed: iperf3 measured nothing",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    let row = |place: &str| {
        text.lines()
            .find(|line| line.trim_start().starts_with(place))
            .unwrap_or_else(|| panic!("a row for {place} in {text}"))
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ")
    };
    assert_eq!(
        row("Los Angeles"),
        "Los Angeles, US Clouvider 1.24 Gbit/s 2.03 Gbit/s 12.4 ms"
    );
    assert_eq!(
        row("New York"),
        "New York, US Leaseweb 912 Mbit/s 94.5 Mbit/s 67.0 ms"
    );
    assert_eq!(row("Ashburn"), "Ashburn, US Clouvider busy 8.73 Mbit/s —");
    assert_eq!(
        row("Frankfurt"),
        "Frankfurt, DE Leaseweb unreachable not tried —"
    );
    assert!(
        text.lines().any(|line| line.contains("Tokyo, JP")
            && line.contains("error  ")
            && line.contains("timed out")),
        "{text}"
    );
}

#[test]
fn a_limit_on_the_streams_shows_in_the_method() {
    let limited = iperf3::Output {
        version: None,
        settings: Settings {
            seconds: 3,
            streams: 1,
            rate: Some(100),
        },
        ..output()
    };
    let text = section(&[run(Status::Completed, Some(limited), None)]);
    assert!(
        shows(
            &text,
            "  iperf3: 1 stream, 3 s each way, TCP, at most 100 Mbit/s a stream"
        ),
        "{text}"
    );
}

#[test]
fn speeds_and_latencies_show_three_digits_or_more_as_iperf3_prints_them() {
    let bits = report::terminal::bits;
    assert_eq!(bits(999_999_999), "1000 Mbit/s");
    assert_eq!(bits(1_000_000_000), "1.00 Gbit/s");
    assert_eq!(bits(12_340_000_000), "12.3 Gbit/s");
    assert_eq!(bits(123_400_000_000), "123 Gbit/s");
    assert_eq!(bits(99_940_000), "99.9 Mbit/s");
    assert_eq!(bits(9_990_000), "9.99 Mbit/s");
    assert_eq!(bits(0), "0.00 Mbit/s");
    let latency = report::terminal::latency;
    assert_eq!(latency(8), "0.01 ms");
    assert_eq!(latency(1_234), "1.23 ms");
    assert_eq!(latency(12_449), "12.4 ms");
    assert_eq!(latency(250_000), "250 ms");
}

#[test]
fn every_way_the_item_ends_shows_its_words() {
    let skipped = |failure| run(Status::Skipped(None), None, Some(Stop::Skipped(failure)));
    let failed = |failure| run(Status::Failed(None), None, Some(Stop::Failed(failure)));
    let list = |error| failed(iperf3::Failure::List(error));
    let runs = [
        failed(iperf3::Failure::Unreadable(
            "No such file or directory (os error 2)".to_owned(),
        )),
        list(ListError::TooLarge),
        list(ListError::Format(
            "expected value at line 1 column 1".to_owned(),
        )),
        skipped(iperf3::Failure::List(ListError::Newer(2))),
        list(ListError::Version(0)),
        list(ListError::TooMany(101)),
        list(ListError::Server(3, Rule::Id)),
        list(ListError::Server(1, Rule::Country)),
        list(ListError::Server(2, Rule::Host)),
        list(ListError::Server(4, Rule::Ports)),
        list(ListError::Server(5, Rule::Families)),
        list(ListError::Server(6, Rule::Text)),
        list(ListError::Duplicate("tokyo".to_owned())),
        skipped(iperf3::Failure::NoServer(Vec::new())),
        skipped(iperf3::Failure::NoServer(vec![
            "BR".to_owned(),
            "xx".to_owned(),
        ])),
        skipped(iperf3::Failure::NoFamily),
        failed(iperf3::Failure::NoResult),
        failed(iperf3::Failure::Cancelled),
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    failed: cannot read the server list: No such file or directory (os error 2)",
        "    failed: the server list is larger than 65536 bytes",
        "    failed: the server list is not in its format: expected value at line 1 column 1",
        "    skipped: the server list needs a newer lowendaff-bench (format 2)",
        "    failed: the server list names a format that does not exist: 0",
        "    failed: the server list has 101 servers, more than 100",
        "    failed: server 3 of the server list breaks a rule: its id must be 1 to 40 characters \
         of a to z, digits and -",
        "    failed: server 1 of the server list breaks a rule: its country must be two capital \
         letters",
        "    failed: server 2 of the server list breaks a rule: its host must be a DNS name or an \
         IP address",
        "    failed: server 4 of the server list breaks a rule: its ports must be one or a range \
         from 1024 to 65535, at most 64",
        "    failed: server 5 of the server list breaks a rule: its families must be ipv4, ipv6 or \
         both",
        "    failed: server 6 of the server list breaks a rule: its location, provider and link \
         must be 1 to 40 characters that print",
        "    failed: two servers of the server list have the id tokyo",
        "    skipped: no server of the list is chosen",
        "    skipped: no server in the list for BR, xx",
        "    skipped: no chosen server answers over a network family of this host",
        "    failed: no server gave a speed",
        "    failed: interrupted",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}

#[test]
fn countries_on_the_command_line_win_over_the_config_files() {
    let mut config = BTreeMap::from([(
        "network.iperf3".to_owned(),
        json!({ "countries": ["DE"], "seconds": 3 }),
    )]);
    app::countries(&mut config, &["us".to_owned(), "JP".to_owned()]);
    assert_eq!(
        config["network.iperf3"],
        json!({ "countries": ["us", "JP"], "seconds": 3 })
    );

    let mut config = BTreeMap::new();
    app::countries(&mut config, &["all".to_owned()]);
    assert_eq!(config["network.iperf3"], json!({ "countries": ["all"] }));

    let mut config = BTreeMap::from([("network.iperf3".to_owned(), json!(5))]);
    app::countries(&mut config, &["JP".to_owned()]);
    assert_eq!(
        config["network.iperf3"],
        json!(5),
        "left for the plan to refuse"
    );
}

#[test]
fn the_plan_takes_the_countries_of_the_command_line_over_inline_overrides() {
    use clap::Parser;

    // Countries that the item refuses in the inline overrides show which countries the plan took.
    let overrides = r#"{"network.iperf3":{"countries":5}}"#;
    let cli = |countries: &[&str]| {
        let args = [
            "lowendaff-bench",
            "--only",
            "network",
            "--internal-overrides",
            overrides,
        ];
        app::Cli::try_parse_from(args.iter().chain(countries).copied()).expect("arguments")
    };
    assert!(app::prepare(&cli(&[]), &registry::all()).is_err());
    assert!(app::prepare(&cli(&["--countries", "JP,de"]), &registry::all()).is_ok());
}
