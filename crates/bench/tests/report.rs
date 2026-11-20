//! The terminal report, from constructed runs: sections by group in the report's order, each
//! status, numbers aligned right, wrapping to a terminal's width, and the summary. Whole reports
//! are compared with snapshots in `tests/snapshots/`; each group's words have tests of their own.

use std::collections::BTreeMap;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Fault;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::report;
use bench::report::Format;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Net;
use bench_api::Stop;
use bench_host::cards::Bus;
use bench_host::cards::Link;
use bench_host::cards::NetworkCard;
use bench_host::cards::Routes;
use bench_host::cpu::Cpu;
use bench_host::cpu::Model;
use bench_host::cpu::Virtualization;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::MemoryFacts;
use bench_host::profile::Read;
use bench_host::profile::Seen;
use bench_host::profile::StorageFacts;
use bench_host::profile::SystemFacts;
use bench_host::storage::Disk;
use bench_host::storage::Filesystem;
use bench_host::storage::Space;
use bench_host::storage::Use;
use plugin_cpu::sysbench;
use plugin_cpu::sysbench::result::Rate;
use plugin_disk::fio;
use plugin_disk::fio::result::Outcome;
use plugin_disk::fio::result::Rates;
use plugin_disk::fio::result::Test;
use plugin_disk::health;
use plugin_disk::health::location::Location;
use plugin_disk::health::result::Disk as HealthDisk;
use plugin_disk::health::result::Health;
use plugin_disk::health::result::Values;
use plugin_ip::quality;
use plugin_ip::quality::FamilyReport;
use plugin_ip::quality::Origin;
use plugin_memory::stream;
use plugin_network::bgp;
use plugin_network::bgp::BgpPath;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::OriginSummary;
use plugin_network::bgp::RpkiStatus;
use plugin_network::bgp::UpstreamShare;
use plugin_network::trace;
use plugin_network::trace::result::Answer;
use plugin_network::trace::result::ExchangeList;
use plugin_network::trace::result::Hop;
use plugin_network::trace::result::Route;
use plugin_network::trace::result::Target;
use plugin_network::trace::result::Trace;
use plugin_network::trace::result::Traced;

/// A small virtual machine, as the report starts with it: its IPv4 address is a documentation
/// address, and it has no IPv6.
fn host() -> HostProfile {
    let cpu = Cpu {
        model: Some(Model::Name("Example CPU @ 2.00GHz".to_owned())),
        logical: Some(2),
        mhz: Some(2000.0),
        caches: vec![("L1d".to_owned(), 32 << 10), ("L2".to_owned(), 4 << 20)],
        aes: Some(true),
        virtualization: Virtualization::Absent,
        ..Cpu::default()
    };
    let memory = MemoryFacts {
        total: 2 << 30,
        used: 1 << 30,
        estimated: false,
        swap_total: 0,
        swap_used: 0,
    };
    let disk = Disk {
        name: "vda".to_owned(),
        interface: "virtio",
        size: 40 << 30,
        rotational: None,
        model: None,
        uses: vec![Use::Mount("/".to_owned())],
    };
    let root = Filesystem {
        mount_point: "/".to_owned(),
        fs_type: "ext4".to_owned(),
        devices: vec![(254, 1)],
        zfs_pool: None,
        disks: vec!["vda".to_owned()],
        space: Some(Space {
            total: 39 << 30,
            used: 10 << 30,
            available: 27 << 30,
        }),
    };
    let system = SystemFacts {
        distribution: Some("Debian GNU/Linux 13 (trixie)".to_owned()),
        kernel: Some("6.12.73+deb13-amd64".to_owned()),
        uptime: Some(Seen {
            value: Duration::from_secs(3 * 86_400 + 4 * 3600),
            host: false,
        }),
        load: Some(Seen {
            value: [0.1, 0.2, 0.3],
            host: false,
        }),
        congestion_control: Some("bbr".to_owned()),
        qdisc: Some("fq".to_owned()),
    };
    let target = |text: &str| text.parse().expect("a target");
    HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        arch: "x86_64".to_owned(),
        libc: Libc::Glibc(GlibcVersion {
            major: 2,
            minor: 41,
        }),
        cpu: Read::Done(CpuFacts {
            cpu,
            usable: Some(2),
            quota: None,
        }),
        memory: Read::Done(Some(memory)),
        numa: Read::default(),
        storage: Read::Done(StorageFacts {
            disks: vec![disk],
            filesystems: vec![root],
            raid: None,
            work_dir: Some((254, 1)),
        }),
        system: Read::Done(system),
        hardware: None,
        cards: Read::Done(vec![NetworkCard {
            interface: "eth0".to_owned(),
            bus: Bus::Pci {
                vendor: 0x1af4,
                device: 0x1000,
            },
            model: Some("Red Hat Virtio network device".to_owned()),
            driver: Some("virtio_net".to_owned()),
            link: Link::Up {
                speed: None,
                duplex: None,
            },
            mtu: Some(1500),
            pcie: None,
            virtual_functions: 0,
            master: None,
            default_route: Routes {
                ipv4: true,
                ipv6: false,
            },
        }]),
        network: Network {
            ipv4: Family {
                targets: vec![(target("1.1.1.1:443"), Reach::Connected)],
                address: Address::Found {
                    address: "203.0.113.7".parse().expect("an address"),
                    source: "Cloudflare",
                },
            },
            ipv6: Family {
                targets: vec![
                    (target("[2606:4700:4700::1111]:443"), Reach::NoRoute),
                    (target("[2400:3200::1]:443"), Reach::NoRoute),
                ],
                address: Address::NotAsked,
            },
        },
    }
}

/// A run that the reporter names as the program does.
fn run(
    id: &'static str,
    status: Status,
    seconds: Option<f64>,
    result: Option<Measured>,
) -> Run<Measured> {
    Run {
        id,
        group: orchestrator::group_of(id),
        title: report::terminal::name(id),
        started: seconds.map(|_| Duration::ZERO),
        elapsed: seconds.map(Duration::from_secs_f64),
        status,
        result,
    }
}

/// smartctl's reading of one disk that passed, with its temperature only.
fn healthy(name: &str) -> HealthDisk {
    HealthDisk {
        location: Location::Named(name.to_owned()),
        health: Health::Passed,
        values: Values {
            temperature: Some(25),
            ..Values::default()
        },
    }
}

fn health(disks: Vec<HealthDisk>) -> Measured {
    Measured::DiskHealth(Report {
        output: Some(health::Output {
            version: Some("7.5".to_owned()),
            disks,
            nodes_left: Vec::new(),
        }),
        stop: None,
    })
}

fn rate(value: f64) -> Rate {
    Rate {
        text: format!("{value:.2}"),
        value,
    }
}

fn sysbench_run(threads: usize, value: f64) -> sysbench::result::Run {
    sysbench::result::Run {
        threads,
        rate: rate(value),
        retries: Vec::new(),
    }
}

/// Every status, in the run order, which differs from the report's; the two plugins of a group
/// that the report does not know come last, in a section of their own.
fn runs() -> Vec<Run<Measured>> {
    vec![
        run(
            "disk.health",
            Status::TimedOut,
            Some(60.0),
            Some(health(vec![healthy("sda")])),
        ),
        run(
            "cpu.sysbench",
            Status::Completed,
            Some(41.2),
            Some(Measured::CpuSysbench(Report {
                output: Some(sysbench::Output {
                    version: Some("1.0.20".to_owned()),
                    single: sysbench_run(1, 859.0),
                    multi: Some(sysbench_run(64, 44631.33)),
                }),
                stop: None,
            })),
        ),
        run(
            "memory.stream",
            Status::Failed(None),
            Some(36.3),
            Some(Measured::MemoryStream(Report {
                output: None,
                stop: Some(Stop::Failed(stream::Failure::Run {
                    threads: 1,
                    faults: vec![stream::Fault::NoResult; 4],
                })),
            })),
        ),
        run(
            "disk.fio",
            Status::Skipped(Some(Unmet::Tool {
                name: "fio",
                why: "download failed: HTTP 404".to_owned(),
            })),
            None,
            None,
        ),
        run("cpu.geekbench", Status::Cancelled, None, None),
        run(
            "test.own-skip",
            Status::Skipped(None),
            Some(0.1),
            Some(Measured::DiskFio(Report {
                output: None,
                stop: Some(Stop::Skipped(fio::Failure::InMemory)),
            })),
        ),
        run(
            "test.panic",
            Status::Failed(Some(Fault::Panic("index out of bounds".to_owned()))),
            Some(0.2),
            None,
        ),
    ]
}

#[test]
fn every_status_in_its_group() {
    let runs = runs();
    let page = Page {
        version: "0.1.0",
        host: &host(),
        show_ip: false,
        not_applicable: &["Memory NUMA"],
        runs: &runs,
        total: Duration::from_secs_f64(137.8),
    };
    insta::assert_snapshot!(report::terminal::render(&page, None));
}

#[test]
fn a_run_with_one_plugin_per_section() {
    let rates = Rates {
        read_kib: 95_045,
        read_iops: 23_761,
        write_kib: 95_294,
        write_iops: 23_823,
    };
    let fio = fio::Output {
        version: Some("3.43".to_owned()),
        size: 2 << 30,
        zfs_needed: None,
        tests: vec![Test {
            block_size: "4k",
            outcome: Outcome::Done(rates),
        }],
    };
    let runs = vec![
        run(
            "disk.fio",
            Status::Completed,
            Some(0.2),
            Some(Measured::DiskFio(Report {
                output: Some(fio),
                stop: None,
            })),
        ),
        run(
            "cpu.geekbench",
            Status::Skipped(Some(Unmet::Network(Net::Ipv4))),
            None,
            None,
        ),
    ];
    let page = Page {
        version: "0.1.0",
        host: &host(),
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_secs(1),
    };
    insta::assert_snapshot!(report::terminal::render(&page, None));
}

#[test]
fn tables_wrap_to_a_terminals_width_only() {
    let long = "a long device name that wraps on a narrow terminal and stays on one line in a file";
    let runs = vec![run(
        "disk.health",
        Status::Completed,
        Some(1.0),
        Some(health(vec![healthy(long)])),
    )];
    let page = Page {
        version: "0.1.0",
        host: &host(),
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_secs(1),
    };
    let unwrapped = report::terminal::render(&page, None);
    assert!(
        unwrapped.lines().any(|line| line.contains(long)),
        "without a terminal a row stays on one line: {unwrapped}"
    );
    let narrow = report::terminal::render(&page, Some(80));
    assert!(
        !narrow.lines().any(|line| line.contains(long)),
        "on an 80-column terminal the cell wraps: {narrow}"
    );
    insta::assert_snapshot!(narrow);
}

#[test]
fn requirements_and_faults_have_their_words() {
    let needed = GlibcVersion {
        major: 2,
        minor: 35,
    };
    let found = Some(GlibcVersion {
        major: 2,
        minor: 34,
    });
    let texts = [
        (
            Unmet::Glibc { needed, found },
            "needs glibc 2.35 or newer; this host has 2.34",
        ),
        (
            Unmet::Glibc {
                needed,
                found: None,
            },
            "needs glibc 2.35 or newer; this host has no glibc",
        ),
        (Unmet::Network(Net::Ipv4), "needs an IPv4 connection"),
        (Unmet::Network(Net::Ipv6), "needs an IPv6 connection"),
        (Unmet::Network(Net::Any), "needs a network connection"),
        (
            Unmet::Tool {
                name: "fio",
                why: "download failed: HTTP 404".to_owned(),
            },
            "tool fio unavailable: download failed: HTTP 404",
        ),
        (
            Unmet::Tool {
                name: "fio",
                why: String::new(),
            },
            "tool fio unavailable",
        ),
        (
            Unmet::File {
                name: "servers",
                why: "download failed: HTTP 404".to_owned(),
            },
            "file servers unavailable: download failed: HTTP 404",
        ),
        (
            Unmet::File {
                name: "servers",
                why: String::new(),
            },
            "file servers unavailable",
        ),
    ];
    for (unmet, text) in texts {
        assert_eq!(report::terminal::unmet_text(&unmet), text);
    }
    assert_eq!(
        report::terminal::fault_text(&Fault::Panic("boom".into())),
        "the plugin crashed: boom"
    );
    assert_eq!(
        report::terminal::fault_text(&Fault::WorkDir("No space left on device".into())),
        "cannot create the work directory: No space left on device"
    );
    assert_eq!(
        report::terminal::fault_text(&Fault::Aborted("task was cancelled".into())),
        "the task ended abnormally: task was cancelled"
    );
}

#[test]
fn markdown_format_renders_gfm_headings_pipe_tables_and_escaped_masked_ips() {
    let runs = runs();
    let host = host();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &["Memory NUMA"],
        runs: &runs,
        total: Duration::from_secs_f64(137.8),
    };
    let md = report::render(&page, Format::Markdown, Some(40));
    assert!(md.starts_with("# lowendaff-bench 0.1.0\n\nEnvironment: KVM virtual machine\n"));
    assert!(md.contains("\n## System\n"), "{md}");
    assert!(
        md.contains("- **IPv4:** available; public address 203.0.\\*.\\* (from Cloudflare)\n"),
        "masked IPv4 asterisks must be escaped so CommonMark does not italicize them: {md}"
    );
    assert!(
        !md.contains("203.0.113.7"),
        "raw IPv4 must stay masked: {md}"
    );
    assert!(
        md.contains("\n### Disks\n\n| Device | Interface | Size | Model | Use |\n"),
        "{md}"
    );
    assert!(
        md.contains(
            "\n## CPU performance\n\n### CPU performance\n\nsysbench 1.0.20: prime limit 10000, \
             20 s each\n\n| Test | Threads | Events/s |\n| --- | ---: | ---: |\n\
             | Single thread | 1 | 859.00 |\n"
        ),
        "numeric columns align right and multi-plugin lines are never indented 4 spaces: {md}"
    );
    assert!(
        !md.contains("\n#### 4K and 64K\n"),
        "no fio result in this page: {md}"
    );
    assert!(md.contains("\n## Summary\n"), "{md}");
    assert!(md.contains("\nNot applicable here: Memory NUMA\n"), "{md}");

    let single = vec![run(
        "disk.fio",
        Status::Completed,
        Some(0.2),
        Some(Measured::DiskFio(Report {
            output: Some(fio::Output {
                version: Some("3.43".to_owned()),
                size: 2 << 30,
                zfs_needed: None,
                tests: vec![Test {
                    block_size: "4k",
                    outcome: Outcome::Done(Rates {
                        read_kib: 95_045,
                        read_iops: 23_761,
                        write_kib: 95_294,
                        write_iops: 23_823,
                    }),
                }],
            }),
            stop: None,
        })),
    )];
    let single_page = Page {
        runs: &single,
        ..page
    };
    let single_md = report::render(&single_page, Format::Markdown, None);
    assert!(
        single_md.contains("\n## Disks\n\nfio 3.43\n") && single_md.contains("\n### 4K and 64K\n"),
        "single-plugin section uses h3 for table subheadings without skipping levels: {single_md}"
    );
}

#[test]
fn json_format_is_structured_and_respects_show_ip_across_host_trace_and_ip_quality() {
    let mut host = host();
    host.network.ipv6 = Family {
        targets: vec![(
            "[2606:4700:4700::1111]:443".parse().expect("target"),
            Reach::Connected,
        )],
        address: Address::Found {
            address: "2001:db8::7".parse().expect("ipv6"),
            source: "ipify",
        },
    };
    let v4_rep = FamilyReport {
        address: "203.0.113.7".parse().expect("ipv4"),
        asn: Some(13335),
        org: Some("Cloudflare".to_owned()),
        hostname: Some("host-203-0-113-7.example.net".to_owned()),
        rir: Some("APNIC".to_owned()),
        registered_country: Some("AU".to_owned()),
        geo_country: Some("AU".to_owned()),
        geo_votes: Vec::new(),
        geo_total: 0,
        origin: Origin::Native {
            country: "AU".to_owned(),
        },
        usage: None,
        scores: Vec::new(),
        factors: Vec::new(),
        dnsbl: None,
        mail: Vec::new(),
        notes: Vec::new(),
    };
    let v6_rep = FamilyReport {
        address: "2001:db8::7".parse().expect("ipv6"),
        hostname: Some("v6.example.net".to_owned()),
        ..v4_rep.clone()
    };
    let quality_run = run(
        "ip.quality",
        Status::Completed,
        Some(1.5),
        Some(Measured::IpQuality(Report {
            output: Some(quality::Output {
                settings: quality::Settings::default(),
                ipv4_reachable: true,
                ipv6_reachable: true,
                ipv4: Some(Box::new(v4_rep)),
                ipv6: Some(Box::new(v6_rep)),
            }),
            stop: None,
        })),
    );
    let trace_run = run(
        "network.trace",
        Status::Completed,
        Some(2.0),
        Some(Measured::NetworkTrace(Report {
            output: Some(trace::Output {
                settings: trace::result::Settings {
                    groups: Vec::new(),
                    targets: Vec::new(),
                    list: false,
                    geo: true,
                },
                unknown: Vec::new(),
                exchanges: ExchangeList::Read(0),
                ipv4: vec![Traced {
                    target: Target::YouTube,
                    host: Some("r1.googlevideo.com".to_owned()),
                    route: Route::Traced(Trace {
                        reached: true,
                        hops: vec![
                            Hop {
                                answers: vec![Answer {
                                    address: "203.0.113.7".parse().expect("own hop"),
                                    network: None,
                                    exchange: None,
                                    place: None,
                                    latency: 500,
                                }],
                            },
                            Hop {
                                answers: vec![Answer {
                                    address: "198.51.100.1".parse().expect("router hop"),
                                    network: Some("AS64496 Example".to_owned()),
                                    exchange: None,
                                    place: None,
                                    latency: 1500,
                                }],
                            },
                        ],
                    }),
                }],
                ipv6: Vec::new(),
            }),
            stop: None,
        })),
    );
    let runs = vec![quality_run, trace_run];

    let masked_page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &["Disk health"],
        runs: &runs,
        total: Duration::from_secs_f64(3.5),
    };
    let masked_raw = report::render(&masked_page, Format::Json, None);
    for secret in [
        "203.0.113.7",
        "2001:db8::7",
        "host-203-0-113-7.example.net",
        "v6.example.net",
    ] {
        assert!(
            !masked_raw.contains(secret),
            "JSON without --show-ip leaked {secret}: {masked_raw}"
        );
    }
    let masked: serde_json::Value = serde_json::from_str(&masked_raw).expect("valid JSON");
    assert_eq!(masked["show_ip"], false);
    assert_eq!(
        masked["system"]["network"]["ipv4"]["public_address"],
        "203.0.*.*"
    );
    assert_eq!(
        masked["system"]["network"]["ipv6"]["public_address"],
        "2001:db8:*"
    );
    assert_eq!(masked["runs"][0]["output"]["ipv4"]["address"], "203.0.*.*");
    assert!(masked["runs"][0]["output"]["ipv4"]["hostname"].is_null());
    assert_eq!(masked["runs"][0]["output"]["ipv6"]["address"], "2001:db8:*");
    assert!(masked["runs"][0]["output"]["ipv6"]["hostname"].is_null());
    let hops = &masked["runs"][1]["output"]["ipv4"][0]["route"]["Traced"]["hops"];
    assert_eq!(hops[0]["answers"][0]["address"], "203.0.*.*");
    assert_eq!(
        hops[1]["answers"][0]["address"], "198.51.100.1",
        "third-party router hops remain unmasked"
    );
    assert_eq!(masked["summary"]["completed"], 2);

    let full_page = Page {
        show_ip: true,
        ..masked_page
    };
    let full: serde_json::Value =
        serde_json::from_str(&report::render(&full_page, Format::Json, None)).expect("valid JSON");
    assert_eq!(full["show_ip"], true);
    assert_eq!(
        full["system"]["network"]["ipv4"]["public_address"],
        "203.0.113.7"
    );
    assert_eq!(full["runs"][0]["output"]["ipv4"]["address"], "203.0.113.7");
    assert_eq!(
        full["runs"][0]["output"]["ipv4"]["hostname"],
        "host-203-0-113-7.example.net"
    );
    assert_eq!(
        full["runs"][1]["output"]["ipv4"][0]["route"]["Traced"]["hops"][0]["answers"][0]["address"],
        "203.0.113.7"
    );
}

#[test]
fn bgp_renders_terminal_diagram_mermaid_and_json_svg_with_ip_masking() {
    let mut asn_names = BTreeMap::new();
    asn_names.insert(64500, "Example Cloud".to_owned());
    asn_names.insert(4809, "ChinaNet CN2".to_owned());
    asn_names.insert(9929, "CUII 9929".to_owned());
    asn_names.insert(3356, "Lumen".to_owned());
    asn_names.insert(1299, "Arelion".to_owned());

    let v4_bgp = FamilyBgp {
        address: "203.0.113.7".parse().expect("ipv4"),
        announced: true,
        prefix: Some("203.0.113.0/24".to_owned()),
        prefix_base: Some("203.0.113.0".parse().expect("prefix base")),
        prefix_len: Some(24),
        origin_asn: Some(64500),
        origins: vec![OriginSummary {
            asn: 64500,
            peers: 4,
        }],
        rpki: RpkiStatus::Valid,
        total_peers: 4,
        distinct_paths: 2,
        upstreams: vec![
            UpstreamShare {
                asn: 4809,
                peers: 3,
            },
            UpstreamShare {
                asn: 9929,
                peers: 1,
            },
        ],
        paths: vec![
            BgpPath {
                asns: vec![3356, 4809, 64500],
                asns_raw: vec![3356, 4809, 64500],
                peers: 3,
                is_best: true,
            },
            BgpPath {
                asns: vec![1299, 9929, 64500],
                asns_raw: vec![1299, 9929, 9929, 64500],
                peers: 1,
                is_best: false,
            },
        ],
        asn_names,
    };

    let bgp_run = run(
        "network.bgp",
        Status::Completed,
        Some(1.2),
        Some(Measured::NetworkBgp(Report {
            output: Some(bgp::Output {
                settings: bgp::Settings::default(),
                ipv4_reachable: true,
                ipv6_reachable: false,
                ipv4: Some(Box::new(v4_bgp)),
                ipv6: None,
                notes: Vec::new(),
            }),
            stop: None,
        })),
    );

    let host_profile = host();
    let runs = [bgp_run];
    let masked_page = Page {
        version: "0.1.0",
        host: &host_profile,
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_millis(1200),
    };

    // 1. Wide terminal text: 2D rounded-box diagram + masked prefix, zero raw IP leak.
    let text = report::render(&masked_page, Format::Text, None);
    assert!(text.contains("╭──────────────╮"), "draws rounded ASN boxes");
    assert!(text.contains("═"), "highlights main path with double wire");
    assert!(text.contains("×2"), "marks collapsed prepend span");
    assert!(text.contains("AS3356 ★"), "marks Tier-1 backbone");
    assert!(text.contains("203.0.*.*/24"), "masks announced prefix");
    assert!(!text.contains("203.0.113."), "leaks no raw IPv4 octets");

    // Medium-narrow TTY wraps 2D boxes into bridged bands; ultra-narrow TTY falls back to tree.
    let banded = report::render(&masked_page, Format::Text, Some(48));
    assert!(banded.contains("1 (cont.)"));
    let narrow = report::render(&masked_page, Format::Text, Some(36));
    assert!(narrow.contains("AS64500 Example Cloud (origin)"));
    assert!(narrow.contains("══▸ AS4809 ChinaNet CN2"));
    assert!(!narrow.contains("╭──────────────╮"));

    // 2. Markdown: native Mermaid `flowchart LR` block + escaped `\*` masked prefix.
    let md = report::render(&masked_page, Format::Markdown, None);
    assert!(md.contains("```mermaid\nflowchart LR\n"));
    assert!(md.contains("==>"));
    assert!(md.contains("-. \"×2\" .->"));
    assert!(md.contains("203.0.\\*.\\*/24"));
    assert!(!md.contains("203.0.113."));

    // 3. JSON: standalone inline-styled SVG string + masked address/prefix, unmasked with show_ip.
    let json_raw = report::render(&masked_page, Format::Json, None);
    assert!(!json_raw.contains("203.0.113."));
    let json: serde_json::Value = serde_json::from_str(&json_raw).expect("valid JSON");
    assert_eq!(json["runs"][0]["output"]["ipv4"]["address"], "203.0.*.*");
    assert_eq!(json["runs"][0]["output"]["ipv4"]["prefix"], "203.0.*.*/24");
    let svg = json["runs"][0]["output"]["ipv4"]["svg"]
        .as_str()
        .expect("JSON includes SVG string");
    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg"));
    assert!(svg.contains("<path d=\"M"));
    assert!(svg.contains("AS3356 &#x2605;"));
    assert!(svg.contains("&#xD7;2"));

    let full_page = Page {
        show_ip: true,
        ..masked_page
    };
    let full_json: serde_json::Value =
        serde_json::from_str(&report::render(&full_page, Format::Json, None)).expect("valid JSON");
    assert_eq!(
        full_json["runs"][0]["output"]["ipv4"]["prefix"],
        "203.0.113.0/24"
    );
}
