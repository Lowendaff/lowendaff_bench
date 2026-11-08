//! Disks and filesystems: block devices from sysfs, mounts from mountinfo, ZFS pool
//! members from the udev database and sizes from statvfs.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use crate::Root;

/// A physical (or, in a virtual machine, virtual) disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    /// Kernel name, such as `sda`, `nvme0n1` or `vda`.
    pub name: String,
    /// Interface: `NVMe`, `SATA`, `SCSI`, `USB`, `virtio`, `Xen` or `MMC`.
    pub interface: &'static str,
    /// Size in bytes.
    pub size: u64,
    /// Whether the disk spins, as the kernel or the ATA rotation rate reports it. Inside
    /// a virtual machine this is whatever the hypervisor claims.
    pub rotational: Option<bool>,
    /// Model name, such as "Samsung SSD 860 EVO 250GB".
    pub model: Option<String>,
    /// What lives on the disk, in mount order.
    pub uses: Vec<Use>,
}

/// Something stored on a disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Use {
    /// A mounted filesystem, by mount point.
    Mount(String),
    /// Swap space.
    Swap,
    /// A member of a ZFS pool, by pool name.
    ZfsPool(String),
}

/// A mounted filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filesystem {
    /// Where it is mounted; the shortest mount point when it is mounted several times.
    pub mount_point: String,
    /// Filesystem type, such as `ext4` or `zfs`.
    pub fs_type: String,
    /// `major:minor` of every mount merged into this row, as in mountinfo: one per ZFS
    /// dataset of a pool, one for all mounts of a block device.
    pub devices: Vec<(u32, u32)>,
    /// The ZFS pool, for ZFS datasets.
    pub zfs_pool: Option<String>,
    /// The disks the filesystem is stored on.
    pub disks: Vec<String>,
    /// Size and usage from statvfs.
    pub space: Option<Space>,
}

/// Size and usage of a filesystem, in bytes, as `df` computes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    /// Total size.
    pub total: u64,
    /// Space in use.
    pub used: u64,
    /// Space available to unprivileged users.
    pub available: u64,
}

/// Disks and filesystems of a host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Storage {
    /// Disks in kernel name order.
    pub disks: Vec<Disk>,
    /// Filesystems in mount order.
    pub filesystems: Vec<Filesystem>,
}

/// Filesystem types that store data on disks; pseudo and network filesystems are left out.
const DISK_FILESYSTEMS: [&str; 15] = [
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "f2fs", "jfs", "reiserfs", "bcachefs", "nilfs2",
    "vfat", "exfat", "ntfs3", "simfs",
];

/// Block devices that are not disks: memory, loops, optical drives, network block devices,
/// ZFS volumes (the disks of guests on a hypervisor) and the logical layers of LVM, RAID
/// and caching, which are resolved to the disks below them.
const NOT_DISKS: [&str; 11] = [
    "loop", "ram", "zram", "sr", "fd", "nbd", "zd", "dm-", "md", "drbd", "bcache",
];

/// Reads the disks and filesystems under `root`. `space` gives the size of a mounted
/// filesystem by mount point; on a real host it calls statvfs.
pub fn read(root: &Root, space: impl Fn(&str) -> Option<Space>) -> Storage {
    let devices = block_devices(root);
    let mounts = mounts(root);

    let mut filesystems: Vec<Filesystem> = Vec::new();
    for mount in &mounts {
        if !DISK_FILESYSTEMS.contains(&mount.fs_type.as_str())
            || mount.mount_point == "/boot"
            || mount.mount_point.starts_with("/boot/")
            // A root other than `/` is a bind mount of part of a filesystem, except on
            // btrfs, where it names the mounted subvolume.
            || (mount.root != "/" && mount.fs_type != "btrfs")
        {
            continue;
        }
        let zfs_pool = (mount.fs_type == "zfs").then(|| {
            mount
                .source
                .split('/')
                .next()
                .unwrap_or_default()
                .to_owned()
        });
        // One row per filesystem: all mounts of a device (or datasets of a pool) are one.
        let existing = filesystems.iter_mut().find(|filesystem| match &zfs_pool {
            Some(pool) => filesystem.zfs_pool.as_ref() == Some(pool),
            None => filesystem.zfs_pool.is_none() && filesystem.devices.contains(&mount.device),
        });
        if let Some(existing) = existing {
            if mount.mount_point.len() < existing.mount_point.len() {
                existing.mount_point.clone_from(&mount.mount_point);
            }
            if !existing.devices.contains(&mount.device) {
                existing.devices.push(mount.device);
            }
            continue;
        }
        let disks = match &zfs_pool {
            Some(pool) => zfs_members(&devices, pool),
            None => mount_device(&devices, mount)
                .map(|device| physical(&devices, &device.name))
                .unwrap_or_default(),
        };
        filesystems.push(Filesystem {
            mount_point: mount.mount_point.clone(),
            fs_type: mount.fs_type.clone(),
            devices: vec![mount.device],
            zfs_pool,
            disks: disks.into_iter().collect(),
            space: None,
        });
    }
    for filesystem in &mut filesystems {
        filesystem.space = space(&filesystem.mount_point);
    }

    let mut uses: BTreeMap<String, Vec<Use>> = BTreeMap::new();
    for filesystem in &filesystems {
        for disk in &filesystem.disks {
            let entry = uses.entry(disk.clone()).or_default();
            let new = match &filesystem.zfs_pool {
                Some(pool) => Use::ZfsPool(pool.clone()),
                None => Use::Mount(filesystem.mount_point.clone()),
            };
            if !entry.contains(&new) {
                entry.push(new);
            }
        }
    }
    for name in swap_devices(root) {
        for disk in physical(&devices, &name) {
            let entry = uses.entry(disk).or_default();
            if !entry.contains(&Use::Swap) {
                entry.push(Use::Swap);
            }
        }
    }
    // Pool members whose datasets are not mounted still belong to the pool.
    for device in &devices {
        if let Some(pool) = device.zfs_pool() {
            for disk in physical(&devices, &device.name) {
                let entry = uses.entry(disk).or_default();
                let new = Use::ZfsPool(pool.to_owned());
                if !entry.contains(&new) {
                    entry.push(new);
                }
            }
        }
    }

    let disks = devices
        .iter()
        .filter(|device| device.is_disk())
        .map(|device| Disk {
            name: device.name.clone(),
            interface: device.interface(),
            size: device.size,
            rotational: device.rotational(),
            model: device.model(),
            uses: uses.remove(&device.name).unwrap_or_default(),
        })
        .collect();
    Storage { disks, filesystems }
}

/// The filesystem mounted at `/`, whatever its type: a container's root is often an
/// overlay or a ZFS dataset rather than a disk filesystem.
pub fn root_filesystem(root: &Root, space: impl Fn(&str) -> Option<Space>) -> Option<Filesystem> {
    // The last `/` mount is the one on top.
    let mount = mounts(root)
        .into_iter()
        .rev()
        .find(|mount| mount.mount_point == "/")?;
    Some(Filesystem {
        space: space("/"),
        zfs_pool: (mount.fs_type == "zfs").then(|| {
            mount
                .source
                .split('/')
                .next()
                .unwrap_or_default()
                .to_owned()
        }),
        devices: vec![mount.device],
        disks: Vec::new(),
        mount_point: mount.mount_point,
        fs_type: mount.fs_type,
    })
}

/// Size and usage of the filesystem at `path`, computed as `df` does.
#[must_use]
pub fn statvfs(path: &str) -> Option<Space> {
    let stat = rustix::fs::statvfs(path).ok()?;
    let block = stat.f_frsize;
    Some(Space {
        total: stat.f_blocks.saturating_mul(block),
        used: stat
            .f_blocks
            .saturating_sub(stat.f_bfree)
            .saturating_mul(block),
        available: stat.f_bavail.saturating_mul(block),
    })
}

/// `major:minor` of the filesystem holding `path`, comparable with mountinfo.
#[must_use]
pub fn device_of(path: &Path) -> Option<(u32, u32)> {
    let stat = rustix::fs::stat(path).ok()?;
    Some((
        rustix::fs::major(stat.st_dev),
        rustix::fs::minor(stat.st_dev),
    ))
}

/// What holds a filesystem's data, as far as a disk test is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemKind {
    /// tmpfs or ramfs: the data is in memory.
    Memory,
    /// A ZFS dataset.
    Zfs,
    /// Any other filesystem.
    Other,
}

impl FilesystemKind {
    /// Classifies the magic number that `statfs(2)` reports in `f_type`.
    #[must_use]
    pub fn from_magic(magic: i128) -> Self {
        match magic {
            // TMPFS_MAGIC and RAMFS_MAGIC from <linux/magic.h>.
            0x0102_1994 | 0x8584_58f6 => Self::Memory,
            // ZFS_SUPER_MAGIC from OpenZFS's include/sys/fs/zfs.h.
            0x2fc1_2fc1 => Self::Zfs,
            _ => Self::Other,
        }
    }
}

/// The kind of the filesystem holding `path`, from `statfs(2)`.
#[must_use]
pub fn filesystem_kind(path: &Path) -> Option<FilesystemKind> {
    let stat = rustix::fs::statfs(path).ok()?;
    // `f_type` is signed on some targets and unsigned on others; i128 holds every value.
    Some(FilesystemKind::from_magic(i128::from(stat.f_type)))
}

/// One entry of `/sys/class/block`.
#[derive(Debug)]
struct BlockDevice {
    name: String,
    dev: (u32, u32),
    size: u64,
    partition: bool,
    slaves: Vec<String>,
    rotational: Option<bool>,
    model: Option<String>,
    vendor: Option<String>,
    /// Device-mapper name, such as `pve-root`, for `/dev/mapper/<name>`.
    dm_name: Option<String>,
    udev: BTreeMap<String, String>,
}

impl BlockDevice {
    fn is_disk(&self) -> bool {
        !self.partition
            && self.size > 0
            && !NOT_DISKS.iter().any(|prefix| self.name.starts_with(prefix))
            // Per-path devices of multipath NVMe, such as nvme0c0n1.
            && !(self.name.starts_with("nvme") && self.name.contains('c'))
    }

    fn interface(&self) -> &'static str {
        let name = self.name.as_str();
        if name.starts_with("nvme") {
            "NVMe"
        } else if name.starts_with("vd") {
            "virtio"
        } else if name.starts_with("xvd") {
            "Xen"
        } else if name.starts_with("mmcblk") {
            "MMC"
        } else {
            match self.udev.get("ID_BUS").map(String::as_str) {
                Some("ata") => "SATA",
                Some("usb") => "USB",
                Some(_) => "SCSI",
                // libata reports the vendor "ATA" for SATA disks.
                None if self.vendor.as_deref() == Some("ATA") => "SATA",
                None => "SCSI",
            }
        }
    }

    fn rotational(&self) -> Option<bool> {
        match self.udev.get("ID_ATA_ROTATION_RATE_RPM") {
            Some(rpm) => Some(rpm != "0"),
            None => self.rotational,
        }
    }

    /// udev's full model name; sysfs truncates ATA models to 16 characters.
    fn model(&self) -> Option<String> {
        self.udev
            .get("ID_MODEL_ENC")
            .map(|encoded| decode_udev(encoded))
            .or_else(|| self.model.clone())
            .map(|model| model.trim().to_owned())
            .filter(|model| !model.is_empty())
    }

    fn zfs_pool(&self) -> Option<&str> {
        (self.udev.get("ID_FS_TYPE").map(String::as_str) == Some("zfs_member"))
            .then(|| self.udev.get("ID_FS_LABEL").map(String::as_str))
            .flatten()
    }
}

fn block_devices(root: &Root) -> Vec<BlockDevice> {
    let mut devices = Vec::new();
    for name in root.list("/sys/class/block") {
        let dir = format!("/sys/class/block/{name}");
        let Some(dev) = root
            .read_line(&format!("{dir}/dev"))
            .and_then(|dev| device_number(&dev))
        else {
            continue;
        };
        let udev = root
            .read(&format!("/run/udev/data/b{}:{}", dev.0, dev.1))
            .map(|text| udev_properties(&text))
            .unwrap_or_default();
        devices.push(BlockDevice {
            size: root
                .read_line(&format!("{dir}/size"))
                .and_then(|sectors| sectors.parse::<u64>().ok())
                .map_or(0, |sectors| sectors * 512),
            partition: root.exists(&format!("{dir}/partition")),
            slaves: root.list(&format!("{dir}/slaves")),
            rotational: root
                .read_line(&format!("{dir}/queue/rotational"))
                .map(|flag| flag == "1"),
            model: root.read_line(&format!("{dir}/device/model")),
            vendor: root.read_line(&format!("{dir}/device/vendor")),
            dm_name: root.read_line(&format!("{dir}/dm/name")),
            udev,
            dev,
            name,
        });
    }
    devices
}

/// The block device a mount is on: by device number, else by its source, because btrfs
/// reports an anonymous device number.
fn mount_device<'a>(devices: &'a [BlockDevice], mount: &Mount) -> Option<&'a BlockDevice> {
    if let Some(device) = devices.iter().find(|device| device.dev == mount.device) {
        return Some(device);
    }
    let name = mount.source.strip_prefix("/dev/")?;
    match name.strip_prefix("mapper/") {
        Some(mapped) => devices
            .iter()
            .find(|device| device.dm_name.as_deref() == Some(mapped)),
        None => devices.iter().find(|device| device.name == name),
    }
}

fn device_number(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once(':')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// `E:KEY=value` lines of a udev database entry.
fn udev_properties(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("E:")?.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

/// Decodes udev's `\xHH` escapes, such as `\x20` for a space.
fn decode_udev(text: &str) -> String {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'\\'
            && let [b'x', high, low, after @ ..] = tail
            && let Ok(hex) = std::str::from_utf8(&[*high, *low])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            bytes.push(byte);
            rest = after;
        } else {
            bytes.push(first);
            rest = tail;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The disks below a block device: partitions belong to their disk, and LVM or RAID
/// devices to the disks of their members.
fn physical(devices: &[BlockDevice], name: &str) -> BTreeSet<String> {
    let mut disks = BTreeSet::new();
    let Some(device) = devices.iter().find(|device| device.name == name) else {
        return disks;
    };
    if device.partition {
        if let Some(parent) = parent_of(devices, name) {
            disks.extend(physical(devices, parent));
        }
    } else if !device.slaves.is_empty() {
        for slave in &device.slaves {
            disks.extend(physical(devices, slave));
        }
    } else if device.is_disk() {
        disks.insert(device.name.clone());
    }
    disks
}

/// The device a partition belongs to. The kernel names partitions after their device,
/// adding `p` when the device name ends in a digit: `sda1`, `nvme0n1p1`, `mmcblk0p1`.
fn parent_of<'a>(devices: &'a [BlockDevice], partition: &str) -> Option<&'a str> {
    devices
        .iter()
        .filter(|device| !device.partition)
        .map(|device| device.name.as_str())
        .find(|parent| {
            let Some(rest) = partition.strip_prefix(parent) else {
                return false;
            };
            let number = if parent.ends_with(|c: char| c.is_ascii_digit()) {
                rest.strip_prefix('p').unwrap_or("")
            } else {
                rest
            };
            !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
        })
}

fn zfs_members(devices: &[BlockDevice], pool: &str) -> BTreeSet<String> {
    devices
        .iter()
        .filter(|device| device.zfs_pool() == Some(pool))
        .flat_map(|device| physical(devices, &device.name))
        .collect()
}

/// Kernel names of the block devices used for swap; swap files are left out.
fn swap_devices(root: &Root) -> Vec<String> {
    let Some(text) = root.read("/proc/swaps") else {
        return Vec::new();
    };
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let path = fields.next()?;
            (fields.next()? == "partition").then(|| path.rsplit('/').next().map(str::to_owned))?
        })
        .collect()
}

/// One line of `/proc/self/mountinfo`.
#[derive(Debug)]
struct Mount {
    device: (u32, u32),
    /// The directory of the filesystem that is mounted; `/` unless it is a bind mount or
    /// a btrfs subvolume.
    root: String,
    mount_point: String,
    fs_type: String,
    source: String,
}

fn mounts(root: &Root) -> Vec<Mount> {
    let Some(text) = root.read("/proc/self/mountinfo") else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            // id parent major:minor root mount-point options [optional...] - type source ...
            let (before, after) = line.split_once(" - ")?;
            let before: Vec<&str> = before.split(' ').collect();
            let mut after = after.split(' ');
            Some(Mount {
                device: device_number(before.get(2)?)?,
                root: unescape(before.get(3)?),
                mount_point: unescape(before.get(4)?),
                fs_type: after.next()?.to_owned(),
                source: unescape(after.next()?),
            })
        })
        .collect()
}

/// Undoes mountinfo's octal escapes, such as `\040` for a space.
fn unescape(field: &str) -> String {
    let mut bytes = Vec::with_capacity(field.len());
    let mut rest = field.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'\\'
            && let [a, b, c, after @ ..] = tail
            && let Ok(octal) = std::str::from_utf8(&[*a, *b, *c])
            && let Ok(byte) = u8::from_str_radix(octal, 8)
        {
            bytes.push(byte);
            rest = after;
        } else {
            bytes.push(first);
            rest = tail;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
