//! Disks and filesystems, checked against `lsblk` and `df` from capture time, and against a
//! synthetic host for layouts the captures do not have.

mod common;

use std::fs;
use std::path::Path;

use bench_host::Root;
use bench_host::storage;
use bench_host::storage::Disk;
use bench_host::storage::Filesystem;
use bench_host::storage::Use;

/// Size and model of a disk as `lsblk -b -o NAME,KNAME,TYPE,SIZE,ROTA,TRAN,MODEL,...` printed
/// them at capture time.
fn lsblk(fixture: &str, disk: &str) -> (u64, String) {
    let text = common::truth(fixture, "lsblk");
    let header = text.lines().next().expect("header");
    let model_column = header.find("MODEL").expect("MODEL column");
    let fstype_column = header.find("FSTYPE").expect("FSTYPE column");
    let line = text
        .lines()
        .find(|line| line.starts_with(&format!("{disk} ")))
        .expect("disk line");
    let size = line
        .split_whitespace()
        .nth(3)
        .expect("size")
        .parse()
        .expect("number");
    let model = line
        .get(model_column..fstype_column.min(line.len()))
        .unwrap_or_default()
        .trim()
        .to_owned();
    (size, model)
}

#[test]
fn a_kvm_guest_has_one_virtio_system_disk() {
    let name = "kvm-debian13-x86_64";
    let storage = storage::read(&common::fixture(name), common::df(name));
    let (size, _) = lsblk(name, "vda");

    // The DVD drive (sr0) is not a disk; the guest's rotation flag is the hypervisor's.
    assert_eq!(
        storage.disks,
        [Disk {
            name: "vda".to_owned(),
            interface: "virtio",
            size,
            rotational: Some(true),
            model: None,
            uses: vec![Use::Mount("/".to_owned())],
        }]
    );
    assert_eq!(
        storage.filesystems,
        [Filesystem {
            mount_point: "/".to_owned(),
            fs_type: "ext4".to_owned(),
            devices: vec![(254, 1)],
            zfs_pool: None,
            disks: vec!["vda".to_owned()],
            space: common::df(name)("/"),
        }]
    );
}

#[test]
fn a_dedicated_server_has_a_system_ssd_and_a_zfs_pool() {
    let name = "dedicated-pve-x86_64";
    let storage = storage::read(&common::fixture(name), common::df(name));

    // ZFS volumes (the guests' disks), LVM volumes and loop devices are not disks.
    let names: Vec<&str> = storage
        .disks
        .iter()
        .map(|disk| disk.name.as_str())
        .collect();
    assert_eq!(names, ["sda", "sdb", "sdc", "sdd"]);
    for disk in &storage.disks {
        let (size, model) = lsblk(name, &disk.name);
        assert_eq!(disk.size, size, "{}", disk.name);
        assert_eq!(disk.model.as_deref(), Some(model.as_str()), "{}", disk.name);
        assert_eq!(disk.interface, "SATA", "{}", disk.name);
        assert_eq!(
            disk.rotational,
            Some(false),
            "{}: ATA rotation rate 0",
            disk.name
        );
    }
    assert_eq!(
        storage.disks[0].uses,
        [Use::Mount("/".to_owned()), Use::Swap],
        "/ and swap are LVM volumes on sda3"
    );
    for disk in &storage.disks[1..] {
        assert_eq!(
            disk.uses,
            [Use::ZfsPool("Volume1".to_owned())],
            "{}",
            disk.name
        );
    }

    // /boot/efi, the FUSE mounts and tmpfs are left out; the pool is one row.
    let rows: Vec<(&str, &str, Option<&str>, &[String])> = storage
        .filesystems
        .iter()
        .map(|filesystem| {
            (
                filesystem.mount_point.as_str(),
                filesystem.fs_type.as_str(),
                filesystem.zfs_pool.as_deref(),
                filesystem.disks.as_slice(),
            )
        })
        .collect();
    let sda = ["sda".to_owned()];
    let pool = ["sdb", "sdc", "sdd"].map(str::to_owned);
    assert_eq!(
        rows,
        [
            ("/", "ext4", None, &sda[..]),
            ("/Volume1", "zfs", Some("Volume1"), &pool[..]),
        ]
    );
    assert_eq!(storage.filesystems[1].space, common::df(name)("/Volume1"));
}

#[test]
fn partitions_raid_and_btrfs_subvolumes_lead_to_their_disks() {
    let dir = common::host((
        &[
            (
                "/proc/self/mountinfo",
                "20 1 259:1 / / rw - ext4 /dev/nvme0n1p1 rw\n\
                 21 20 9:0 / /data rw - xfs /dev/md0 rw\n\
                 22 20 0:40 /@srv /srv rw - btrfs /dev/sdc rw\n\
                 23 22 0:40 /@home /srv/home rw - btrfs /dev/sdc rw\n\
                 24 20 8:49 /containers/x/hosts /etc/hosts rw - ext4 /dev/sdd1 rw\n\
                 25 20 8:65 / /mnt/my\\040disk rw - ext4 /dev/sde1 rw\n\
                 26 20 0:30 / /tmp rw - tmpfs tmpfs rw\n\
                 27 20 7:0 / /snap/core rw - squashfs /dev/loop0 ro\n",
            ),
            (
                "/proc/swaps",
                "Filename\tType\tSize\tUsed\tPriority\n\
                 /dev/sda2 partition 1048572 0 -2\n\
                 /swapfile file 1048572 0 -3\n",
            ),
            (
                "/run/udev/data/b8:97",
                "E:ID_FS_TYPE=zfs_member\nE:ID_FS_LABEL=tank\n",
            ),
            ("/sys/class/block/sda/device/vendor", "ATA     \n"),
        ],
        &[],
    ));
    let gib = 2 * 1024 * 1024;
    for (name, dev, partition, slaves) in [
        ("nvme0n1", "259:0", false, &[][..]),
        ("nvme0n1p1", "259:1", true, &[]),
        ("sda", "8:0", false, &[]),
        ("sda1", "8:1", true, &[]),
        ("sda2", "8:2", true, &[]),
        ("sdb", "8:16", false, &[]),
        ("sdb1", "8:17", true, &[]),
        ("md0", "9:0", false, &["sda1", "sdb1"]),
        ("sdc", "8:32", false, &[]),
        ("sdd", "8:48", false, &[]),
        ("sdd1", "8:49", true, &[]),
        ("sde", "8:64", false, &[]),
        ("sde1", "8:65", true, &[]),
        ("sdf", "8:80", false, &[]),
        ("sdg", "8:96", false, &[]),
        ("sdg1", "8:97", true, &[]),
        ("loop0", "7:0", false, &[]),
        ("zd0", "230:0", false, &[]),
        ("sr0", "11:0", false, &[]),
    ] {
        common::block(&dir, name, dev, gib, partition, slaves);
    }
    let storage = storage::read(&Root::new(dir.path()), |_| None);

    let disks: Vec<(&str, &str, Vec<Use>)> = storage
        .disks
        .iter()
        .map(|disk| (disk.name.as_str(), disk.interface, disk.uses.clone()))
        .collect();
    let mount = |point: &str| Use::Mount(point.to_owned());
    assert_eq!(
        disks,
        [
            ("nvme0n1", "NVMe", vec![mount("/")]),
            ("sda", "SATA", vec![mount("/data"), Use::Swap]),
            ("sdb", "SCSI", vec![mount("/data")]),
            ("sdc", "SCSI", vec![mount("/srv")]),
            // Only bind-mounted as a file: not a filesystem of this host.
            ("sdd", "SCSI", vec![]),
            ("sde", "SCSI", vec![mount("/mnt/my disk")]),
            ("sdf", "SCSI", vec![]),
            ("sdg", "SCSI", vec![Use::ZfsPool("tank".to_owned())]),
        ]
    );
    let filesystems: Vec<(&str, &[String])> = storage
        .filesystems
        .iter()
        .map(|filesystem| (filesystem.mount_point.as_str(), filesystem.disks.as_slice()))
        .collect();
    let names = |disks: &[&str]| {
        disks
            .iter()
            .map(|disk| (*disk).to_owned())
            .collect::<Vec<_>>()
    };
    let (nvme, raid, btrfs, escaped) = (
        names(&["nvme0n1"]),
        names(&["sda", "sdb"]),
        names(&["sdc"]),
        names(&["sde"]),
    );
    assert_eq!(
        filesystems,
        [
            ("/", &nvme[..]),
            ("/data", &raid[..]),
            ("/srv", &btrfs[..]),
            ("/mnt/my disk", &escaped[..]),
        ]
    );
    assert_eq!(
        storage.filesystems[2].devices,
        [(0, 40)],
        "btrfs subvolumes share one row"
    );
}

#[test]
fn a_container_shows_its_own_root_filesystem() {
    let name = "docker-debian13-aarch64";
    let root =
        storage::root_filesystem(&common::fixture(name), common::df(name)).expect("a root mount");
    assert_eq!(root.mount_point, "/");
    assert_eq!(root.fs_type, "overlay");
    assert_eq!(root.space, common::df(name)("/"));
    assert!(
        root.disks.is_empty(),
        "the disks seen in a container are the host's"
    );
}

#[test]
fn statvfs_and_device_of_work_on_this_host() {
    let space = storage::statvfs(".").expect("statvfs of the current directory");
    assert!(space.used <= space.total && space.available <= space.total);
    assert!(storage::device_of(Path::new(".")).is_some());
    assert_eq!(storage::device_of(Path::new("/no/such/path")), None);
    assert!(fs::metadata(".").is_ok());
}
