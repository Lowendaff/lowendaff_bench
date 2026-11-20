//! Streaming media unlock's part of the report: regional tables, IPv4 and IPv6 columns, notes on
//! unknown ids and unexpected responses, and every way the item ends.

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
use plugin_media::unlock::Choice;
use plugin_media::unlock::Failure;
use plugin_media::unlock::Group;
use plugin_media::unlock::Output;
use plugin_media::unlock::Restriction;
use plugin_media::unlock::ServiceResult;
use plugin_media::unlock::Settings;
use plugin_media::unlock::Verdict;

fn run(status: Status, output: Option<Output>, stop: Option<Stop<Failure>>) -> Run<Measured> {
    Run {
        id: "media.unlock",
        group: "media",
        title: Some("Media unlock"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_secs_f64(6.4)),
        status,
        result: Some(Measured::MediaUnlock(Report { output, stop })),
    }
}

fn host() -> HostProfile {
    HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![("1.1.1.1:443".parse().expect("addr"), Reach::Connected)],
                address: Address::Found {
                    address: "198.51.100.10".parse().expect("ip"),
                    source: "Cloudflare",
                },
            },
            ipv6: Family::default(),
        },
        ..HostProfile::default()
    }
}

fn section(runs: &[Run<Measured>]) -> String {
    let host = host();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs,
        total: Duration::from_secs_f64(7.4),
    };
    let text = report::terminal::render(&page, None);
    let start = text
        .find("[Streaming media]")
        .expect("a streaming media section");
    text[start..].to_owned()
}

#[test]
fn regional_tables_show_every_verdict_and_notes_follow_each_group() {
    let output = Output {
        settings: Settings {
            groups: vec![Choice::Global, Choice::Asia],
            services: Vec::new(),
            custom_base: false,
        },
        unknown: vec!["mystery-tv".to_owned()],
        ipv4_reachable: true,
        ipv6_reachable: true,
        results: vec![
            ServiceResult {
                id: "netflix",
                name: "Netflix",
                group: Group::Global,
                ipv4: Some(Verdict::yes_in("US")),
                ipv6: Some(Verdict::OriginalsOnly {
                    region: Some("US".to_owned()),
                }),
            },
            ServiceResult {
                id: "youtube",
                name: "YouTube Premium",
                group: Group::Global,
                ipv4: Some(Verdict::yes_in("US")),
                ipv6: Some(Verdict::No {
                    restriction: Some(Restriction::ChinaMainland),
                }),
            },
            ServiceResult {
                id: "disney",
                name: "Disney+",
                group: Group::Global,
                ipv4: Some(Verdict::BlockedWaf),
                ipv6: Some(Verdict::RateLimited),
            },
            ServiceResult {
                id: "bilibili",
                name: "Bilibili",
                group: Group::Asia,
                ipv4: Some(Verdict::Yes {
                    region: Some("HK/MO".to_owned()),
                }),
                ipv6: Some(Verdict::NoAddress),
            },
            ServiceResult {
                id: "abema",
                name: "Abema",
                group: Group::Asia,
                ipv4: Some(Verdict::TimedOut),
                ipv6: Some(Verdict::Unexpected("HTTP 503".to_owned())),
            },
        ],
    };
    let shown = section(&[run(Status::Completed, Some(output), None)]);
    assert!(shown.contains("HTTPS probes, direct (no proxy); groups global, asia\n"));
    assert!(shown.contains("no streaming service in the catalog for mystery-tv\n"));
    assert!(shown.contains("Netflix          yes (US)       originals only (US)\n"));
    assert!(shown.contains("YouTube Premium  yes (US)       no (CN)\n"));
    assert!(shown.contains("Disney+          blocked (WAF)  rate limited\n"));
    assert!(shown.contains("Bilibili  yes (HK/MO)  no IPv6 address\n"));
    assert!(shown.contains("Abema over IPv6: HTTP 503\n"));
    insta::assert_snapshot!(shown);
}

#[test]
fn every_way_media_unlock_ends_shows_its_words() {
    let runs = [
        run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(Failure::NoFamily)),
        ),
        run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(Failure::NoService(vec!["foo".to_owned()]))),
        ),
        run(
            Status::Failed(None),
            None,
            Some(Stop::Failed(Failure::InvalidBaseUrl(
                "http://bad".to_owned(),
            ))),
        ),
        run(
            Status::Failed(None),
            None,
            Some(Stop::Failed(Failure::Cancelled)),
        ),
    ];
    let shown = section(&runs);
    for expected in [
        "skipped: the host reaches neither IPv4 nor IPv6",
        "skipped: no streaming service in the catalog for foo",
        concat!(
            "failed: base_url must start with https:// or loopback ",
            "http://127.0.0.1 or http://[::1]: http://bad"
        ),
        "failed: interrupted",
    ] {
        assert!(
            shown.contains(expected),
            "expected {expected:?} in:\n{shown}"
        );
    }
    insta::assert_snapshot!(shown);
}
