//! The scan.

mod common;

use plugin_disk::health::device;
use plugin_disk::health::device::Device;

#[test]
fn the_owners_scan_lists_each_disk_directly_and_through_the_raid_card() {
    let devices = device::scan(&common::fixture("owner/scan.json")).expect("the scan succeeded");
    let listed: Vec<(&str, &str)> = devices
        .iter()
        .map(|device| (device.name.as_str(), device.kind.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            ("/dev/sda", "sat"),
            ("/dev/sdb", "sat"),
            ("/dev/sdc", "sat"),
            ("/dev/sdd", "sat"),
            ("/dev/bus/0", "sat+megaraid,18"),
            ("/dev/bus/0", "sat+megaraid,21"),
            ("/dev/bus/0", "sat+megaraid,23"),
            ("/dev/bus/0", "sat+megaraid,24"),
        ],
        "smartctl's order is kept"
    );
}

#[test]
fn a_scan_without_disks_lists_nothing() {
    // smartctl prints no `devices` at all then (fixture from a container without disks).
    let devices = device::scan(&common::fixture("local/scan-empty.json"));
    assert_eq!(devices, Ok(Vec::new()));
}

#[test]
fn a_failed_scan_gives_smartctls_error() {
    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 2,
        "messages": [{"string": "Unable to open /dev", "severity": "error"}]}}"#;
    assert_eq!(device::scan(output), Err("Unable to open /dev".to_owned()));
}

#[test]
fn output_that_is_not_smartctls_json_fails_the_scan() {
    let expected = Err("smartctl 的输出无法解析".to_owned());
    assert_eq!(device::scan("Segmentation fault"), expected, "not JSON");
    assert_eq!(device::scan("{}"), expected, "JSON without an exit status");
}

#[test]
fn a_device_is_read_back_with_its_type_and_name() {
    let device = Device {
        name: "/dev/bus/0".to_owned(),
        kind: "sat+megaraid,18".to_owned(),
    };
    assert_eq!(
        device.read_args(),
        ["--json", "-x", "-d", "sat+megaraid,18", "/dev/bus/0"]
    );
}

#[test]
fn the_scan_opens_the_devices_and_prints_json() {
    assert_eq!(device::SCAN_ARGS, ["--scan-open", "--json"]);
}

#[test]
fn the_runners_scans_list_their_single_disk() {
    let nvme = device::scan(&common::fixture("runner/scan-nvme.json")).expect("a scan");
    let expected = Device {
        name: "/dev/nvme0".to_owned(),
        kind: "nvme".to_owned(),
    };
    assert_eq!(nvme, [expected]);
    let scsi = device::scan(&common::fixture("runner/scan-virtual-disk.json")).expect("a scan");
    let expected = Device {
        name: "/dev/sda".to_owned(),
        kind: "scsi".to_owned(),
    };
    assert_eq!(scsi, [expected]);
}
