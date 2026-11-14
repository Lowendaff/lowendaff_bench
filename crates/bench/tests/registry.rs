//! The registry's checks, which every plugin of either contract must pass: ids, names, limits,
//! config strictness and tool pins.

use std::time::Duration;

use bench::registry;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed;
use bench_api::typed::Measurement;
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
            numa: false,
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
fn a_correct_registry_passes() {
    assert_eq!(
        registry::validate(&[registry::legacy::<Good>(), registry::legacy::<AlsoFio>()]),
        Vec::<String>::new()
    );
}

#[test]
fn registry_checks_catch_common_plugin_mistakes() {
    let problems = registry::validate(&[
        registry::legacy::<Good>(),
        registry::legacy::<Good>(),
        registry::legacy::<BadId>(),
        registry::legacy::<Lax>(),
        registry::legacy::<Strict>(),
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
        registry::legacy::<Good>(),
        registry::legacy::<BrokenTool>(),
        registry::legacy::<ConflictingFio>(),
        registry::legacy::<NestedArchive>(),
        registry::legacy::<Archive>(),
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
fn plugins_that_share_a_title_need_disjoint_platforms() {
    assert_eq!(
        registry::validate(&[
            registry::legacy::<InfoVm>(),
            registry::legacy::<InfoDedicated>(),
        ]),
        Vec::<String>::new()
    );
    let problems = registry::validate(&[
        registry::legacy::<InfoVm>(),
        registry::legacy::<InfoVmOrContainer>(),
        registry::legacy::<InfoEverywhere>(),
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

#[derive(Debug, Clone, Serialize)]
struct Count(u32);

impl Measurement for Count {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

/// The result enum has no variant for the tests' plugins, which never run here.
fn unused<O, F>(_report: Report<O, F>) -> Measured {
    Measured::Legacy(Report {
        output: None,
        stop: None,
    })
}

/// A typed plugin that repeats `disk.fio` with another fio and no limit.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TypedFio {}

impl typed::Plugin for TypedFio {
    type Output = Count;
    type Failure = ();
    const DESCRIPTOR: typed::Descriptor = typed::Descriptor {
        id: "disk.fio",
        requires: Requirements {
            tools: &[OTHER_FIO],
            ..Requirements::NONE
        },
        timeout: Duration::ZERO,
    };

    async fn run(&self, _ctx: &typed::Context<Count>) -> Result<Count, Stop<()>> {
        Ok(Count(1))
    }
}

/// A typed plugin whose config takes a level but forgot `deny_unknown_fields`.
#[derive(Default, Deserialize)]
#[serde(default)]
struct TypedLax {
    level: u8,
}

impl typed::Plugin for TypedLax {
    type Output = Count;
    type Failure = ();
    const DESCRIPTOR: typed::Descriptor = typed::Descriptor {
        id: "test.typed-lax",
        requires: Requirements::NONE,
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, _ctx: &typed::Context<Count>) -> Result<Count, Stop<()>> {
        Ok(Count(u32::from(self.level)))
    }
}

#[test]
fn typed_plugins_pass_the_same_checks_and_need_a_name() {
    let problems = registry::validate(&[
        registry::legacy::<Good>(),
        registry::entry::<TypedFio>(unused),
        registry::entry::<TypedLax>(unused),
    ]);
    assert_eq!(
        problems,
        [
            "disk.fio: duplicate id",
            "disk.fio: timeout must be positive",
            "disk.fio: tool fio differs from another plugin's declaration",
            "test.typed-lax: the reporter has no name for it",
            "test.typed-lax: config accepts unknown fields (missing #[serde(deny_unknown_fields)]?)",
        ]
    );
}

#[test]
fn a_typed_plugin_is_built_from_its_config() {
    let entry = registry::entry::<TypedLax>(unused);
    assert_eq!(entry.id, "test.typed-lax");
    assert_eq!(entry.title, None, "the reporter has no name for it");
    let planned = entry
        .build(Some(&serde_json::json!({ "level": 3 })))
        .expect("a level fits");
    assert_eq!(planned.id(), "test.typed-lax");
    let Err(error) = entry.build(Some(&serde_json::json!({ "level": 300 }))) else {
        panic!("300 is no u8");
    };
    assert!(error.contains("300"), "{error}");
}
