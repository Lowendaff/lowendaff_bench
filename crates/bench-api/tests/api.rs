//! Contract tests for the plugin API: glibc versions, and the platforms and NUMA layouts that a
//! plugin applies to.

use bench_api::Requirements;
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
