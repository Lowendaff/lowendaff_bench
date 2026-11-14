//! The registry's checks, which every plugin must pass: ids, names, limits, config strictness and
//! tool pins.

use std::time::Duration;

use bench::registry;
use bench::registry::Entry;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Net;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed::Context;
use bench_api::typed::Descriptor;
use bench_api::typed::Measurement;
use bench_api::typed::Plugin;
use bench_api::typed::Stop;
use bench_api::typed::ToolVersion;
use bench_host::platform::PlatformKind;
use serde::Deserialize;
use serde::Serialize;

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
    on(id, tools, &[])
}

const fn on(
    id: &'static str,
    tools: &'static [ToolSpec],
    platforms: &'static [PlatformKind],
) -> Descriptor {
    Descriptor {
        id,
        requires: Requirements {
            tools,
            net: Net::None,
            platforms,
            glibc: None,
            numa: false,
        },
        timeout: Duration::from_secs(10),
    }
}

#[derive(Debug, Clone, Serialize)]
struct Count(u32);

impl Measurement for Count {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
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
            type Output = Count;
            type Failure = ();
            const DESCRIPTOR: Descriptor = $descriptor;

            async fn run(&self, _ctx: &Context<Count>) -> Result<Count, Stop<()>> {
                Ok(Count(0))
            }
        }
    };
}

/// The result enum has no variant for the tests' plugins, which never run here.
fn unused<O, F>(_report: Report<O, F>) -> Measured {
    Measured::CpuGeekbench(Report {
        output: None,
        stop: None,
    })
}

/// The entry of a plugin of these tests, with the name that the reporter would give it.
fn named<P: Plugin>(name: &'static str) -> Entry {
    let mut entry = registry::entry::<P>(unused);
    entry.title = Some(name);
    entry
}

plugin!(Good, "disk.fio", &[FIO]);
plugin!(AlsoFio, "disk.fio-seq", &[FIO]);
plugin!(BadId, "Disk FIO", &[]);
plugin!(Unnamed, "test.unnamed", &[]);

/// Forgot `deny_unknown_fields`.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Lax {
    level: u8,
}

impl Plugin for Lax {
    type Output = Count;
    type Failure = ();
    const DESCRIPTOR: Descriptor = descriptor("test.lax", &[]);

    async fn run(&self, _ctx: &Context<Count>) -> Result<Count, Stop<()>> {
        Ok(Count(u32::from(self.level)))
    }
}

/// Forgot `#[serde(default)]`, so an empty config table cannot build.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Strict {
    path: String,
}

impl Plugin for Strict {
    type Output = Count;
    type Failure = ();
    const DESCRIPTOR: Descriptor = descriptor("test.strict", &[]);

    async fn run(&self, _ctx: &Context<Count>) -> Result<Count, Stop<()>> {
        let _ = &self.path;
        Ok(Count(0))
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

// Repeats `disk.fio`, with another fio and no limit.
plugin!(
    FioAgain,
    Descriptor {
        timeout: Duration::ZERO,
        ..descriptor("disk.fio", &[OTHER_FIO])
    }
);

plugin!(InfoVm, on("info.vm", &[], &[PlatformKind::Vm]));
plugin!(
    InfoDedicated,
    on("info.dedicated", &[], &[PlatformKind::Dedicated])
);
plugin!(InfoEverywhere, on("info.any", &[], &[]));

#[test]
fn a_correct_registry_passes() {
    assert_eq!(
        registry::validate(&[named::<Good>("硬盘性能"), named::<AlsoFio>("硬盘顺序读写")]),
        Vec::<String>::new()
    );
}

#[test]
fn registry_checks_catch_common_plugin_mistakes() {
    let problems = registry::validate(&[
        named::<Good>("硬盘性能"),
        named::<Good>("硬盘性能"),
        named::<BadId>("错误的 id"),
        named::<Lax>("宽松的配置"),
        named::<Strict>("严格的配置"),
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
    let problems = registry::validate(&[
        named::<Good>("硬盘性能"),
        named::<BrokenTool>("网络"),
        named::<ConflictingFio>("旧的 fio"),
        named::<NestedArchive>("嵌套的压缩包"),
        named::<Archive>("压缩包"),
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
fn no_two_plugins_share_a_name_whatever_their_platforms() {
    let problems = registry::validate(&[
        named::<InfoVm>("信息"),
        named::<InfoDedicated>("信息"),
        named::<InfoEverywhere>("信息"),
    ]);
    assert_eq!(
        problems,
        [
            "info.dedicated: shares its name with info.vm",
            "info.any: shares its name with info.vm",
        ]
    );
}

#[test]
fn the_reporter_must_name_each_plugin_and_its_limit_must_be_positive() {
    let problems = registry::validate(&[
        named::<Good>("硬盘性能"),
        registry::entry::<FioAgain>(unused),
        registry::entry::<Lax>(unused),
        named::<Unnamed>(""),
    ]);
    assert_eq!(
        problems,
        [
            "disk.fio: duplicate id",
            "disk.fio: timeout must be positive",
            "disk.fio: tool fio differs from another plugin's declaration",
            "test.lax: the reporter has no name for it",
            "test.lax: config accepts unknown fields (missing #[serde(deny_unknown_fields)]?)",
            "test.unnamed: the reporter has no name for it",
        ]
    );
}

#[test]
fn a_plugin_is_built_from_its_config() {
    let entry = registry::entry::<Lax>(unused);
    assert_eq!(entry.id, "test.lax");
    assert_eq!(entry.title, None, "the reporter has no name for it");
    let planned = entry
        .build(Some(&serde_json::json!({ "level": 3 })))
        .expect("a level fits");
    assert_eq!(planned.id(), "test.lax");
    let Err(error) = entry.build(Some(&serde_json::json!({ "level": 300 }))) else {
        panic!("300 is no u8");
    };
    assert!(error.contains("300"), "{error}");
}
