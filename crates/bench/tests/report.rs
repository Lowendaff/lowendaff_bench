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
use plugin_cpu::bench as cpu_bench;
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
            "cpu.bench",
            Status::Completed,
            Some(41.2),
            Some(Measured::cpu_bench(Report {
                output: Some(cpu_bench::Output {
                    sysbench: Some(cpu_bench::SuiteOutcome::Done {
                        output: sysbench::Output {
                            version: Some("1.0.20".to_owned()),
                            single: sysbench_run(1, 859.0),
                            multi: Some(sysbench_run(64, 44631.33)),
                        },
                    }),
                    ..cpu_bench::Output::default()
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
        run("ip.media", Status::Cancelled, None, None),
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
            "cpu.bench",
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
        md.contains("| IPv4 | available; public address 203.0.\\*.\\* (from Cloudflare) |\n"),
        "key-value fields render as a 2-column table and masked IPv4 asterisks stay escaped: {md}"
    );
    assert!(
        !md.contains("\n- **"),
        "lowendaff.com review-markdown forbids bullet lists: {md}"
    );
    assert!(
        !md.contains("done in "),
        "no trailing elapsed debug lines: {md}"
    );
    assert!(
        !md.contains("\n## Summary\n"),
        "no trailing Summary section: {md}"
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
            "\n## CPU benchmark\n\nsysbench 1.0.20: prime limit 10000, \
             20 s each\n\n| Test | Threads | Events/s |\n| --- | ---: | ---: |\n\
             | Single thread | 1 | 859.00 |\n"
        ),
        "numeric columns align right under per-item h2 section: {md}"
    );
    assert!(
        !md.contains("\n#### 4K and 64K\n"),
        "no fio result in this page: {md}"
    );
    assert!(
        !md.contains("\n##### "),
        "heading depth never exceeds h4: {md}"
    );

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
        single_md.contains("\n## Disk performance\n\nfio 3.43\n")
            && single_md.contains("\n### 4K and 64K\n"),
        "item section uses h2 and table subheadings use h3 without skipping levels: {single_md}"
    );

    // VM CPU %si / %st 5-second telemetry renders on VMs and is omitted on dedicated servers.
    let vm_samples = vec![
        bench::vm_cpu::VmCpuSample {
            elapsed: Duration::from_secs(5),
            item: Some("disk.fio"),
            si_pct: 0.35,
            st_pct: 0.10,
        },
        bench::vm_cpu::VmCpuSample {
            elapsed: Duration::from_secs(10),
            item: Some("cpu.bench"),
            si_pct: 0.80,
            st_pct: 3.25,
        },
    ];
    let vm_text = report::render_with_vm_cpu(&single_page, &vm_samples, Format::Text, None);
    assert!(vm_text.contains("\n[CPU steal & softirq]\n"), "{vm_text}");
    assert!(
        vm_text.contains("00:05  Disk performance  0.35%  0.10%"),
        "{vm_text}"
    );
    assert!(
        vm_text.contains("00:10  CPU benchmark     0.80%  3.25%"),
        "{vm_text}"
    );

    let vm_md = report::render_with_vm_cpu(&single_page, &vm_samples, Format::Markdown, None);
    assert!(vm_md.contains("\n## CPU steal & softirq\n"), "{vm_md}");
    assert!(vm_md.contains("\n### Summary\n"), "{vm_md}");
    assert!(
        vm_md.contains("\n### Timeline\n\n```mermaid\nxychart-beta\n"),
        "{vm_md}"
    );
    assert!(
        vm_md.contains("    x-axis [\"00:05\", \"00:10\"]\n"),
        "{vm_md}"
    );
    assert!(
        vm_md.contains("    line \"Steal (%st)\" [0.10, 3.25]\n"),
        "{vm_md}"
    );
    assert!(
        vm_md.contains("    line \"SoftIRQ (%si)\" [0.35, 0.80]\n"),
        "{vm_md}"
    );
    assert!(
        !vm_md.contains("\n### Samples\n"),
        "Markdown mode replaces the long sample table with Summary + Mermaid chart: {vm_md}"
    );

    let vm_json: serde_json::Value = serde_json::from_str(&report::render_with_vm_cpu(
        &single_page,
        &vm_samples,
        Format::Json,
        None,
    ))
    .expect("valid VM JSON");
    assert_eq!(vm_json["vm_cpu_telemetry"]["interval_seconds"], 5);
    assert_eq!(vm_json["vm_cpu_telemetry"]["sample_count"], 2);
    assert_eq!(
        vm_json["vm_cpu_telemetry"]["st_pct"]["peak_item"],
        "cpu.bench"
    );

    let mut dedicated_host = crate::host();
    dedicated_host.platform = Platform::Dedicated;
    let dedicated_page = Page {
        host: &dedicated_host,
        ..single_page
    };
    let ded_text = report::render_with_vm_cpu(&dedicated_page, &vm_samples, Format::Text, None);
    assert!(!ded_text.contains("CPU steal & softirq"), "{ded_text}");
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
        city: Some("Sydney".to_owned()),
        region: Some("New South Wales".to_owned()),
        timezone: Some("Australia/Sydney".to_owned()),
        usage: None,
        asn_usage: None,
        company_usage: None,
        dual_isp: None,
        usage_sources: Vec::new(),
        scores: vec![(
            "Scamalytics",
            plugin_ip::quality::ScoreVerdict::Unexpected(
                "status 500 for 203.0.113.7 / 2001:db8::7".to_owned(),
            ),
        )],
        factors: Vec::new(),
        dnsbl: None,
        mail: Vec::new(),
        notes: vec![("ipinfo.io", "timeout on 2001:db8::7".to_owned())],
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
                    tcp: true,
                    large_packets: 1024,
                },
                unknown: Vec::new(),
                exchanges: ExchangeList::Read(0),
                ipv4: vec![Traced {
                    target: Target::YouTube,
                    host: Some("r1.googlevideo.com".to_owned()),
                    route: Route::Traced(Trace {
                        reached: true,
                        fragmented: None,
                        hops: vec![
                            Hop {
                                answers: vec![Answer {
                                    address: "203.0.113.7".parse().expect("own hop"),
                                    asn: None,
                                    network: None,
                                    exchange: None,
                                    place: None,
                                    latency: 500,
                                }],
                            },
                            Hop {
                                answers: vec![Answer {
                                    address: "198.51.100.1".parse().expect("router hop"),
                                    asn: Some("AS64496".to_owned()),
                                    network: Some("Example".to_owned()),
                                    exchange: None,
                                    place: None,
                                    latency: 1500,
                                }],
                            },
                        ],
                    }),
                    tcp: Some(trace::result::TcpTrace {
                        port: 443,
                        packet_size: 1024,
                        route: Route::Traced(Trace {
                            reached: true,
                            fragmented: None,
                            hops: vec![Hop {
                                answers: vec![Answer {
                                    address: "203.0.113.7".parse().expect("own tcp hop"),
                                    asn: None,
                                    network: None,
                                    exchange: None,
                                    place: None,
                                    latency: 600,
                                }],
                            }],
                        }),
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
    let tcp_hops = &masked["runs"][1]["output"]["ipv4"][0]["tcp"]["route"]["Traced"]["hops"];
    assert_eq!(tcp_hops[0]["answers"][0]["address"], "203.0.*.*");
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
fn bgp_renders_text_imageupload_url_mermaid_and_json_svg_with_ip_masking() {
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
        diagram_url: Some("https://imageupload.app/i/9d0e951d4f615260d67a".to_owned()),
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
                ipv4: Some(Box::new(v4_bgp.clone())),
                ipv6: None,
                notes: Vec::new(),
                upload_notes: vec![(true, "HTTP status 429".to_owned())],
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

    // 1. Terminal text: hosted imageupload.app link + observed AS_PATH table + upload note,
    //    no Unicode box diagram, and zero raw IP leak.
    let text = report::render(&masked_page, Format::Text, Some(80));
    assert!(
        text.contains("  IPv4 topology       https://imageupload.app/i/9d0e951d4f615260d67a\n"),
        "prints full clickable imageupload.app link without mid-URL wrapping:\n{text}"
    );
    assert!(!text.contains("╭"), "retires Unicode box-and-wire diagram");
    assert!(text.contains("AS3356 ★ (Lumen)"), "keeps AS_PATH table");
    assert!(text.contains("×2"), "marks collapsed prepend span in table");
    assert!(
        text.contains("IPv6 topology image upload failed: HTTP status 429"),
        "surfaces non-fatal upload note:\n{text}"
    );
    assert!(text.contains("203.0.*.*/24"), "masks announced prefix");
    assert!(!text.contains("203.0.113."), "leaks no raw IPv4 octets");

    // When `--no-bgp-upload` disables upload, method line notes it and no topology link prints.
    let no_upload_run = run(
        "network.bgp",
        Status::Completed,
        Some(1.0),
        Some(Measured::NetworkBgp(Report {
            output: Some(bgp::Output {
                settings: bgp::Settings {
                    upload: false,
                    ..bgp::Settings::default()
                },
                ipv4_reachable: true,
                ipv6_reachable: false,
                ipv4: Some(Box::new(FamilyBgp {
                    diagram_url: None,
                    ..v4_bgp
                })),
                ipv6: None,
                notes: Vec::new(),
                upload_notes: Vec::new(),
            }),
            stop: None,
        })),
    );
    let no_upload_runs = [no_upload_run];
    let no_upload_text = report::render(
        &Page {
            runs: &no_upload_runs,
            ..masked_page
        },
        Format::Text,
        None,
    );
    assert!(no_upload_text.contains("; without topology image upload"));
    assert!(!no_upload_text.contains("IPv4 topology"));

    // 2. Markdown: native Mermaid `flowchart LR` block (no imageupload.app link or upload note).
    let md = report::render(&masked_page, Format::Markdown, None);
    assert!(md.contains("```mermaid\nflowchart LR\n"));
    assert!(md.contains("==>"));
    assert!(md.contains("-. \"×2\" .->"));
    assert!(!md.contains("imageupload.app"));
    assert!(md.contains("203.0.\\*.\\*/24"));
    assert!(!md.contains("203.0.113."));

    // 3. JSON: standalone inline-styled SVG string (no `diagram_url` or `upload_notes` field).
    let json_raw = report::render(&masked_page, Format::Json, None);
    assert!(!json_raw.contains("203.0.113."));
    assert!(!json_raw.contains("imageupload.app"));
    let json: serde_json::Value = serde_json::from_str(&json_raw).expect("valid JSON");
    assert_eq!(json["runs"][0]["output"]["ipv4"]["address"], "203.0.*.*");
    assert_eq!(json["runs"][0]["output"]["ipv4"]["prefix"], "203.0.*.*/24");
    assert!(json["runs"][0]["output"]["ipv4"]["diagram_url"].is_null());
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

    // A BGP report constructed without `prefix_base` still masks `prefix` when `!show_ip`.
    let raw_only_run = run(
        "network.bgp",
        Status::Completed,
        Some(1.0),
        Some(Measured::NetworkBgp(Report {
            output: Some(bgp::Output {
                settings: bgp::Settings {
                    upload: false,
                    ..bgp::Settings::default()
                },
                ipv4_reachable: true,
                ipv6_reachable: false,
                ipv4: Some(Box::new(FamilyBgp {
                    address: "203.0.113.7".parse().expect("ipv4"),
                    announced: true,
                    prefix: Some("203.0.113.0/24".to_owned()),
                    prefix_base: None,
                    prefix_len: None,
                    origin_asn: None,
                    origins: Vec::new(),
                    rpki: RpkiStatus::Unknown,
                    total_peers: 0,
                    distinct_paths: 0,
                    upstreams: Vec::new(),
                    paths: Vec::new(),
                    asn_names: BTreeMap::new(),
                    diagram_url: None,
                })),
                ipv6: None,
                notes: Vec::new(),
                upload_notes: Vec::new(),
            }),
            stop: None,
        })),
    );
    let raw_only_runs = [raw_only_run];
    let raw_only_json: serde_json::Value = serde_json::from_str(&report::render(
        &Page {
            runs: &raw_only_runs,
            ..masked_page
        },
        Format::Json,
        None,
    ))
    .expect("valid JSON");
    assert_eq!(
        raw_only_json["runs"][0]["output"]["ipv4"]["prefix"],
        "203.0.*.*/24"
    );
}

#[test]
fn taier_renders_text_markdown_json_fallbacks_and_stop_reasons() {
    use plugin_network::taier;
    use plugin_network::taier::NodeMeasured;
    use plugin_network::taier::NodeOutcome;
    use plugin_network::taier::Operator;
    use plugin_network::taier::Settings;
    use plugin_network::taier::TestedNode;

    let output = taier::Output {
        settings: Settings {
            seconds: 4,
            streams: 8,
            selections: Vec::new(),
            cities: vec!["Beijing".to_owned(), "Guangzhou".to_owned()],
            operators: vec![Operator::Telecom, Operator::Unicom],
            custom_list: false,
        },
        unknown: vec!["Atlantis".to_owned()],
        results: vec![
            TestedNode {
                id: "beijing".to_owned(),
                province: "Beijing".to_owned(),
                city: "Beijing".to_owned(),
                operator: Operator::Telecom,
                ip: "192.0.2.10".parse().expect("doc ip"),
                port: 65499,
                tried_nodes: 2,
                outcome: NodeOutcome::Measured(NodeMeasured {
                    download_bps: 945_000_000,
                    upload_bps: 312_000_000,
                }),
            },
            TestedNode {
                id: "guangdong-guangzhou".to_owned(),
                province: "Guangdong".to_owned(),
                city: "Guangzhou".to_owned(),
                operator: Operator::Unicom,
                ip: "198.51.100.20".parse().expect("doc ip"),
                port: 9443,
                tried_nodes: 1,
                outcome: NodeOutcome::Busy,
            },
            TestedNode {
                id: "guangdong-guangzhou".to_owned(),
                province: "Guangdong".to_owned(),
                city: "Guangzhou".to_owned(),
                operator: Operator::Telecom,
                ip: "198.51.100.21".parse().expect("doc ip"),
                port: 65499,
                tried_nodes: 1,
                outcome: NodeOutcome::Failed("download closed before HTTP header".to_owned()),
            },
        ],
    };

    let taier_run = run(
        "network.taier",
        Status::Completed,
        Some(9.4),
        Some(Measured::NetworkTaier(Report {
            output: Some(output),
            stop: None,
        })),
    );
    let skipped_run = run(
        "network.taier",
        Status::Skipped(None),
        Some(0.1),
        Some(Measured::NetworkTaier(Report {
            output: None,
            stop: Some(Stop::Skipped(taier::Failure::NoNode(vec![
                "Mars".to_owned(),
            ]))),
        })),
    );

    let host_profile = host();
    let runs = [taier_run];
    let page = Page {
        version: "0.1.0",
        host: &host_profile,
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_millis(9400),
    };

    let text = report::render(&page, Format::Text, None);
    assert!(
        text.contains(
            "CAICT GlobalSpeed (Taier): 8 streams, 4 s each way, HTTP/1.1; \
             cities Beijing, Guangzhou; carriers telecom, unicom"
        ),
        "{text}"
    );
    assert!(
        text.contains("no location in the Taier list for Atlantis"),
        "{text}"
    );
    assert!(text.contains("945 Mbit/s"), "{text}");
    assert!(text.contains("312 Mbit/s"), "{text}");
    assert!(text.contains("Guangzhou, Guangdong"), "{text}");
    assert!(text.contains("busy"), "{text}");
    assert!(
        text.contains("Beijing (Telecom): connected on candidate 2 after busy/unreachable nodes"),
        "{text}"
    );
    assert!(
        text.contains("Guangzhou, Guangdong (Telecom): failed: download closed before HTTP header"),
        "{text}"
    );

    let md = report::render(&page, Format::Markdown, None);
    assert!(
        md.contains("| Location | Carrier | Download | Upload |"),
        "{md}"
    );

    let json: serde_json::Value =
        serde_json::from_str(&report::render(&page, Format::Json, None)).expect("valid JSON");
    assert_eq!(json["runs"][0]["id"], "network.taier");
    assert_eq!(json["runs"][0]["title"], "China carrier speed");
    assert_eq!(
        json["runs"][0]["output"]["results"][0]["outcome"]["data"]["download_bps"],
        945_000_000
    );

    let skip_runs = [skipped_run];
    let skip_text = report::render(
        &Page {
            runs: &skip_runs,
            ..page
        },
        Format::Text,
        None,
    );
    assert!(
        skip_text.contains("skipped: no carrier node in the Taier list for Mars"),
        "{skip_text}"
    );

    let selector_output = taier::Output {
        settings: Settings {
            seconds: 4,
            streams: 8,
            selections: vec![
                "beijing:cu".to_owned(),
                "guangdong/shenzhen:ct+cm".to_owned(),
            ],
            cities: Vec::new(),
            operators: vec![Operator::Telecom, Operator::Unicom, Operator::Mobile],
            custom_list: false,
        },
        unknown: Vec::new(),
        results: Vec::new(),
    };
    let sel_runs = [run(
        "network.taier",
        Status::Completed,
        Some(4.2),
        Some(Measured::NetworkTaier(Report {
            output: Some(selector_output),
            stop: None,
        })),
    )];
    let sel_text = report::render(
        &Page {
            runs: &sel_runs,
            ..page
        },
        Format::Text,
        None,
    );
    assert!(
        sel_text.contains("targets beijing:cu, guangdong/shenzhen:ct+cm"),
        "{sel_text}"
    );
}

#[test]
fn cloudflare_and_inetspeed_render_text_markdown_json_and_mask_client_ip() {
    use plugin_network::cloudflare;
    use plugin_network::inetspeed;

    let cf_fam = cloudflare::FamilyCloudflare {
        client_ip: Some("198.51.100.77".parse().expect("doc ip")),
        colo: Some("LAX".to_owned()),
        server: Some("LAX - Los Angeles, United States".to_owned()),
        asn: Some(64500),
        as_org: Some("ExampleISP".to_owned()),
        idle_latency: cloudflare::LatencyStats {
            sent: 8,
            received: 8,
            loss_pct: 0.0,
            min_ms: Some(11.2),
            mean_ms: Some(12.0),
            median_ms: Some(11.8),
            p25_ms: Some(11.5),
            p75_ms: Some(12.4),
            p95_ms: Some(13.1),
            p99_ms: Some(13.4),
            max_ms: Some(13.5),
            jitter_ms: Some(0.85),
        },
        download: cloudflare::ThroughputStats {
            bytes: 950_000_000,
            duration_ms: 8_000,
            bps: 950_000_000,
            mbps: 950.0,
            mean_mbps: 950.0,
            median_mbps: 955.0,
            p25_mbps: 920.0,
            p75_mbps: 970.0,
            p95_mbps: 985.0,
            p99_mbps: 990.0,
        },
        upload: cloudflare::ThroughputStats {
            bytes: 420_000_000,
            duration_ms: 8_000,
            bps: 420_000_000,
            mbps: 420.0,
            mean_mbps: 420.0,
            median_mbps: 422.0,
            p25_mbps: 405.0,
            p75_mbps: 430.0,
            p95_mbps: 440.0,
            p99_mbps: 445.0,
        },
        loaded_download_latency: cloudflare::LatencyStats {
            sent: 16,
            received: 16,
            loss_pct: 0.0,
            min_ms: Some(18.0),
            mean_ms: Some(22.0),
            median_ms: Some(21.5),
            max_ms: Some(28.0),
            jitter_ms: Some(2.1),
            ..cloudflare::LatencyStats::default()
        },
        loaded_upload_latency: cloudflare::LatencyStats {
            sent: 16,
            received: 16,
            loss_pct: 0.0,
            min_ms: Some(20.0),
            mean_ms: Some(25.0),
            median_ms: Some(24.8),
            max_ms: Some(32.0),
            jitter_ms: Some(2.6),
            ..cloudflare::LatencyStats::default()
        },
        udp: Some(cloudflare::UdpLossStats {
            target: "stun:stun.example:3478".to_owned(),
            sent: 50,
            received: 50,
            loss_pct: 0.0,
            out_of_order: 0,
            out_of_order_pct: 0.0,
            median_ms: Some(11.9),
            jitter_ms: Some(0.9),
            mos: Some(4.40),
            quality: cloudflare::UdpQuality::Excellent,
        }),
        udp_note: None,
        quality: Some(cloudflare::QualityStats {
            bufferbloat_grade: cloudflare::BufferbloatGrade::A,
            bufferbloat_ms: Some(13.0),
            stability_grade: cloudflare::StabilityGrade::A,
            stability_cv_pct: Some(2.4),
        }),
    };

    let cf_v6 = cloudflare::FamilyCloudflare {
        client_ip: Some("2001:db8::77".parse().expect("doc ipv6")),
        udp: None,
        udp_note: Some(cloudflare::FamilyIssue::TimedOut),
        ..cf_fam.clone()
    };

    let cf_run = run(
        "network.cloudflare",
        Status::Completed,
        Some(24.0),
        Some(Measured::NetworkCloudflare(Report {
            output: Some(cloudflare::Output {
                settings: cloudflare::Settings::default(),
                ipv4: Some(Box::new(cf_fam)),
                ipv6: Some(Box::new(cf_v6)),
                notes: vec![(true, cloudflare::FamilyIssue::NoAddress)],
            }),
            stop: None,
        })),
    );

    let inet_run = run(
        "network.inetspeed",
        Status::Completed,
        Some(18.5),
        Some(Measured::NetworkInetspeed(Report {
            output: Some(inetspeed::Output {
                settings: inetspeed::Settings::default(),
                selected: inetspeed::SelectedEndpoint {
                    host: "cdn.example".to_owned(),
                    ip: Some("192.0.2.55".parse().expect("doc ip")),
                    source: inetspeed::EndpointSource::Doh,
                    rtt_ms: Some(8.42),
                },
                candidates: Vec::new(),
                idle_latency: Some(inetspeed::LatencyStats {
                    samples: 20,
                    min_ms: 7.90,
                    avg_ms: 8.55,
                    median_ms: 8.40,
                    max_ms: 10.10,
                    jitter_ms: 0.35,
                }),
                rounds: vec![
                    inetspeed::RoundResult {
                        direction: inetspeed::TransferDirection::Download,
                        threads: 1,
                        bytes: 500_000_000,
                        duration_ms: 5_000,
                        bps: 800_000_000,
                        mbps: 800.0,
                        fault_count: 0,
                        loaded_latency: Some(inetspeed::LatencyStats {
                            samples: 12,
                            min_ms: 10.0,
                            avg_ms: 12.2,
                            median_ms: 12.0,
                            max_ms: 15.0,
                            jitter_ms: 0.80,
                        }),
                        error: None,
                    },
                    inetspeed::RoundResult {
                        direction: inetspeed::TransferDirection::Upload,
                        threads: 4,
                        bytes: 300_000_000,
                        duration_ms: 5_000,
                        bps: 480_000_000,
                        mbps: 480.0,
                        fault_count: 0,
                        loaded_latency: None,
                        error: None,
                    },
                    inetspeed::RoundResult {
                        direction: inetspeed::TransferDirection::Upload,
                        threads: 1,
                        bytes: 0,
                        duration_ms: 5_000,
                        bps: 0,
                        mbps: 0.0,
                        fault_count: 1,
                        loaded_latency: None,
                        error: Some(inetspeed::RoundIssue::WorkerFaults(1)),
                    },
                ],
                total_bytes: 800_000_000,
                warnings: vec![inetspeed::Warning::DohEmpty],
            }),
            stop: None,
        })),
    );

    let cf_failed = run(
        "network.cloudflare",
        Status::Failed(None),
        Some(2.0),
        Some(Measured::NetworkCloudflare(Report {
            output: None,
            stop: Some(bench_api::Stop::Failed(cloudflare::Failure::NoResult(
                Some(cloudflare::FamilyIssue::TimedOut),
            ))),
        })),
    );
    let inet_failed = run(
        "network.inetspeed",
        Status::Failed(None),
        Some(2.0),
        Some(Measured::NetworkInetspeed(Report {
            output: None,
            stop: Some(bench_api::Stop::Failed(inetspeed::Failure::NoResult)),
        })),
    );

    let host_profile = host();
    let runs = [cf_run, inet_run, cf_failed, inet_failed];
    let masked_page = Page {
        version: "0.1.0",
        host: &host_profile,
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_millis(42_500),
    };

    let text = report::render(&masked_page, Format::Text, None);
    assert!(
        text.contains("Cloudflare Speed: 6 workers, 10 s each way"),
        "{text}"
    );
    assert!(text.contains("LAX - Los Angeles, United States"), "{text}");
    assert!(
        text.contains("198.51.*.*") && text.contains("2001:db8:*"),
        "masks Cloudflare v4 and v6 client IPs:\n{text}"
    );
    assert!(
        !text.contains("198.51.100.77") && !text.contains("2001:db8::77"),
        "leaks no raw client IP:\n{text}"
    );
    assert!(
        text.contains("bufferbloat A (+13.0 ms), stability A (CV 2.4%)"),
        "{text}"
    );
    assert!(
        text.contains("Apple CDN (iNetSpeed): 1 & 4 threads, 2G / 10 s cap"),
        "{text}"
    );
    assert!(
        text.contains("192.0.2.55 (cdn.example, 8.42 ms via Dual DoH)"),
        "{text}"
    );
    assert!(text.contains("Download (single)"), "{text}");
    assert!(text.contains("Upload (multi)"), "{text}");
    assert!(text.contains("Upload (single)"), "{text}");
    assert!(text.contains("error"), "{text}");

    let md = report::render(&masked_page, Format::Markdown, None);
    assert!(md.contains("| Metric | IPv4 | IPv6 |"), "{md}");
    assert!(
        md.contains("| Round | Threads | Speed | Data | Loaded latency |"),
        "{md}"
    );
    assert!(
        !md.contains("198.51.100.77") && !md.contains("2001:db8::77"),
        "{md}"
    );
    assert!(
        md.contains("198.51.\\*.\\*") && md.contains("2001:db8:*"),
        "{md}"
    );

    let json_raw = report::render(&masked_page, Format::Json, None);
    assert!(
        !json_raw.contains("198.51.100.77") && !json_raw.contains("2001:db8::77"),
        "{json_raw}"
    );
    let json: serde_json::Value = serde_json::from_str(&json_raw).expect("valid JSON");
    assert_eq!(json["runs"][0]["output"]["ipv4"]["client_ip"], "198.51.*.*");
    assert_eq!(json["runs"][0]["output"]["ipv6"]["client_ip"], "2001:db8:*");
    assert_eq!(json["runs"][1]["id"], "network.inetspeed");
    assert_eq!(json["runs"][1]["title"], "Apple CDN speed");
    assert_eq!(json["runs"][2]["status"], "failed");
    assert!(json["runs"][2]["reason"].as_str().is_some());
    assert_eq!(json["runs"][3]["status"], "failed");
    assert!(json["runs"][3]["reason"].as_str().is_some());

    let unmasked_json: serde_json::Value = serde_json::from_str(&report::render(
        &Page {
            show_ip: true,
            ..masked_page
        },
        Format::Json,
        None,
    ))
    .expect("valid JSON");
    assert_eq!(
        unmasked_json["runs"][0]["output"]["ipv4"]["client_ip"],
        "198.51.100.77"
    );
    assert_eq!(
        unmasked_json["runs"][0]["output"]["ipv6"]["client_ip"],
        "2001:db8::77"
    );
}

#[test]
fn ip_quality_notes_redact_own_and_prefix_overlapping_ips_across_all_formats() {
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
    let leak_detail = concat!(
        "err 203%2E0%2E113%2E7 / 203.0.113.79 / ",
        "2001%3A0db8%3A0000%3A0000%3A0000%3A0000%3A0000%3A0007"
    );
    let v4_rep = FamilyReport {
        address: "203.0.113.79".parse().expect("secondary egress"),
        asn: Some(13335),
        org: Some("Cloudflare".to_owned()),
        hostname: None,
        rir: Some("APNIC".to_owned()),
        registered_country: Some("AU".to_owned()),
        geo_country: Some("AU".to_owned()),
        geo_votes: Vec::new(),
        geo_total: 0,
        origin: Origin::Native {
            country: "AU".to_owned(),
        },
        city: Some("Sydney".to_owned()),
        region: Some("New South Wales".to_owned()),
        timezone: Some("Australia/Sydney".to_owned()),
        usage: None,
        asn_usage: None,
        company_usage: None,
        dual_isp: None,
        usage_sources: Vec::new(),
        scores: vec![(
            "Scamalytics",
            plugin_ip::quality::ScoreVerdict::Unexpected(leak_detail.to_owned()),
        )],
        factors: Vec::new(),
        dnsbl: None,
        mail: Vec::new(),
        notes: vec![("ipinfo.io", leak_detail.to_owned())],
    };
    let runs = vec![run(
        "ip.quality",
        Status::Completed,
        Some(1.0),
        Some(Measured::IpQuality(Report {
            output: Some(quality::Output {
                settings: quality::Settings::default(),
                ipv4_reachable: true,
                ipv6_reachable: true,
                ipv4: Some(Box::new(v4_rep)),
                ipv6: None,
            }),
            stop: None,
        })),
    )];
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs: &runs,
        total: Duration::from_secs(1),
    };

    for fmt in [Format::Text, Format::Markdown, Format::Json] {
        let rendered = report::render(&page, fmt, None);
        for leaked in [
            "203%2E0%2E113%2E7",
            "203.0.113.79",
            "203.0.*.*9",
            "203.0.\\*.\\*9",
            "2001%3A0db8",
            "0007",
        ] {
            assert!(
                !rendered.contains(leaked),
                "{fmt:?} leaked {leaked:?}:\n{rendered}"
            );
        }
    }
}

#[test]
fn multilingual_text_and_markdown_localize_while_json_stays_strictly_english() {
    use bench::locale;
    use bench::locale::Language;
    use bench::vm_cpu::VmCpuSample;

    let host = host();
    let runs = vec![
        run(
            "cpu.bench",
            Status::Completed,
            Some(2.0),
            Some(Measured::cpu_bench(Report {
                output: Some(cpu_bench::Output {
                    sysbench: Some(cpu_bench::SuiteOutcome::Done {
                        output: sysbench::Output {
                            version: Some("1.0.20".to_owned()),
                            single: sysbench_run(1, 1500.0),
                            multi: Some(sysbench_run(2, 2950.0)),
                        },
                    }),
                    ..cpu_bench::Output::default()
                }),
                stop: None,
            })),
        ),
        run(
            "disk.fio",
            Status::Skipped(Some(Unmet::Network(Net::Ipv6))),
            None,
            None,
        ),
    ];
    let vm_samples = vec![VmCpuSample {
        elapsed: Duration::from_secs(5),
        item: Some("cpu.bench"),
        si_pct: 0.4,
        st_pct: 1.8,
    }];
    let page = Page {
        version: "0.1.2",
        host: &host,
        show_ip: false,
        not_applicable: &["Disk health"],
        runs: &runs,
        total: Duration::from_secs(5),
    };

    let en_text = locale::with_language(Language::En, || {
        report::render_with_vm_cpu(&page, &vm_samples, Format::Text, None)
    });
    let en_json = locale::with_language(Language::En, || {
        report::render_with_vm_cpu(&page, &vm_samples, Format::Json, None)
    });
    assert!(en_json.contains("\"title\": \"CPU benchmark\""));
    assert!(en_json.contains("\"peak_title\": \"CPU benchmark\""));
    assert!(en_json.contains("needs an IPv6 connection"));

    for (lang, expected_sys, expected_item) in [
        (
            Language::ZhCn,
            "\u{7cfb}\u{7edf}\u{4fe1}\u{606f}",
            "CPU \u{7efc}\u{5408}\u{57fa}\u{51c6}",
        ),
        (
            Language::ZhTw,
            "\u{7cfb}\u{7d71}\u{8cc7}\u{8a0a}",
            "CPU \u{7d9c}\u{5408}\u{57fa}\u{6e96}",
        ),
        (
            Language::Ja,
            "\u{30b7}\u{30b9}\u{30c6}\u{30e0}\u{6982}\u{8981}",
            "CPU \u{7d71}\u{5408}\u{30d9}\u{30f3}\u{30c1}\u{30de}\u{30fc}\u{30af}",
        ),
    ] {
        locale::with_language(lang, || {
            let text = report::render_with_vm_cpu(&page, &vm_samples, Format::Text, None);
            let md = report::render_with_vm_cpu(&page, &vm_samples, Format::Markdown, None);
            let json = report::render_with_vm_cpu(&page, &vm_samples, Format::Json, None);

            assert_ne!(text, en_text, "{lang:?} text report must localize");
            assert!(
                text.contains(&format!("[{expected_sys}]")),
                "{lang:?} text missing [{expected_sys}]:\n{text}"
            );
            assert!(
                text.contains(expected_item),
                "{lang:?} text missing {expected_item}:\n{text}"
            );
            assert!(
                md.contains(&format!("## {expected_sys}"))
                    && md.contains(&format!("## {expected_item}")),
                "{lang:?} markdown missing localized headings:\n{md}"
            );
            assert_eq!(
                json, en_json,
                "{lang:?} JSON output must stay byte-for-byte identical to English"
            );
        });
    }
}
