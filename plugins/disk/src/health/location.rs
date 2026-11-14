//! Where a disk sits, which decides its name and its place in the table.

use bench_host::Root;
use serde::Serialize;

use super::device::Device;

/// A disk with a kernel name, or one that smartctl reaches only through a RAID card. The
/// order is the table's: named disks first, by name, then RAID-card disks by card and number.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Location {
    /// A kernel name such as `sda` or `nvme0n1`, as 系统信息 shows it.
    Named(String),
    /// A disk on a RAID card.
    Card {
        /// The card's SCSI host number.
        host: u32,
        /// The disk on the card: MegaRAID's device id, or 3SNIC's enclosure and slot.
        numbers: Vec<u32>,
    },
}

impl Location {
    /// Where `device` sits. NVMe namespaces are counted under `/sys/class/nvme` in `root`.
    #[must_use]
    pub fn of(device: &Device, root: &Root) -> Self {
        if let Some(card) = card(device) {
            return card;
        }
        let name = device.name.strip_prefix("/dev/").unwrap_or(&device.name);
        if is_nvme_controller(name) {
            return Self::Named(nvme_name(name, root));
        }
        Self::Named(name.to_owned())
    }
}

/// A disk on a MegaRAID card (`megaraid,N` or `sat+megaraid,N` on `/dev/bus/<host>`) or a
/// 3SNIC card (`sssraid,E,S` on `/dev/bsg/sssraid<host>`).
fn card(device: &Device) -> Option<Location> {
    let (_, numbers) = device
        .kind
        .split_once("megaraid,")
        .or_else(|| device.kind.split_once("sssraid,"))?;
    let numbers: Option<Vec<u32>> = numbers.split(',').map(|n| n.parse().ok()).collect();
    let host = device
        .name
        .trim_start_matches(|c: char| !c.is_ascii_digit());
    Some(Location::Card {
        host: host.parse().ok()?,
        numbers: numbers?,
    })
}

/// `nvme0` to `nvme99`: what the scan lists for NVMe.
fn is_nvme_controller(name: &str) -> bool {
    name.strip_prefix("nvme")
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// The block device of a controller's only namespace, or the controller's own name when it
/// has several. `nvme_number_of_namespaces` cannot tell: it is the controller's maximum.
fn nvme_name(controller: &str, root: &Root) -> String {
    let mut namespaces = Vec::new();
    for entry in root.list(&format!("/sys/class/nvme/{controller}")) {
        if let Some(block) = namespace_block(&entry) {
            namespaces.push(block);
        }
    }
    match namespaces.as_slice() {
        [only] => only.clone(),
        _ => controller.to_owned(),
    }
}

/// The block device for an entry of `/sys/class/nvme/<controller>`: `nvme0n1` for itself,
/// and for a path of a multipath namespace such as `nvme0c0n1`. Other entries give `None`.
fn namespace_block(entry: &str) -> Option<String> {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    let (head, namespace) = entry.strip_prefix("nvme")?.split_once('n')?;
    let (subsystem, path) = match head.split_once('c') {
        Some((subsystem, path)) => (subsystem, Some(path)),
        None => (head, None),
    };
    let valid = digits(subsystem) && digits(namespace) && path.is_none_or(digits);
    valid.then(|| format!("nvme{subsystem}n{namespace}"))
}
