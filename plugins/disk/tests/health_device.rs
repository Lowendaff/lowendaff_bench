//! The commands that scan the disks and read one back.

use plugin_disk::health::device;
use plugin_disk::health::device::Device;

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
