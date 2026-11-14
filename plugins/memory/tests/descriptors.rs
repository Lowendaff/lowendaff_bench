//! The plugins' identities and requirements: each needs only its own tool, runs on every
//! platform, and has a timeout of its own; only the NUMA chapter needs two NUMA nodes.

use std::time::Duration;

use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use plugin_memory::latency::Latency;
use plugin_memory::numa::Numa;
use plugin_memory::stream::Stream;
use plugin_memory::sysbench::Sysbench;

/// A descriptor's id, tools with their versions, timeout in minutes and NUMA requirement.
fn summary(descriptor: &Descriptor) -> (&str, Vec<(&str, &str)>, u64, bool) {
    let requires = &descriptor.requires;
    assert!(requires.platforms.is_empty(), "every platform");
    assert_eq!(requires.net, Net::None);
    assert_eq!(requires.glibc, None, "the tools are linked statically");
    let tools = requires
        .tools
        .iter()
        .map(|tool| (tool.name, tool.version))
        .collect();
    assert_eq!(descriptor.timeout.as_secs() % 60, 0);
    (
        descriptor.id,
        tools,
        descriptor.timeout.as_secs() / 60,
        requires.numa,
    )
}

#[test]
fn each_chapter_needs_only_its_own_tool_and_has_its_own_timeout() {
    assert_eq!(
        summary(&Sysbench::DESCRIPTOR),
        ("memory.sysbench", vec![("sysbench", "1.0.20")], 20, false)
    );
    assert_eq!(
        summary(&Stream::DESCRIPTOR),
        ("memory.stream", vec![("stream", "5.10")], 30, false)
    );
    assert_eq!(
        summary(&Latency::DESCRIPTOR),
        (
            "memory.latency",
            vec![("multichase", "2026.10.01")],
            20,
            false
        )
    );
    assert_eq!(
        summary(&Numa::DESCRIPTOR),
        ("memory.numa", vec![("multichase", "2026.10.01")], 30, true)
    );
    assert_eq!(Sysbench::DESCRIPTOR.timeout, Duration::from_secs(20 * 60));
}
