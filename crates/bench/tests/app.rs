//! Command-line flow: argument parsing, selection, config file, and a full run through
//! the terminal renderer with the run directory removed afterwards.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench::app;
use bench::app::Cli;
use bench::app::EXIT_INTERRUPTED;
use bench::orchestrator;
use bench::orchestrator::Planned;
use bench::progress;
use bench::registry;
use bench::registry::Entry;
use bench::results::Measured;
use bench_api::CancellationToken;
use bench_api::Net;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed::Context;
use bench_api::typed::Descriptor;
use bench_api::typed::Plugin;
use bench_api::typed::Stop;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::numa::Layout;
use bench_host::numa::Node;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;
use common::Captured;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Scores;
use serde::Deserialize;

mod common;

/// What the tests' plugins return, Geekbench 7's result being the simplest of the program's to
/// build: empty, so that the report shows only how a plugin ended.
type Output = geekbench::Output;
type Failure = geekbench::Failure;

const fn descriptor(id: &'static str) -> Descriptor {
    Descriptor {
        id,
        requires: Requirements::NONE,
        timeout: Duration::from_secs(10),
    }
}

/// A plugin that stands in for one of the program's and leaves an empty result.
macro_rules! stand_in {
    ($name:ident, $descriptor:expr) => {
        #[derive(Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct $name {}

        impl Plugin for $name {
            type Output = Output;
            type Failure = Failure;
            const DESCRIPTOR: Descriptor = $descriptor;

            async fn run(&self, _ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
                Ok(Output::default())
            }
        }
    };
}

/// Its config names the single-core score that its result shows.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Good {
    samples: u32,
}

impl Plugin for Good {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.good");

    async fn run(&self, _ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        Ok(Output {
            scores: Some(Scores {
                version: None,
                single: self.samples,
                multi: self.samples,
                url: None,
            }),
            ..Output::default()
        })
    }
}

stand_in!(Other, descriptor("test.other"));
// A group of one plugin, whose id is the group's name.
stand_in!(Lone, descriptor("lone"));

/// The registry's entry of a plugin of these tests.
fn entry<P: Plugin<Output = Output, Failure = Failure>>() -> Entry {
    registry::entry::<P>(Measured::CpuGeekbench)
}

fn plugins() -> Vec<Entry> {
    vec![entry::<Good>(), entry::<Other>()]
}

fn groups() -> Vec<Entry> {
    vec![entry::<Good>(), entry::<Lone>(), entry::<Other>()]
}

fn cli(args: &[&str]) -> Cli {
    use clap::Parser;

    Cli::try_parse_from(std::iter::once("lowendaff-bench").chain(args.iter().copied()))
        .expect("test arguments must parse")
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

/// Runs the plan as the program does, and returns the exit code, the report and the progress
/// log, once the run directory is gone. Every run is captured, so that no test logs while
/// another listens (see `common`).
async fn execute(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    cancel: &CancellationToken,
) -> (u8, String, Vec<String>) {
    execute_showing(planned, host, cancel, false).await
}

/// [`execute`], with the public addresses in full in the report when `show_ip` is set. The log
/// starts as the program's does, with the host and its network.
async fn execute_showing(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    cancel: &CancellationToken,
    show_ip: bool,
) -> (u8, String, Vec<String>) {
    let work = tempfile::tempdir().expect("a work directory for the run");
    let mut out = Vec::new();
    let (log, capturing) = Captured::start(true);
    progress::host(&host);
    progress::network(&host.network);
    let code = app::execute(planned, host, work.path(), cancel, &mut out, None, show_ip)
        .await
        .expect("the run starts");
    drop(capturing);
    let left = std::fs::read_dir(work.path())
        .expect("the work directory stays")
        .count();
    assert_eq!(left, 0, "run directory must be removed");
    let report = String::from_utf8(out).expect("the report is UTF-8");
    (code, report, log.lines())
}

#[test]
fn ids_are_comma_separated_and_work_dir_defaults_to_cwd() {
    let cli = cli(&["--only", "test.good,test.other", "--skip", "test.other"]);
    assert_eq!(cli.only, ["test.good", "test.other"]);
    assert_eq!(cli.skip, ["test.other"]);
    assert_eq!(cli.work_dir, PathBuf::from("."));
}

#[test]
fn selection_keeps_registry_order_and_rejects_unknown_ids() {
    assert_eq!(
        app::select(&plugins(), &[], &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(&plugins(), &ids(&["test.other", "test.good"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(&plugins(), &[], &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(&plugins(), &ids(&["nope"]), &ids(&["nada"])).unwrap_err(),
        ["未知测试项或分组 `nope`", "未知测试项或分组 `nada`"]
    );
}

#[test]
fn a_group_name_selects_every_plugin_of_the_group() {
    assert_eq!(
        app::select(&groups(), &ids(&["test"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(&groups(), &[], &ids(&["test"])).unwrap(),
        ["lone"]
    );
    assert_eq!(
        app::select(&groups(), &ids(&["test.other", "lone"]), &[]).unwrap(),
        ["lone", "test.other"]
    );
    assert_eq!(
        app::select(&groups(), &ids(&["test"]), &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(&groups(), &ids(&["tes", "test."]), &ids(&["good", "lon"])).unwrap_err(),
        [
            "未知测试项或分组 `tes`",
            "未知测试项或分组 `test.`",
            "未知测试项或分组 `good`",
            "未知测试项或分组 `lon`",
        ],
        "a name is a whole id or group"
    );
}

#[test]
fn config_file_holds_plugin_tables_only() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        path
    };

    let good = write("good.toml", "[plugins.\"test.good\"]\nsamples = 2\n");
    let expected = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    assert_eq!(app::load_config(Some(&good)).unwrap(), expected);

    let stray = write("stray.toml", "timeout = 3\n");
    assert_eq!(
        app::load_config(Some(&stray)).unwrap_err(),
        "配置文件中有未知的项 `timeout`，只支持 [plugins.\"<id>\"]"
    );

    let broken = write("broken.toml", "[plugins\n");
    assert!(
        app::load_config(Some(&broken))
            .unwrap_err()
            .starts_with("无法解析配置文件")
    );

    let bad_value = write("bad.toml", "[plugins.\"test.good\"]\nsamples = \"many\"\n");
    let config = bad_value.to_string_lossy().into_owned();
    let Err(errors) = app::prepare(&cli(&["--config", &config]), &plugins()) else {
        panic!("bad config must not plan")
    };
    assert!(
        errors[0].starts_with("测试项 test.good 的配置无效："),
        "{errors:?}"
    );
}

#[tokio::test]
async fn a_run_prints_sections_and_summary_then_removes_its_directory() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("bench.toml");
    std::fs::write(&config, "[plugins.\"test.good\"]\nsamples = 3\n").unwrap();
    let planned = app::prepare(
        &cli(&[
            "--skip",
            "test.other",
            "--config",
            &config.to_string_lossy(),
        ]),
        &plugins(),
    )
    .unwrap();

    let (code, text, _) = execute(planned, HostProfile::default(), &CancellationToken::new()).await;

    assert_eq!(code, 0);
    assert!(
        text.starts_with(&format!(
            "lowendaff-bench {}\n运行环境：未检测到虚拟化\n\n【系统信息】\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(text.contains("\n\n【test】\n"), "{text}");
    assert!(
        text.contains("  单核              3\n"),
        "the score that the config gave: {text}"
    );
    assert!(
        text.contains("\n共 1 项：完成 1，跳过 0，失败 0，超时 0，中断 0。总用时 "),
        "{text}"
    );
}

#[tokio::test]
async fn an_interrupted_run_exits_130_and_still_lists_every_plugin() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let planned = app::prepare(&cli(&[]), &plugins()).unwrap();

    let (code, text, _) = execute(planned, HostProfile::default(), &cancel).await;

    assert_eq!(code, EXIT_INTERRUPTED);
    assert!(
        text.contains("【test】\n  test.good\n    已中断\n  test.other\n    已中断\n"),
        "two plugins of one group share a section: {text}"
    );
    assert_eq!(text.matches("【test】").count(), 1, "{text}");
    assert!(
        text.contains("共 2 项：完成 0，跳过 0，失败 0，超时 0，中断 2。"),
        "{text}"
    );
}

stand_in!(
    InfoDedicated,
    Descriptor {
        requires: Requirements {
            platforms: &[PlatformKind::Dedicated],
            ..Requirements::NONE
        },
        ..descriptor("test.info-dedicated")
    }
);
stand_in!(
    InfoVm,
    Descriptor {
        requires: Requirements {
            platforms: &[PlatformKind::Vm],
            ..Requirements::NONE
        },
        ..descriptor("test.info-vm")
    }
);

const SMARTCTL: ToolSpec = ToolSpec {
    name: "smartctl",
    version: "7.4",
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://example.invalid/smartctl-x86_64",
            sha256: "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8",
            size: 24,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://example.invalid/smartctl-aarch64",
            sha256: "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8",
            size: 24,
        },
    ],
};

stand_in!(
    DiskHealth,
    Descriptor {
        requires: Requirements {
            tools: &[SMARTCTL],
            net: Net::None,
            platforms: &[PlatformKind::Dedicated],
            glibc: None,
            numa: false,
        },
        ..descriptor("test.disk-health")
    }
);
// A test between NUMA nodes, which needs two of them.
stand_in!(
    Numa,
    Descriptor {
        requires: Requirements {
            numa: true,
            ..Requirements::NONE
        },
        ..descriptor("test.numa")
    }
);

fn platform_plugins() -> Vec<Entry> {
    vec![
        entry::<InfoDedicated>(),
        entry::<InfoVm>(),
        entry::<DiskHealth>(),
        entry::<Numa>(),
    ]
}

/// A host whose NUMA layout has `count` nodes, each with a CPU and memory.
fn with_nodes(count: usize) -> HostProfile {
    HostProfile {
        platform: Platform::Dedicated,
        numa: Read::Done(Layout {
            nodes: (0..count)
                .map(|id| Node {
                    id,
                    cpus: vec![id],
                    total: Some(1 << 30),
                    free: Some(1 << 29),
                })
                .collect(),
            cpus: Some((0..count).collect()),
            mems: None,
        }),
        ..HostProfile::default()
    }
}

/// The summary's line of the plugin `id`, which a plugin that ran has.
fn summary_line<'a>(text: &'a str, id: &str) -> Option<&'a str> {
    text.lines().find(|line| {
        line.strip_prefix("  ")
            .is_some_and(|rest| rest.starts_with(&format!("{id} ")))
    })
}

#[tokio::test]
async fn a_plugin_that_needs_numa_nodes_runs_only_where_two_can_be_used() {
    let registry = [entry::<Numa>()];
    for (count, runs) in [(1, false), (2, true)] {
        let planned = app::prepare(&cli(&[]), &registry).unwrap();
        let (code, text, _) = execute(planned, with_nodes(count), &CancellationToken::new()).await;
        assert_eq!(code, 0);
        assert_eq!(
            summary_line(&text, "test.numa").is_some(),
            runs,
            "{count} nodes: {text}"
        );
        assert_eq!(
            text.contains("  本机不适用：test.numa\n"),
            !runs,
            "{count} nodes: {text}"
        );
        let total = if runs {
            "共 1 项：完成 1，"
        } else {
            "共 0 项：完成 0，"
        };
        assert!(
            text.contains(total),
            "{count} nodes, not counted elsewhere: {text}"
        );
    }
}

#[tokio::test]
async fn only_plugins_for_the_detected_platform_run() {
    let planned = app::prepare(&cli(&[]), &platform_plugins()).unwrap();
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        ..HostProfile::default()
    };

    let (code, text, log) = execute(planned, host, &CancellationToken::new()).await;

    assert_eq!(code, 0);
    assert!(
        text.starts_with(&format!(
            "lowendaff-bench {}\n运行环境：KVM 虚拟机\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(text.contains("\n\n【test】\n"), "{text}");
    assert!(
        text.contains("  本机不适用：test.info-dedicated、test.disk-health、test.numa\n"),
        "{text}"
    );
    assert!(
        summary_line(&text, "test.info-vm").is_some_and(|line| line.contains("完成")),
        "{text}"
    );
    assert!(
        !log.iter().any(|line| line.starts_with("准备工具")),
        "tools of plugins that do not apply must not be downloaded: {log:?}"
    );
    assert!(
        text.contains("\n共 1 项：完成 1，跳过 0，失败 0，超时 0，中断 0。"),
        "{text}"
    );
}

stand_in!(
    NeedsSmartctl,
    Descriptor {
        requires: Requirements {
            tools: &[SMARTCTL],
            ..Requirements::NONE
        },
        ..descriptor("test.smartctl")
    }
);

#[tokio::test]
async fn a_tool_that_cannot_be_provisioned_skips_its_plugins_with_the_reason() {
    let registry = [entry::<NeedsSmartctl>()];
    let planned = app::prepare(&cli(&[]), &registry).unwrap();
    // No download is tried: the tool has no build for this architecture.
    let host = HostProfile {
        arch: "riscv64".to_owned(),
        ..HostProfile::default()
    };

    let (code, text, log) = execute(planned, host, &CancellationToken::new()).await;

    assert_eq!(code, 0);
    let reason = "工具 smartctl 不可用：没有适用于 riscv64 的版本";
    assert!(text.contains(&format!("\n  跳过：{reason}\n")), "{text}");
    assert_eq!(
        log,
        [
            "主机概况：未检测到虚拟化，riscv64".to_owned(),
            "网络：IPv4 不可用".to_owned(),
            "网络：IPv6 不可用".to_owned(),
            "准备工具：smartctl 7.4 失败：没有适用于 riscv64 的版本".to_owned(),
            format!("test.smartctl 跳过：{reason}"),
        ]
    );
}

const GEEKBENCH: ToolSpec = ToolSpec {
    name: "geekbench",
    version: "7.1.0",
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    assets: SMARTCTL.assets,
};

stand_in!(
    NeedsGlibcAndIpv4,
    Descriptor {
        requires: Requirements {
            tools: &[GEEKBENCH],
            net: Net::Ipv4,
            platforms: &[],
            glibc: Some(GlibcVersion {
                major: 2,
                minor: 35,
            }),
            numa: false,
        },
        ..descriptor("test.geekbench")
    }
);

/// A network whose IPv4 side reached a target, with no public address asked for.
fn ipv4_only() -> Network {
    Network {
        ipv4: Family {
            targets: vec![("1.1.1.1:443".parse().expect("an address"), Reach::Connected)],
            address: Address::NotAsked,
        },
        ipv6: Family::default(),
    }
}

#[tokio::test]
async fn tools_of_items_the_host_cannot_run_are_not_downloaded() {
    let registry = [entry::<NeedsGlibcAndIpv4>()];
    let without_glibc = HostProfile {
        libc: Libc::NoGlibc,
        network: ipv4_only(),
        ..HostProfile::default()
    };
    let without_ipv4 = HostProfile {
        libc: Libc::GlibcUnknown,
        ..HostProfile::default()
    };
    let cases = [
        (without_glibc, "需要 glibc 2.35 或更新，本机没有 glibc"),
        (without_ipv4, "需要 IPv4 网络连接"),
    ];
    for (host, reason) in cases {
        let planned = app::prepare(&cli(&[]), &registry).unwrap();
        let (code, text, log) = execute(planned, host, &CancellationToken::new()).await;
        assert_eq!(code, 0);
        assert!(
            !log.iter().any(|line| line.starts_with("准备工具")),
            "nothing is downloaded for an item that is skipped anyway: {log:?}"
        );
        assert!(text.contains(&format!("跳过：{reason}")), "{text}");
    }
}

/// What a plugin saw of its host: the platform, the architecture, the C library and whether
/// IPv4 reached the Internet.
type Seen = (Platform, String, Libc, bool);

/// Notes the host that its context hands it.
#[derive(Default, Deserialize)]
struct ShowsHost {
    #[serde(skip)]
    seen: Arc<Mutex<Option<Seen>>>,
}

impl Plugin for ShowsHost {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.host");

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let host = ctx.host();
        let seen = (
            host.platform,
            host.arch.clone(),
            host.libc,
            host.network.ipv4.reachable(),
        );
        *self.seen.lock().expect("nothing panics while holding it") = Some(seen);
        Ok(Output::default())
    }
}

#[tokio::test]
async fn plugins_see_the_host_as_the_run_read_it() {
    let seen = Arc::default();
    let plugin = ShowsHost {
        seen: Arc::clone(&seen),
    };
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        arch: "riscv64".to_owned(),
        libc: Libc::NoGlibc,
        network: ipv4_only(),
        ..HostProfile::default()
    };

    let (code, _, _) = execute(
        vec![Planned::typed(plugin, Measured::CpuGeekbench)],
        host,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(code, 0);
    assert_eq!(
        *seen.lock().unwrap(),
        Some((
            Platform::Vm(Hypervisor::Kvm),
            "riscv64".to_owned(),
            Libc::NoGlibc,
            true
        ))
    );
}

#[tokio::test]
async fn the_hosts_addresses_appear_in_full_only_in_the_report_and_only_on_request() {
    let host = || HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![("1.1.1.1:443".parse().expect("a target"), Reach::Connected)],
                address: Address::Found {
                    address: "203.0.113.7".parse().expect("an address"),
                    source: "Cloudflare",
                },
            },
            ipv6: Family {
                targets: vec![(
                    "[2606:4700:4700::1111]:443".parse().expect("a target"),
                    Reach::Connected,
                )],
                address: Address::Found {
                    address: "2001:db8::7".parse().expect("an address"),
                    source: "ipify",
                },
            },
        },
        ..HostProfile::default()
    };
    let full = ["203.0.113.7", "2001:db8::7"];
    for show_ip in [false, true] {
        let planned = app::prepare(&cli(&[]), &plugins()).unwrap();
        let (_, text, log) =
            execute_showing(planned, host(), &CancellationToken::new(), show_ip).await;
        let log = log.join("\n");
        assert!(
            log.contains("203.0.*.*") && log.contains("2001:db8:*"),
            "{log}"
        );
        for address in full {
            assert!(!log.contains(address), "the log masks {address}: {log}");
            assert_eq!(text.contains(address), show_ip, "{address}: {text}");
        }
    }
}

#[test]
fn the_list_shows_where_each_plugin_applies() {
    let mut out = Vec::new();
    app::list(&platform_plugins(), &mut out);

    assert_eq!(
        String::from_utf8(out).unwrap(),
        "共 4 个测试项\n\
         【test】（分组：test）\n\
         \x20 test.info-dedicated  test.info-dedicated（适用：独立服务器）\n\
         \x20 test.info-vm  test.info-vm（适用：虚拟机）\n\
         \x20 test.disk-health  test.disk-health（适用：独立服务器）（工具：smartctl 7.4）\n\
         \x20 test.numa  test.numa（适用：多个 NUMA 节点）\n"
    );
}

#[test]
fn the_list_keeps_each_group_together_in_the_reports_order() {
    let mut out = Vec::new();
    app::list(&groups(), &mut out);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "共 3 个测试项\n\
         【test】（分组：test）\n\
         \x20 test.good  test.good\n\
         \x20 test.other  test.other\n\
         【lone】（分组：lone）\n\
         \x20 lone  lone\n"
    );

    let mut out = Vec::new();
    app::list(&registry::all(), &mut out);
    let text = String::from_utf8(out).unwrap();
    let headings: Vec<&str> = text.lines().filter(|line| line.starts_with('【')).collect();
    assert_eq!(
        headings,
        [
            "【CPU 性能】（分组：cpu）",
            "【内存性能】（分组：memory）",
            "【硬盘】（分组：disk）"
        ]
    );
    let ids: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(
        ids,
        [
            "cpu.sysbench",
            "cpu.geekbench",
            "memory.sysbench",
            "memory.stream",
            "memory.latency",
            "memory.numa",
            "disk.health",
            "disk.fio"
        ]
    );
    let cpu: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  cpu."))
        .collect();
    assert_eq!(
        cpu,
        [
            "  cpu.sysbench  CPU 性能（工具：sysbench 1.0.20）",
            "  cpu.geekbench  Geekbench 7（工具：geekbench 7.1.0）"
        ],
        "the reporter names the plugins of the typed contract"
    );
    let memory: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  memory."))
        .collect();
    assert_eq!(
        memory,
        [
            "  memory.sysbench  内存 sysbench（工具：sysbench 1.0.20）",
            "  memory.stream  内存 STREAM（工具：stream 5.10）",
            "  memory.latency  内存延迟（工具：multichase 2026.10.01）",
            "  memory.numa  内存 NUMA（适用：多个 NUMA 节点）（工具：multichase 2026.10.01）"
        ]
    );
    let disk: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  disk."))
        .collect();
    assert_eq!(
        disk,
        [
            "  disk.health  硬盘健康（适用：独立服务器）（工具：smartctl 7.5）",
            "  disk.fio  硬盘性能（工具：fio 3.43）"
        ]
    );
}

#[test]
fn the_plan_keeps_the_reporters_names() {
    let planned = app::prepare(&cli(&["--only", "cpu"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("cpu.sysbench", Some("CPU 性能")),
            ("cpu.geekbench", Some("Geekbench 7"))
        ]
    );
    let planned = app::prepare(&cli(&["--only", "disk"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("disk.health", Some("硬盘健康")),
            ("disk.fio", Some("硬盘性能"))
        ]
    );
    let planned = app::prepare(&cli(&["--only", "memory"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("memory.sysbench", Some("内存 sysbench")),
            ("memory.stream", Some("内存 STREAM")),
            ("memory.latency", Some("内存延迟")),
            ("memory.numa", Some("内存 NUMA"))
        ],
        "the group selects its four plugins"
    );
}

#[test]
fn the_items_that_do_not_apply_here_are_named_as_the_report_names_them() {
    let planned = app::prepare(&cli(&[]), &registry::all()).unwrap();
    let vm = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        ..with_nodes(1)
    };

    let (applicable, names) = orchestrator::for_host(planned, &vm);

    assert_eq!(names, ["硬盘健康", "内存 NUMA"]);
    assert_eq!(applicable.len(), 6, "the others run");
}
