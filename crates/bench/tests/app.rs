//! Command-line flow: argument parsing, selection, config file, and a full run through
//! the terminal renderer with the run directory removed afterwards.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bench::app;
use bench::app::Cli;
use bench::app::EXIT_INTERRUPTED;
use bench::orchestrator::Planned;
use bench::progress;
use bench::results::Measured;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Measurement;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Registration;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use common::Captured;
use serde::Deserialize;

mod common;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Good {
    samples: u32,
}

impl Plugin for Good {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.good",
        title: "示例测试",
        version: "0.1.0",
        requires: Requirements::NONE,
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::metric(
            "rate",
            "速率",
            Measurement::try_new(f64::from(self.samples), "MB/s")?,
        ));
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Other {}

impl Plugin for Other {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.other",
        title: "另一个测试",
        ..Good::DESCRIPTOR
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}

const REGISTRY: &[Registration] = &[
    bench_api::register::<Good>(),
    bench_api::register::<Other>(),
];

/// A group of one plugin, whose id is the group's name.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Lone {}

impl Plugin for Lone {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "lone",
        title: "单独的测试",
        ..Good::DESCRIPTOR
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}

const GROUPS: &[Registration] = &[
    bench_api::register::<Good>(),
    bench_api::register::<Lone>(),
    bench_api::register::<Other>(),
];

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
        app::select(REGISTRY, &[], &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(REGISTRY, &ids(&["test.other", "test.good"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(REGISTRY, &[], &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(REGISTRY, &ids(&["nope"]), &ids(&["nada"])).unwrap_err(),
        ["未知测试项或分组 `nope`", "未知测试项或分组 `nada`"]
    );
}

#[test]
fn a_group_name_selects_every_plugin_of_the_group() {
    assert_eq!(bench_api::validate_registry(GROUPS), Vec::<String>::new());
    assert_eq!(
        app::select(GROUPS, &ids(&["test"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(app::select(GROUPS, &[], &ids(&["test"])).unwrap(), ["lone"]);
    assert_eq!(
        app::select(GROUPS, &ids(&["test.other", "lone"]), &[]).unwrap(),
        ["lone", "test.other"]
    );
    assert_eq!(
        app::select(GROUPS, &ids(&["test"]), &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(GROUPS, &ids(&["tes", "test."]), &ids(&["good", "lon"])).unwrap_err(),
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
    let Err(errors) = app::prepare(&cli(&["--config", &config]), REGISTRY) else {
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
        REGISTRY,
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
    assert!(text.contains("  速率              3 MB/s\n"), "{text}");
    assert!(
        text.contains("\n共 1 项：完成 1，跳过 0，失败 0，超时 0，中断 0。总用时 "),
        "{text}"
    );
}

#[tokio::test]
async fn an_interrupted_run_exits_130_and_still_lists_every_plugin() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let planned = app::prepare(&cli(&[]), REGISTRY).unwrap();

    let (code, text, _) = execute(planned, HostProfile::default(), &cancel).await;

    assert_eq!(code, EXIT_INTERRUPTED);
    assert!(
        text.contains("【test】\n  示例测试\n    已中断\n  另一个测试\n    已中断\n"),
        "two plugins of one group share a section: {text}"
    );
    assert_eq!(text.matches("【test】").count(), 1, "{text}");
    assert!(
        text.contains("共 2 项：完成 0，跳过 0，失败 0，超时 0，中断 2。"),
        "{text}"
    );
}

/// Two variants of one test item, and an item that applies to dedicated servers only.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct InfoDedicated {}

impl Plugin for InfoDedicated {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.info-dedicated",
        title: "系统信息",
        requires: Requirements {
            platforms: &[PlatformKind::Dedicated],
            ..Requirements::NONE
        },
        ..Good::DESCRIPTOR
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("variant", "版本", "独立服务器版"));
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct InfoVm {}

impl Plugin for InfoVm {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.info-vm",
        title: "系统信息",
        requires: Requirements {
            platforms: &[PlatformKind::Vm],
            ..Requirements::NONE
        },
        ..Good::DESCRIPTOR
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::fact("variant", "版本", "虚拟机版"));
        Ok(())
    }
}

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

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DiskHealth {}

impl Plugin for DiskHealth {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.disk-health",
        title: "硬盘健康",
        requires: Requirements {
            tools: &[SMARTCTL],
            net: Net::None,
            platforms: &[PlatformKind::Dedicated],
            glibc: None,
        },
        ..Good::DESCRIPTOR
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}

const PLATFORM_REGISTRY: &[Registration] = &[
    bench_api::register::<InfoDedicated>(),
    bench_api::register::<InfoVm>(),
    bench_api::register::<DiskHealth>(),
];

#[tokio::test]
async fn only_plugins_for_the_detected_platform_run() {
    assert_eq!(
        bench_api::validate_registry(PLATFORM_REGISTRY),
        Vec::<String>::new()
    );
    let planned = app::prepare(&cli(&[]), PLATFORM_REGISTRY).unwrap();
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        ..HostProfile::default()
    };

    let (code, text, log) = execute(planned, host, &CancellationToken::new()).await;

    assert_eq!(code, 0);
    // The other variant of 系统信息 runs instead, so only 硬盘健康 is reported.
    assert!(
        text.starts_with(&format!(
            "lowendaff-bench {}\n运行环境：KVM 虚拟机\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(text.contains("\n\n【test】\n"), "{text}");
    assert!(text.contains("  本机不适用：硬盘健康\n"), "{text}");
    assert!(text.contains("虚拟机版"), "{text}");
    assert!(!text.contains("独立服务器版"), "{text}");
    assert!(
        !log.iter().any(|line| line.starts_with("准备工具")),
        "tools of plugins that do not apply must not be downloaded: {log:?}"
    );
    assert!(
        text.contains("\n共 1 项：完成 1，跳过 0，失败 0，超时 0，中断 0。"),
        "{text}"
    );
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NeedsSmartctl {}

impl Plugin for NeedsSmartctl {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.smartctl",
        title: "需要工具的测试",
        requires: Requirements {
            tools: &[SMARTCTL],
            ..Requirements::NONE
        },
        ..Good::DESCRIPTOR
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}

#[tokio::test]
async fn a_tool_that_cannot_be_provisioned_skips_its_plugins_with_the_reason() {
    let registry = [bench_api::register::<NeedsSmartctl>()];
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

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NeedsGlibcAndIpv4 {}

impl Plugin for NeedsGlibcAndIpv4 {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.geekbench",
        title: "Geekbench 7",
        requires: Requirements {
            tools: &[GEEKBENCH],
            net: Net::Ipv4,
            platforms: &[],
            glibc: Some(GlibcVersion {
                major: 2,
                minor: 35,
            }),
        },
        ..Good::DESCRIPTOR
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}

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
    let registry = [bench_api::register::<NeedsGlibcAndIpv4>()];
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

/// Reports the host as the first contract hands it to a plugin.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ShowsEnv {}

impl Plugin for ShowsEnv {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.env",
        title: "运行环境测试",
        ..Good::DESCRIPTOR
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let env = ctx.env();
        let seen = format!(
            "{:?} {} {:?} ipv4={} ipv6={}",
            env.platform, env.arch, env.libc, env.ipv4, env.ipv6
        );
        ctx.emit(Record::fact("env", "所见", seen));
        Ok(())
    }
}

#[tokio::test]
async fn plugins_of_the_first_contract_see_the_profile_through_env() {
    let registry = [bench_api::register::<ShowsEnv>()];
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        arch: "riscv64".to_owned(),
        libc: Libc::NoGlibc,
        network: ipv4_only(),
        ..HostProfile::default()
    };
    let planned = app::prepare(&cli(&[]), &registry).unwrap();

    let (_, text, _) = execute(planned, host, &CancellationToken::new()).await;

    assert!(
        text.contains("所见              Vm(Kvm) riscv64 NoGlibc ipv4=true ipv6=false\n"),
        "{text}"
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
        let planned = app::prepare(&cli(&[]), REGISTRY).unwrap();
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
    app::list(PLATFORM_REGISTRY, &mut out);

    assert_eq!(
        String::from_utf8(out).unwrap(),
        "共 3 个测试项\n\
         \x20 test.info-dedicated  系统信息（适用：独立服务器）\n\
         \x20 test.info-vm  系统信息（适用：虚拟机）\n\
         \x20 test.disk-health  硬盘健康（适用：独立服务器）（工具：smartctl 7.4）\n"
    );
}
