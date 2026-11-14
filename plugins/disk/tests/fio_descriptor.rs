//! The item's identity and requirements.

use std::time::Duration;

use bench_api::Net;
use bench_api::Plugin;
use plugin_disk::fio::Fio;

#[test]
fn the_item_runs_on_every_platform_with_the_pinned_fio() {
    let descriptor = Fio::DESCRIPTOR;
    assert_eq!(descriptor.id, "disk.fio");
    assert_eq!(descriptor.timeout, Duration::from_secs(10 * 60));
    assert!(descriptor.requires.platforms.is_empty(), "every platform");
    assert_eq!(descriptor.requires.net, Net::None);
    assert_eq!(descriptor.requires.glibc, None, "fio is linked statically");
    let tools: Vec<(&str, &str)> = descriptor
        .requires
        .tools
        .iter()
        .map(|tool| (tool.name, tool.version))
        .collect();
    assert_eq!(tools, [("fio", "3.43")]);
}
