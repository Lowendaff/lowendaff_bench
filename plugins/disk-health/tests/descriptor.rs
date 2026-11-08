//! The item's identity and requirements (design disk-health, §5.1).

use std::time::Duration;

use bench_api::Net;
use bench_api::PlatformKind;
use bench_api::Plugin;
use plugin_disk_health::Smartctl;

#[test]
fn the_item_runs_only_on_dedicated_servers_with_the_pinned_smartctl() {
    let descriptor = Smartctl::DESCRIPTOR;
    assert_eq!(descriptor.id, "disk.health");
    assert_eq!(descriptor.title, "硬盘健康");
    assert_eq!(descriptor.timeout, Duration::from_secs(5 * 60));
    assert_eq!(descriptor.requires.platforms, [PlatformKind::Dedicated]);
    assert_eq!(descriptor.requires.net, Net::None);
    let tools: Vec<(&str, &str)> = descriptor
        .requires
        .tools
        .iter()
        .map(|tool| (tool.name, tool.version))
        .collect();
    assert_eq!(tools, [("smartctl", "7.5")]);
}
