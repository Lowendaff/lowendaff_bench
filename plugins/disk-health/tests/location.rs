//! Names and order (design disk-health, §5.3).

use std::fs;

use bench_host::Root;
use plugin_disk_health::device::Device;
use plugin_disk_health::location::Location;

fn device(name: &str, kind: &str) -> Device {
    Device {
        name: name.to_owned(),
        kind: kind.to_owned(),
    }
}

/// A root whose `/sys/class/nvme/nvme0` holds `entries`, next to entries that are no
/// namespaces.
fn nvme_root(entries: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temporary directory");
    let controller = root.path().join("sys/class/nvme/nvme0");
    let mut all = entries.to_vec();
    all.extend(["ng0n1", "hwmon2", "model"]);
    for entry in all {
        fs::create_dir_all(controller.join(entry)).expect("the fixture layout is writable");
    }
    root
}

fn located(name: &str, kind: &str, root: &Root) -> Location {
    Location::of(&device(name, kind), root)
}

#[test]
fn ata_and_scsi_disks_keep_their_kernel_names() {
    let root = Root::new("/nonexistent");
    assert_eq!(
        located("/dev/sda", "sat", &root),
        Location::Named("sda".to_owned())
    );
    assert_eq!(
        located("/dev/sdaa", "scsi", &root),
        Location::Named("sdaa".to_owned())
    );
    assert_eq!(
        located("/dev/hdb", "ata", &root),
        Location::Named("hdb".to_owned())
    );
    assert_eq!(located("/dev/sda", "sat", &root).name(true), "sda");
}

#[test]
fn an_nvme_controller_with_one_namespace_shows_as_that_namespace() {
    let dir = nvme_root(&["nvme0n1"]);
    let root = Root::new(dir.path());
    assert_eq!(located("/dev/nvme0", "nvme", &root).name(false), "nvme0n1");
}

#[test]
fn a_multipath_path_shows_as_its_namespaces_block_device() {
    // nvme<subsystem>c<controller>n<namespace>: the block device is nvme1n1.
    let dir = nvme_root(&["nvme1c0n1"]);
    let root = Root::new(dir.path());
    assert_eq!(located("/dev/nvme0", "nvme", &root).name(false), "nvme1n1");
}

#[test]
fn an_nvme_controller_with_several_namespaces_or_none_keeps_its_name() {
    let dir = nvme_root(&["nvme0n1", "nvme0n2"]);
    let root = Root::new(dir.path());
    assert_eq!(located("/dev/nvme0", "nvme", &root).name(false), "nvme0");

    let dir = nvme_root(&[]);
    let root = Root::new(dir.path());
    assert_eq!(located("/dev/nvme0", "nvme", &root).name(false), "nvme0");
    let missing = Root::new("/nonexistent");
    assert_eq!(located("/dev/nvme0", "nvme", &missing).name(false), "nvme0");
}

#[test]
fn raid_card_disks_are_named_by_their_number_on_the_card() {
    let root = Root::new("/nonexistent");
    let megaraid = located("/dev/bus/0", "sat+megaraid,18", &root);
    assert_eq!(
        megaraid,
        Location::Card {
            host: 0,
            numbers: vec![18],
        }
    );
    assert_eq!(megaraid.name(false), "RAID 卡 18 号盘");
    assert_eq!(megaraid.name(true), "RAID 卡 host0 的 18 号盘");

    let sas = located("/dev/bus/12", "megaraid,5", &root);
    assert_eq!(sas.name(true), "RAID 卡 host12 的 5 号盘");

    let sssraid = located("/dev/bsg/sssraid3", "sssraid,0,5", &root);
    assert_eq!(
        sssraid,
        Location::Card {
            host: 3,
            numbers: vec![0, 5],
        }
    );
    assert_eq!(sssraid.name(false), "RAID 卡 0:5 号盘");
}

#[test]
fn a_raid_type_without_numbers_falls_back_to_the_device_name() {
    let root = Root::new("/nonexistent");
    let odd = located("/dev/bus/0", "megaraid,x", &root);
    assert_eq!(odd, Location::Named("bus/0".to_owned()));
}

#[test]
fn named_disks_come_first_by_name_then_raid_card_disks_by_card_and_number() {
    let card = |host, numbers: &[u32]| Location::Card {
        host,
        numbers: numbers.to_vec(),
    };
    let named = |name: &str| Location::Named(name.to_owned());
    let mut locations = vec![
        card(1, &[5]),
        card(0, &[21]),
        named("sdb"),
        card(0, &[18]),
        named("sdaa"),
        named("sda"),
        named("nvme0n1"),
    ];
    locations.sort();
    assert_eq!(
        locations,
        [
            named("nvme0n1"),
            named("sda"),
            named("sdaa"),
            named("sdb"),
            card(0, &[18]),
            card(0, &[21]),
            card(1, &[5]),
        ],
        "plain string order, as 系统基础信息 lists its disks"
    );
}
