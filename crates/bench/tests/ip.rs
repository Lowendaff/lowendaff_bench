//! IP reputation's part of the report (`[IP]`): default IP and PTR masking versus `--show-ip`,
//! native and broadcast origin, risk scores, risk factors, DNSBLs, port-25 mail, both `ip.quality`
//! and `ip.media` sharing `[IP]`, and every failure/skip reason.

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
use plugin_ip::quality::DnsblSummary;
use plugin_ip::quality::FactorSummary;
use plugin_ip::quality::Failure;
use plugin_ip::quality::FamilyReport;
use plugin_ip::quality::GeoVote;
use plugin_ip::quality::GreyNoiseClass;
use plugin_ip::quality::MailCheck;
use plugin_ip::quality::MailStatus;
use plugin_ip::quality::Origin;
use plugin_ip::quality::Output;
use plugin_ip::quality::RiskFactor;
use plugin_ip::quality::RiskLevel;
use plugin_ip::quality::ScoreVerdict;
use plugin_ip::quality::Settings;
use plugin_ip::quality::UsageType;

fn run(status: Status, output: Option<Output>, stop: Option<Stop<Failure>>) -> Run<Measured> {
    Run {
        id: "ip.quality",
        group: "ip",
        title: Some("IP reputation"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_secs_f64(3.8)),
        status,
        result: Some(Measured::IpQuality(Report { output, stop })),
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
            ipv6: Family {
                targets: vec![(
                    "[2606:4700:4700::1111]:443".parse().expect("addr6"),
                    Reach::Connected,
                )],
                address: Address::Found {
                    address: "2001:db8::10".parse().expect("ip6"),
                    source: "Cloudflare",
                },
            },
        },
        ..HostProfile::default()
    }
}

fn section(runs: &[Run<Measured>], show_ip: bool) -> String {
    let host = host();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip,
        not_applicable: &[],
        runs,
        total: Duration::from_secs_f64(4.8),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("[IP]").expect("an IP section");
    text[start..].to_owned()
}

fn sample_output() -> Output {
    let factors = |dc: Vec<&'static str>| -> Vec<FactorSummary> {
        RiskFactor::ALL
            .into_iter()
            .map(|factor| FactorSummary {
                factor,
                checked: 3,
                flagged_by: if factor == RiskFactor::Datacenter {
                    dc.clone()
                } else {
                    Vec::new()
                },
            })
            .collect()
    };

    let v4 = FamilyReport {
        address: "198.51.100.10".parse().expect("v4"),
        asn: Some(13335),
        org: Some("Cloudflare, Inc.".to_owned()),
        hostname: Some("host-198-51-100-10.example.net".to_owned()),
        rir: Some("APNIC".to_owned()),
        registered_country: Some("SG".to_owned()),
        geo_country: Some("SG".to_owned()),
        geo_votes: vec![GeoVote {
            country: "SG".to_owned(),
            count: 5,
        }],
        geo_total: 5,
        origin: Origin::Native {
            country: "SG".to_owned(),
        },
        usage: Some(UsageType::Hosting),
        scores: vec![
            (
                "Scamalytics",
                ScoreVerdict::Numeric {
                    score: 12,
                    max: 100,
                    level: RiskLevel::Low,
                },
            ),
            (
                "ProxyCheck",
                ScoreVerdict::Numeric {
                    score: 15,
                    max: 100,
                    level: RiskLevel::Low,
                },
            ),
            (
                "ipapi.is",
                ScoreVerdict::Ratio {
                    text: "0.0039".to_owned(),
                    level: RiskLevel::Low,
                },
            ),
            (
                "GreyNoise",
                ScoreVerdict::GreyNoise {
                    riot: false,
                    noise: false,
                    classification: GreyNoiseClass::NotObserved,
                },
            ),
            (
                "Shodan",
                ScoreVerdict::Shodan {
                    ports: 2,
                    vulns: 0,
                    flagged: false,
                },
            ),
        ],
        factors: factors(vec!["ipapi.is", "Shodan"]),
        dnsbl: Some(DnsblSummary {
            checked: 23,
            refused: 1,
            listed: Vec::new(),
        }),
        mail: vec![
            MailCheck {
                id: "gmail",
                name: "Gmail",
                status: MailStatus::Deliverable(220),
            },
            MailCheck {
                id: "icloud",
                name: "iCloud",
                status: MailStatus::Rejected(554),
            },
        ],
        notes: Vec::new(),
    };

    let v6 = FamilyReport {
        address: "2001:db8::10".parse().expect("v6"),
        asn: Some(13335),
        org: Some("Cloudflare, Inc.".to_owned()),
        hostname: None,
        rir: Some("ARIN".to_owned()),
        registered_country: Some("US".to_owned()),
        geo_country: Some("SG".to_owned()),
        geo_votes: vec![
            GeoVote {
                country: "SG".to_owned(),
                count: 4,
            },
            GeoVote {
                country: "US".to_owned(),
                count: 1,
            },
        ],
        geo_total: 5,
        origin: Origin::Broadcast {
            registered: "US".to_owned(),
            located: "SG".to_owned(),
        },
        usage: Some(UsageType::Hosting),
        scores: vec![
            ("Scamalytics", ScoreVerdict::BlockedWaf),
            (
                "ProxyCheck",
                ScoreVerdict::Numeric {
                    score: 0,
                    max: 100,
                    level: RiskLevel::Low,
                },
            ),
            (
                "ipapi.is",
                ScoreVerdict::Ratio {
                    text: "0.0012".to_owned(),
                    level: RiskLevel::Low,
                },
            ),
            ("GreyNoise", ScoreVerdict::NoAddress),
            ("Shodan", ScoreVerdict::NoAddress),
        ],
        factors: factors(vec!["ipapi.is"]),
        dnsbl: Some(DnsblSummary {
            checked: 9,
            refused: 0,
            listed: vec!["DroneBL"],
        }),
        mail: vec![
            MailCheck {
                id: "gmail",
                name: "Gmail",
                status: MailStatus::Blocked,
            },
            MailCheck {
                id: "icloud",
                name: "iCloud",
                status: MailStatus::NoAddress,
            },
        ],
        notes: vec![("DB-IP", "HTTP 502".to_owned())],
    };

    Output {
        settings: Settings::default(),
        ipv4_reachable: true,
        ipv6_reachable: true,
        ipv4: Some(Box::new(v4)),
        ipv6: Some(Box::new(v6)),
    }
}

#[test]
fn report_masks_addresses_and_hides_ptr_by_default_and_shows_them_with_show_ip() {
    let masked = section(
        &[run(Status::Completed, Some(sample_output()), None)],
        false,
    );
    assert!(masked.contains("198.51.*.*"));
    assert!(masked.contains("2001:db8:*"));
    assert!(!masked.contains("198.51.100.10"));
    assert!(!masked.contains("2001:db8::10"));
    assert!(
        !masked.contains("host-198-51-100-10.example.net"),
        "PTR hostname must stay hidden unless --show-ip is given"
    );
    assert!(masked.contains("native (SG)"));
    assert!(masked.contains("broadcast (US -> SG)"));
    assert!(masked.contains("listed (1/9: DroneBL)"));
    assert!(masked.contains("rejected (554)"));
    assert!(masked.contains("DB-IP over IPv6: HTTP 502"));
    insta::assert_snapshot!("ip_quality_masked", masked);

    let full = section(&[run(Status::Completed, Some(sample_output()), None)], true);
    assert!(full.contains("198.51.100.10"));
    assert!(full.contains("2001:db8::10"));
    assert!(full.contains("host-198-51-100-10.example.net"));
    insta::assert_snapshot!("ip_quality_show_ip", full);
}

#[test]
fn every_way_ip_quality_ends_shows_its_words() {
    let runs = [
        run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(Failure::NoFamily)),
        ),
        run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(Failure::NoPublicAddress)),
        ),
        run(
            Status::Failed(None),
            None,
            Some(Stop::Failed(Failure::InvalidEndpoint(
                "8.8.8.8:53".to_owned(),
            ))),
        ),
        run(
            Status::Failed(None),
            None,
            Some(Stop::Failed(Failure::Cancelled)),
        ),
    ];
    let shown = section(&runs, false);
    for expected in [
        "skipped: the host reaches neither IPv4 nor IPv6",
        "skipped: no public IP address was found for any reachable network family",
        concat!(
            "failed: endpoint override must be https:// or a loopback address ",
            "(127.0.0.1 / [::1]): 8.8.8.8:53"
        ),
        "failed: interrupted",
    ] {
        assert!(
            shown.contains(expected),
            "missing {expected:?} in:\n{shown}"
        );
    }
    insta::assert_snapshot!("ip_quality_failures", shown);
}

#[test]
fn ip_quality_and_ip_media_share_one_ip_section() {
    use plugin_ip::media;
    use plugin_ip::media::Group;
    use plugin_ip::media::ServiceResult;
    use plugin_ip::media::Verdict;

    let media_run = Run {
        id: "ip.media",
        group: "ip",
        title: Some("Media unlock"),
        started: Some(Duration::from_secs(5)),
        elapsed: Some(Duration::from_secs_f64(2.1)),
        status: Status::Completed,
        result: Some(Measured::IpMedia(Report {
            output: Some(media::Output {
                settings: media::Settings::default(),
                unknown: Vec::new(),
                ipv4_reachable: true,
                ipv6_reachable: true,
                results: vec![ServiceResult {
                    id: "netflix",
                    name: "Netflix",
                    group: Group::Global,
                    ipv4: Some(Verdict::yes_in("SG")),
                    ipv6: Some(Verdict::yes_in("SG")),
                }],
            }),
            stop: None,
        })),
    };
    let shown = section(
        &[
            run(Status::Completed, Some(sample_output()), None),
            media_run,
        ],
        false,
    );
    assert_eq!(shown.matches("[IP]\n").count(), 1);
    assert!(shown.contains("  IP reputation\n    Direct checks (no proxy):"));
    assert!(shown.contains("  Media unlock\n    HTTPS probes, direct (no proxy)"));
}
