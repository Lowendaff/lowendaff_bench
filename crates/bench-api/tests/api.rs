//! Contract tests for the plugin API: record invariants, error chains, tool lookup, glibc
//! versions and platform kinds.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Env;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_api::Table;
use bench_host::glibc::GlibcVersion;
use bench_host::numa::Layout;
use bench_host::numa::Node;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;

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

/// A host on `platform` whose NUMA layout is `numa`.
fn host(platform: Platform, numa: Read<Layout>) -> HostProfile {
    HostProfile {
        platform,
        numa,
        ..HostProfile::default()
    }
}

/// A NUMA layout of `count` nodes, each with a CPU and memory, all of which this process may use.
fn nodes(count: usize) -> Read<Layout> {
    let nodes: Vec<Node> = (0..count)
        .map(|id| Node {
            id,
            cpus: vec![id],
            total: Some(1 << 30),
            free: Some(1 << 29),
        })
        .collect();
    Read::Done(Layout {
        cpus: Some((0..count).collect()),
        mems: Some((0..count).collect()),
        nodes,
    })
}

#[test]
fn a_plugin_applies_to_the_kinds_it_declares() {
    let vm = Requirements {
        platforms: &[PlatformKind::Vm],
        ..Requirements::NONE
    };
    let on = |platform| host(platform, nodes(1));
    assert!(vm.applies_to(&on(Platform::Vm(Hypervisor::Kvm))));
    assert!(vm.applies_to(&on(Platform::Vm(Hypervisor::Other))));
    assert!(!vm.applies_to(&on(Platform::Dedicated)));
    assert!(!vm.applies_to(&on(Platform::Container(ContainerKind::Lxc))));
    for platform in [
        Platform::Dedicated,
        Platform::Vm(Hypervisor::Xen),
        Platform::Container(ContainerKind::Docker),
    ] {
        assert!(
            Requirements::NONE.applies_to(&on(platform)),
            "an empty list means every platform, including {platform:?}"
        );
    }
}

#[test]
fn a_plugin_that_needs_numa_nodes_applies_only_where_two_can_be_used() {
    let numa = Requirements {
        numa: true,
        ..Requirements::NONE
    };
    for count in [0, 1] {
        assert!(!numa.applies_to(&host(Platform::Dedicated, nodes(count))));
        assert!(
            Requirements::NONE.applies_to(&host(Platform::Dedicated, nodes(count))),
            "the others run anyway"
        );
    }
    assert!(numa.applies_to(&host(Platform::Dedicated, nodes(2))));
    assert!(numa.applies_to(&host(Platform::Vm(Hypervisor::Kvm), nodes(4))));
    for unread in [Read::TimedOut, Read::Failed] {
        assert!(
            !numa.applies_to(&host(Platform::Dedicated, unread.clone())),
            "{unread:?}: an unknown layout has no nodes"
        );
    }
    let dedicated = Requirements {
        platforms: &[PlatformKind::Dedicated],
        ..numa
    };
    assert!(dedicated.applies_to(&host(Platform::Dedicated, nodes(2))));
    assert!(
        !dedicated.applies_to(&host(Platform::Vm(Hypervisor::Kvm), nodes(2))),
        "both conditions hold"
    );
    assert!(!dedicated.applies_to(&host(Platform::Dedicated, nodes(1))));
}
