//! Contract tests for the plugin API: record invariants, error chains, tool lookup and
//! the registry checks that every plugin must pass.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::ContainerKind;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Env;
use bench_api::GlibcVersion;
use bench_api::Hypervisor;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::Net;
use bench_api::Platform;
use bench_api::PlatformKind;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::Table;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use serde::Deserialize;

const SHA: &str = "4f2d1c0b9a8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a392817060504";

const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.39",
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://example.com/fio-x86_64",
            sha256: SHA,
            size: 10,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://example.com/fio-aarch64",
            sha256: SHA,
            size: 10,
        },
    ],
};

const fn descriptor(id: &'static str, tools: &'static [ToolSpec]) -> Descriptor {
    variant(id, id, tools, &[])
}

const fn variant(
    id: &'static str,
    title: &'static str,
    tools: &'static [ToolSpec],
    platforms: &'static [PlatformKind],
) -> Descriptor {
    Descriptor {
        id,
        title,
        version: "0.1.0",
        requires: Requirements {
            tools,
            net: Net::None,
            platforms,
            glibc: None,
        },
        timeout: Duration::from_secs(10),
    }
}

macro_rules! plugin {
    ($name:ident, $id:expr, $tools:expr) => {
        plugin!($name, descriptor($id, $tools));
    };
    ($name:ident, $descriptor:expr) => {
        #[derive(Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct $name {}

        impl Plugin for $name {
            const DESCRIPTOR: Descriptor = $descriptor;

            async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
                Ok(())
            }
        }
    };
}

plugin!(Good, "disk.fio", &[FIO]);
plugin!(AlsoFio, "disk.fio-seq", &[FIO]);
plugin!(BadId, "Disk FIO", &[]);

/// Forgot `deny_unknown_fields`.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Lax {
    level: u8,
}

impl Plugin for Lax {
    const DESCRIPTOR: Descriptor = descriptor("test.lax", &[]);

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        let _ = self.level;
        Ok(())
    }
}

/// Forgot `#[serde(default)]`, so an empty config table cannot build.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Strict {
    path: String,
}

impl Plugin for Strict {
    const DESCRIPTOR: Descriptor = descriptor("test.strict", &[]);

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        let _ = &self.path;
        Ok(())
    }
}

const BROKEN_TOOL: ToolSpec = ToolSpec {
    name: "iperf3",
    version: "3.19",
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "http://example.com/iperf3",
        sha256: "ABC",
        size: 0,
    }],
};
plugin!(BrokenTool, "net.iperf", &[BROKEN_TOOL]);

const NESTED_ARCHIVE: ToolSpec = ToolSpec {
    name: "geekbench",
    version: "7.1.0",
    form: ToolForm::TarGz {
        executable: "bin/geekbench7",
    },
    ..FIO
};
plugin!(NestedArchive, "cpu.nested", &[NESTED_ARCHIVE]);

const ARCHIVE: ToolSpec = ToolSpec {
    name: "geekbench7",
    version: "7.1.0",
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    ..FIO
};
plugin!(Archive, "cpu.archive", &[ARCHIVE]);

const OTHER_FIO: ToolSpec = ToolSpec {
    version: "3.40",
    ..FIO
};
plugin!(ConflictingFio, "disk.fio-old", &[OTHER_FIO]);

plugin!(InfoVm, variant("info.vm", "信息", &[], &[PlatformKind::Vm]));
plugin!(
    InfoDedicated,
    variant("info.dedicated", "信息", &[], &[PlatformKind::Dedicated])
);
plugin!(
    InfoVmOrContainer,
    variant(
        "info.shared",
        "信息",
        &[],
        &[PlatformKind::Vm, PlatformKind::Container]
    )
);
plugin!(InfoEverywhere, variant("info.any", "信息", &[], &[]));

#[test]
fn measurement_displays_a_value_with_unit_or_the_missing_reason() {
    let measured = Measurement::try_new(171.01, "MB/s").expect("finite value with a unit");
    assert_eq!(measured.to_string(), "171.01 MB/s");
    let whole = Measurement::try_new(940.0, "Mbit/s").expect("finite value with a unit");
    assert_eq!(
        whole.to_string(),
        "940 Mbit/s",
        "whole numbers print without decimals"
    );

    assert_eq!(Measurement::new_missing(Missing::Busy).to_string(), "繁忙");
    assert_eq!(Missing::Timeout.to_string(), "超时");

    assert!(Measurement::try_new(f64::NAN, "ms").is_err());
    assert!(Measurement::try_new(f64::INFINITY, "ms").is_err());
    assert!(Measurement::try_new(1.0, "").is_err());
}

#[test]
fn table_rows_must_match_headers() {
    let table = Table::try_new(
        "fio",
        "随机读写",
        vec!["块大小".into(), "读取".into()],
        vec![vec!["4k".into(), "171 MB/s".into()]],
    )
    .unwrap();
    assert_eq!(table.headers(), ["块大小", "读取"]);
    assert_eq!(Record::Table(table).id(), "fio");

    let ragged = Table::try_new(
        "t",
        "t",
        vec!["a".into(), "b".into()],
        vec![vec!["只有一列".into()]],
    );
    assert_eq!(ragged.unwrap_err().0, "表格 t 第 0 行有 1 列，应为 2 列");
}

#[derive(Debug)]
struct Outer(std::io::Error);

impl std::fmt::Display for Outer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("读取结果失败")
    }
}

impl std::error::Error for Outer {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn question_mark_keeps_the_cause_chain() {
    let err = PluginError::from(Outer(std::io::Error::other("磁盘已满")));
    let PluginError::Fail(message) = err else {
        panic!("expected Fail")
    };
    assert_eq!(message, "读取结果失败: 磁盘已满");
}

#[tokio::test]
async fn tool_lookup_returns_only_verified_tools() {
    let env = Env {
        tools: BTreeMap::from([("fio".to_owned(), PathBuf::from("/run/tools/fio"))]),
        ..Env::default()
    };
    let deadline = tokio::time::Instant::now();
    let (ctx, _records) = Context::new(
        Arc::new(env),
        deadline,
        PathBuf::from("/run/work"),
        CancellationToken::new(),
    );

    assert_eq!(ctx.tool("fio").unwrap(), PathBuf::from("/run/tools/fio"));
    let Err(PluginError::Fail(message)) = ctx.tool("iperf3") else {
        panic!("expected Fail")
    };
    assert_eq!(message, "工具 iperf3 未声明或不可用");
}

#[test]
fn a_correct_registry_passes() {
    assert_eq!(
        bench_api::validate_registry(&[
            bench_api::register::<Good>(),
            bench_api::register::<AlsoFio>()
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn registry_checks_catch_common_plugin_mistakes() {
    let problems = bench_api::validate_registry(&[
        bench_api::register::<Good>(),
        bench_api::register::<Good>(),
        bench_api::register::<BadId>(),
        bench_api::register::<Lax>(),
        bench_api::register::<Strict>(),
    ]);
    let expected = [
        "disk.fio: duplicate id",
        "Disk FIO: id must be lowercase dotted segments of [a-z0-9-]",
        "test.lax: config accepts unknown fields (missing #[serde(deny_unknown_fields)]?)",
    ];
    for line in expected {
        assert!(
            problems.iter().any(|p| p == line),
            "missing {line:?} in {problems:#?}"
        );
    }
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("test.strict: empty config table must build")),
        "{problems:#?}"
    );
    assert_eq!(problems.len(), 4, "{problems:#?}");
}

#[test]
fn registry_checks_catch_bad_tool_pins() {
    let problems = bench_api::validate_registry(&[
        bench_api::register::<Good>(),
        bench_api::register::<BrokenTool>(),
        bench_api::register::<ConflictingFio>(),
        bench_api::register::<NestedArchive>(),
        bench_api::register::<Archive>(),
    ]);
    let expected = [
        "net.iperf: tool iperf3 has no aarch64 build",
        "net.iperf: tool iperf3 must be downloaded over https",
        "net.iperf: tool iperf3 sha256 must be 64 lowercase hex digits",
        "net.iperf: tool iperf3 size must be positive",
        "disk.fio-old: tool fio differs from another plugin's declaration",
        "cpu.nested: tool geekbench must name its executable in the archive by file name",
    ];
    for line in expected {
        assert!(
            problems.iter().any(|p| p == line),
            "missing {line:?} in {problems:#?}"
        );
    }
    assert_eq!(problems.len(), expected.len(), "{problems:#?}");
}

#[test]
fn glibc_versions_order_by_major_then_minor_and_print_as_major_dot_minor() {
    let version = |major, minor| GlibcVersion { major, minor };
    assert!(version(2, 34) < version(2, 35));
    assert!(
        version(2, 9) < version(2, 10),
        "minor numbers compare as numbers"
    );
    assert!(version(1, 99) < version(2, 0));
    assert_eq!(version(2, 35).to_string(), "2.35");
}

#[test]
fn plugins_that_share_a_title_need_disjoint_platforms() {
    assert_eq!(
        bench_api::validate_registry(&[
            bench_api::register::<InfoVm>(),
            bench_api::register::<InfoDedicated>(),
        ]),
        Vec::<String>::new()
    );
    let problems = bench_api::validate_registry(&[
        bench_api::register::<InfoVm>(),
        bench_api::register::<InfoVmOrContainer>(),
        bench_api::register::<InfoEverywhere>(),
    ]);
    assert_eq!(
        problems,
        [
            "info.shared: shares its title with info.vm, so their platforms must not overlap",
            "info.any: shares its title with info.vm, so their platforms must not overlap",
            "info.any: shares its title with info.shared, so their platforms must not overlap",
        ]
    );
}

#[test]
fn a_plugin_applies_to_the_kinds_it_declares() {
    let vm = Requirements {
        platforms: &[PlatformKind::Vm],
        ..Requirements::NONE
    };
    assert!(vm.applies_to(Platform::Vm(Hypervisor::Kvm)));
    assert!(vm.applies_to(Platform::Vm(Hypervisor::Other)));
    assert!(!vm.applies_to(Platform::Dedicated));
    assert!(!vm.applies_to(Platform::Container(ContainerKind::Lxc)));
    for platform in [
        Platform::Dedicated,
        Platform::Vm(Hypervisor::Xen),
        Platform::Container(ContainerKind::Docker),
    ] {
        assert!(
            Requirements::NONE.applies_to(platform),
            "an empty list means every platform, including {platform}"
        );
    }
}

#[test]
fn platforms_are_named_in_chinese() {
    let names = [
        (Platform::Dedicated, "未检测到虚拟化"),
        (Platform::Vm(Hypervisor::Kvm), "KVM 虚拟机"),
        (Platform::Vm(Hypervisor::Qemu), "QEMU 虚拟机（纯软件模拟）"),
        (Platform::Vm(Hypervisor::HyperV), "Hyper-V 虚拟机"),
        (Platform::Vm(Hypervisor::Other), "虚拟机（类型未知）"),
        (Platform::Container(ContainerKind::Lxc), "LXC 容器"),
        (Platform::Container(ContainerKind::Openvz), "OpenVZ 容器"),
        (Platform::Container(ContainerKind::Wsl), "WSL"),
        (
            Platform::Container(ContainerKind::Other),
            "容器（类型未知）",
        ),
    ];
    for (platform, name) in names {
        assert_eq!(platform.to_string(), name);
    }
    assert_eq!(PlatformKind::Dedicated.to_string(), "独立服务器");
    assert_eq!(Platform::default(), Platform::Dedicated);
}
