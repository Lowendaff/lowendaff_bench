//! Contract tests for the plugin API: record invariants, error chains, tool lookup and
//! the registry checks that every plugin must pass.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Env;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::Table;
use bench_api::ToolAsset;
use bench_api::ToolSpec;
use serde::Deserialize;

const SHA: &str = "4f2d1c0b9a8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a392817060504";

const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.39",
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
    Descriptor {
        id,
        title: "测试项",
        version: "0.1.0",
        requires: Requirements {
            tools,
            net: Net::None,
        },
        timeout: Duration::from_secs(10),
    }
}

macro_rules! plugin {
    ($name:ident, $id:expr, $tools:expr) => {
        #[derive(Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct $name {}

        impl Plugin for $name {
            const DESCRIPTOR: Descriptor = descriptor($id, $tools);

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
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "http://example.com/iperf3",
        sha256: "ABC",
        size: 0,
    }],
};
plugin!(BrokenTool, "net.iperf", &[BROKEN_TOOL]);

const OTHER_FIO: ToolSpec = ToolSpec {
    version: "3.40",
    ..FIO
};
plugin!(ConflictingFio, "disk.fio-old", &[OTHER_FIO]);

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
    ]);
    let expected = [
        "net.iperf: tool iperf3 has no aarch64 build",
        "net.iperf: tool iperf3 must be downloaded over https",
        "net.iperf: tool iperf3 sha256 must be 64 lowercase hex digits",
        "net.iperf: tool iperf3 size must be positive",
        "disk.fio-old: tool fio differs from another plugin's declaration",
    ];
    for line in expected {
        assert!(
            problems.iter().any(|p| p == line),
            "missing {line:?} in {problems:#?}"
        );
    }
    assert_eq!(problems.len(), expected.len(), "{problems:#?}");
}
